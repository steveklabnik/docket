use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::str::FromStr;
use uuid::Uuid;

/// Special release version for unscheduled/backlog items.
/// This release cannot be Released or Cancelled.
pub const UNSCHEDULED_RELEASE: &str = "unscheduled";

/// Release status in its lifecycle
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ReleaseStatus {
    /// Deciding what goes in this release
    Planning,
    /// Actively developing
    Active,
    /// Code freeze - scope locked
    Frozen,
    /// Shipped
    Released,
    /// Abandoned
    Cancelled,
}

impl ReleaseStatus {
    fn sort_order(&self) -> u8 {
        match self {
            ReleaseStatus::Active => 0,    // Active releases first
            ReleaseStatus::Frozen => 1,    // Then frozen (about to ship)
            ReleaseStatus::Planning => 2,  // Then planning
            ReleaseStatus::Released => 3,  // Then released
            ReleaseStatus::Cancelled => 4, // Cancelled last
        }
    }
}

impl Ord for ReleaseStatus {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.sort_order().cmp(&other.sort_order())
    }
}

impl PartialOrd for ReleaseStatus {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for ReleaseStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReleaseStatus::Planning => write!(f, "planning"),
            ReleaseStatus::Active => write!(f, "active"),
            ReleaseStatus::Frozen => write!(f, "frozen"),
            ReleaseStatus::Released => write!(f, "released"),
            ReleaseStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl FromStr for ReleaseStatus {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "planning" => Ok(ReleaseStatus::Planning),
            "active" => Ok(ReleaseStatus::Active),
            "frozen" | "freeze" => Ok(ReleaseStatus::Frozen),
            "released" | "shipped" => Ok(ReleaseStatus::Released),
            "cancelled" | "canceled" => Ok(ReleaseStatus::Cancelled),
            _ => Err(anyhow!("unknown release status: {}", s)),
        }
    }
}

/// Metadata for a release
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseMetadata {
    /// Semver version string (e.g., "0.3.0") - acts as ID
    pub version: String,
    /// Human-readable name (e.g., "Performance Release")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Current status in the release lifecycle
    pub status: ReleaseStatus,
    /// When the release was created
    pub created: DateTime<Utc>,
    /// Target release date
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_date: Option<DateTime<Utc>>,
    /// Actual release date (set when status becomes Released)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub released_date: Option<DateTime<Utc>>,
}

/// A release with its description
#[derive(Debug, Clone)]
pub struct Release {
    pub metadata: ReleaseMetadata,
    /// Release theme/notes - what this release is about
    pub description: String,
}

impl Release {
    pub fn version(&self) -> &str {
        &self.metadata.version
    }

    pub fn title(&self) -> Option<&str> {
        self.metadata.title.as_deref()
    }

    pub fn status(&self) -> &ReleaseStatus {
        &self.metadata.status
    }

    pub fn created(&self) -> DateTime<Utc> {
        self.metadata.created
    }

    pub fn target_date(&self) -> Option<DateTime<Utc>> {
        self.metadata.target_date
    }

    pub fn released_date(&self) -> Option<DateTime<Utc>> {
        self.metadata.released_date
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    /// Returns true if this is the special "unscheduled" release
    pub fn is_unscheduled(&self) -> bool {
        self.metadata.version == UNSCHEDULED_RELEASE
    }

    /// Returns a display name (title if set, otherwise version)
    pub fn display_name(&self) -> &str {
        self.metadata
            .title
            .as_deref()
            .unwrap_or(&self.metadata.version)
    }
}

/// Validate that a version string is valid semver.
/// The special "unscheduled" version is exempt from validation.
pub fn validate_version(version: &str) -> Result<()> {
    if version == UNSCHEDULED_RELEASE {
        return Ok(());
    }

    semver::Version::parse(version).with_context(|| {
        format!(
            "'{}' is not a valid semver version (e.g., '1.0.0', '0.3.0-beta.1')",
            version
        )
    })?;

    Ok(())
}

/// Parse a version string into a semver Version.
/// Returns None for the special "unscheduled" version.
pub fn parse_version(version: &str) -> Option<semver::Version> {
    if version == UNSCHEDULED_RELEASE {
        return None;
    }
    semver::Version::parse(version).ok()
}

// ============================================================================
// Release Event System
// ============================================================================

/// Event data for release events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "data")]
pub enum ReleaseEventData {
    /// Release created
    Created {
        version: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default)]
        description: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target_date: Option<DateTime<Utc>>,
    },
    /// Release status changed
    StatusChanged {
        from: ReleaseStatus,
        to: ReleaseStatus,
    },
    /// Release updated (title, description, target_date)
    Updated {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target_date: Option<Option<DateTime<Utc>>>,
    },
    /// Release shipped at a specific date
    ReleasedAt { date: DateTime<Utc> },
}

/// Current event schema version for release events.
pub const CURRENT_RELEASE_EVENT_VERSION: u32 = 1;

/// Default version for events that don't have a version field.
fn default_version() -> u32 {
    1
}

/// A release event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseEvent {
    /// Schema version for this event
    #[serde(default = "default_version")]
    pub version: u32,
    /// Unique event ID
    pub id: String,
    /// Release version this event belongs to
    pub release_version: String,
    /// When this event occurred
    pub timestamp: DateTime<Utc>,
    /// The event data
    #[serde(flatten)]
    pub data: ReleaseEventData,
    /// Actor (hostname) that created this event
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
}

impl ReleaseEvent {
    pub fn new(release_version: String, data: ReleaseEventData) -> Self {
        ReleaseEvent {
            version: CURRENT_RELEASE_EVENT_VERSION,
            id: Uuid::new_v4().to_string(),
            release_version,
            timestamp: Utc::now(),
            data,
            actor: whoami::fallible::hostname().ok(),
        }
    }

    pub fn created(
        version: String,
        title: Option<String>,
        description: String,
        target_date: Option<DateTime<Utc>>,
    ) -> Self {
        Self::new(
            version.clone(),
            ReleaseEventData::Created {
                version,
                title,
                description,
                target_date,
            },
        )
    }

    pub fn status_changed(release_version: String, from: ReleaseStatus, to: ReleaseStatus) -> Self {
        Self::new(
            release_version,
            ReleaseEventData::StatusChanged { from, to },
        )
    }

    pub fn updated(
        release_version: String,
        title: Option<String>,
        description: Option<String>,
        target_date: Option<Option<DateTime<Utc>>>,
    ) -> Self {
        Self::new(
            release_version,
            ReleaseEventData::Updated {
                title,
                description,
                target_date,
            },
        )
    }

    pub fn released_at(release_version: String, date: DateTime<Utc>) -> Self {
        Self::new(release_version, ReleaseEventData::ReleasedAt { date })
    }
}

/// Append an event to a release's JSONL file
pub fn append_release_event(path: &Path, event: &ReleaseEvent) -> Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;

    let json = serde_json::to_string(event)?;
    writeln!(file, "{}", json)?;
    Ok(())
}

/// Read all events from a release's JSONL file
pub fn read_release_events(path: &Path) -> Result<Vec<ReleaseEvent>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file =
        fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();

    for (line_num, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("failed to read line {}", line_num + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        let event: ReleaseEvent = serde_json::from_str(&line)
            .with_context(|| format!("failed to parse event on line {}", line_num + 1))?;
        events.push(event);
    }

    // Sort by timestamp to ensure correct replay order
    events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

    Ok(events)
}

/// Derive current release state by replaying events
pub fn derive_release(events: &[ReleaseEvent]) -> Result<Release> {
    if events.is_empty() {
        return Err(anyhow!("no events to derive release from"));
    }

    // Find the Created event to get initial state
    let created = events
        .iter()
        .find(|e| matches!(e.data, ReleaseEventData::Created { .. }))
        .ok_or_else(|| anyhow!("no Created event found"))?;

    let (initial_version, initial_title, initial_description, initial_target_date) =
        match &created.data {
            ReleaseEventData::Created {
                version,
                title,
                description,
                target_date,
            } => (
                version.clone(),
                title.clone(),
                description.clone(),
                *target_date,
            ),
            _ => unreachable!(),
        };

    let mut release = Release {
        metadata: ReleaseMetadata {
            version: initial_version,
            title: initial_title,
            status: ReleaseStatus::Planning,
            created: created.timestamp,
            target_date: initial_target_date,
            released_date: None,
        },
        description: initial_description,
    };

    // Replay all events in order
    for event in events {
        match &event.data {
            ReleaseEventData::Created { .. } => {
                // Already handled above
            }
            ReleaseEventData::StatusChanged { to, .. } => {
                release.metadata.status = to.clone();
            }
            ReleaseEventData::Updated {
                title,
                description,
                target_date,
            } => {
                if let Some(t) = title {
                    release.metadata.title = Some(t.clone());
                }
                if let Some(d) = description {
                    release.description = d.clone();
                }
                if let Some(td) = target_date {
                    release.metadata.target_date = *td;
                }
            }
            ReleaseEventData::ReleasedAt { date } => {
                release.metadata.released_date = Some(*date);
            }
        }
    }

    Ok(release)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn make_event(
        release_version: &str,
        data: ReleaseEventData,
        timestamp: DateTime<Utc>,
    ) -> ReleaseEvent {
        ReleaseEvent {
            version: CURRENT_RELEASE_EVENT_VERSION,
            id: "test-event-id".to_string(),
            release_version: release_version.to_string(),
            timestamp,
            data,
            actor: Some("test".to_string()),
        }
    }

    #[test]
    fn validate_version_valid_semver() {
        assert!(validate_version("1.0.0").is_ok());
        assert!(validate_version("0.3.0").is_ok());
        assert!(validate_version("1.0.0-beta.1").is_ok());
        assert!(validate_version("2.0.0-rc.1+build.123").is_ok());
    }

    #[test]
    fn validate_version_invalid_semver() {
        assert!(validate_version("1.0").is_err());
        assert!(validate_version("v1.0.0").is_err());
        assert!(validate_version("latest").is_err());
        assert!(validate_version("").is_err());
    }

    #[test]
    fn validate_version_unscheduled_allowed() {
        assert!(validate_version(UNSCHEDULED_RELEASE).is_ok());
    }

    #[test]
    fn parse_version_valid() {
        let v = parse_version("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
    }

    #[test]
    fn parse_version_unscheduled_returns_none() {
        assert!(parse_version(UNSCHEDULED_RELEASE).is_none());
    }

    #[test]
    fn release_status_display() {
        assert_eq!(ReleaseStatus::Planning.to_string(), "planning");
        assert_eq!(ReleaseStatus::Active.to_string(), "active");
        assert_eq!(ReleaseStatus::Frozen.to_string(), "frozen");
        assert_eq!(ReleaseStatus::Released.to_string(), "released");
        assert_eq!(ReleaseStatus::Cancelled.to_string(), "cancelled");
    }

    #[test]
    fn release_status_from_str() {
        assert_eq!(
            ReleaseStatus::from_str("planning").unwrap(),
            ReleaseStatus::Planning
        );
        assert_eq!(
            ReleaseStatus::from_str("active").unwrap(),
            ReleaseStatus::Active
        );
        assert_eq!(
            ReleaseStatus::from_str("frozen").unwrap(),
            ReleaseStatus::Frozen
        );
        assert_eq!(
            ReleaseStatus::from_str("freeze").unwrap(),
            ReleaseStatus::Frozen
        );
        assert_eq!(
            ReleaseStatus::from_str("released").unwrap(),
            ReleaseStatus::Released
        );
        assert_eq!(
            ReleaseStatus::from_str("shipped").unwrap(),
            ReleaseStatus::Released
        );
        assert_eq!(
            ReleaseStatus::from_str("cancelled").unwrap(),
            ReleaseStatus::Cancelled
        );
    }

    #[test]
    fn release_status_ordering() {
        assert!(ReleaseStatus::Active < ReleaseStatus::Frozen);
        assert!(ReleaseStatus::Frozen < ReleaseStatus::Planning);
        assert!(ReleaseStatus::Planning < ReleaseStatus::Released);
        assert!(ReleaseStatus::Released < ReleaseStatus::Cancelled);
    }

    #[test]
    fn derive_release_from_created_event() {
        let events = vec![make_event(
            "1.0.0",
            ReleaseEventData::Created {
                version: "1.0.0".to_string(),
                title: Some("Initial Release".to_string()),
                description: "First release".to_string(),
                target_date: None,
            },
            ts(1000),
        )];

        let release = derive_release(&events).unwrap();

        assert_eq!(release.version(), "1.0.0");
        assert_eq!(release.title(), Some("Initial Release"));
        assert_eq!(release.description(), "First release");
        assert_eq!(release.status(), &ReleaseStatus::Planning);
    }

    #[test]
    fn derive_release_with_status_changes() {
        let events = vec![
            make_event(
                "1.0.0",
                ReleaseEventData::Created {
                    version: "1.0.0".to_string(),
                    title: None,
                    description: "".to_string(),
                    target_date: None,
                },
                ts(1000),
            ),
            make_event(
                "1.0.0",
                ReleaseEventData::StatusChanged {
                    from: ReleaseStatus::Planning,
                    to: ReleaseStatus::Active,
                },
                ts(2000),
            ),
        ];

        let release = derive_release(&events).unwrap();
        assert_eq!(release.status(), &ReleaseStatus::Active);
    }

    #[test]
    fn derive_release_with_updates() {
        let events = vec![
            make_event(
                "1.0.0",
                ReleaseEventData::Created {
                    version: "1.0.0".to_string(),
                    title: None,
                    description: "Original".to_string(),
                    target_date: None,
                },
                ts(1000),
            ),
            make_event(
                "1.0.0",
                ReleaseEventData::Updated {
                    title: Some("New Title".to_string()),
                    description: Some("Updated description".to_string()),
                    target_date: None,
                },
                ts(2000),
            ),
        ];

        let release = derive_release(&events).unwrap();
        assert_eq!(release.title(), Some("New Title"));
        assert_eq!(release.description(), "Updated description");
    }

    #[test]
    fn release_is_unscheduled() {
        let release = Release {
            metadata: ReleaseMetadata {
                version: UNSCHEDULED_RELEASE.to_string(),
                title: None,
                status: ReleaseStatus::Planning,
                created: Utc::now(),
                target_date: None,
                released_date: None,
            },
            description: String::new(),
        };
        assert!(release.is_unscheduled());

        let release2 = Release {
            metadata: ReleaseMetadata {
                version: "1.0.0".to_string(),
                title: None,
                status: ReleaseStatus::Planning,
                created: Utc::now(),
                target_date: None,
                released_date: None,
            },
            description: String::new(),
        };
        assert!(!release2.is_unscheduled());
    }
}
