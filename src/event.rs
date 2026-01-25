use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use uuid::Uuid;

use crate::change::{Change, ChangeMetadata, ChangelogType, Priority, Status};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "data")]
pub enum EventData {
    Created {
        title: String,
        priority: Priority,
        body: String,
        /// If true, this change is an epic (parent container for ordered steps)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        is_epic: Option<bool>,
        /// Parent change ID (unified model). Takes precedence over parent_epic.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent: Option<String>,
        /// Legacy field: parent epic ID. Alias for `parent` for backward compatibility.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_epic: Option<String>,
    },
    StatusChanged {
        from: Status,
        to: Status,
    },
    /// Change blocked on external dependency
    Blocked {
        from: Status,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// Change unblocked and back to in progress
    Unblocked,
    /// Change paused (intentionally set aside)
    Paused {
        from: Status,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// Change resumed from paused state
    Resumed,
    Updated {
        title: Option<String>,
        body: Option<String>,
    },
    PriorityChanged {
        from: Priority,
        to: Priority,
    },
    /// Deprecated: linked changes are no longer used.
    /// Kept for backwards compatibility with existing event logs.
    #[serde(rename = "change_linked")]
    ChangeLinked {
        change_id: String,
    },
    /// Set the changelog type for a change
    ChangelogTypeSet {
        changelog_type: ChangelogType,
    },
    /// Add a version to a change (can have multiple versions for backports)
    VersionAdded {
        version: String,
    },
    /// Remove a version from a change
    VersionRemoved {
        version: String,
    },
    /// Add a tag to a change
    TagAdded {
        tag: String,
    },
    /// Remove a tag from a change
    TagRemoved {
        tag: String,
    },
    /// Mark a change as blocked by another change (inter-change dependency)
    DependencyAdded {
        /// The change ID that blocks this change
        blocked_by: String,
    },
    /// Remove a dependency on another change
    DependencyRemoved {
        /// The change ID that was blocking this change
        blocked_by: String,
    },
    /// Change the parent of a change (reparenting)
    ParentChanged {
        /// Previous parent ID (None if was a top-level change)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        old_parent: Option<String>,
        /// New parent ID (None to make it a top-level change)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        new_parent: Option<String>,
    },
    /// Append content to the scratchpad (working notes)
    ScratchpadAppended {
        /// Content to append to the scratchpad
        content: String,
    },
}

/// Current event schema version.
///
/// Increment this when making breaking changes to the event format.
/// See `docs/schema-versioning.md` for the upgrade policy.
pub const CURRENT_EVENT_VERSION: u32 = 1;

/// Default version for events that don't have a version field.
/// This handles backwards compatibility with events created before versioning was added.
fn default_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Schema version for this event. Used for forward/backward compatibility.
    /// Events without a version field are assumed to be version 1.
    #[serde(default = "default_version")]
    pub version: u32,
    pub id: String,
    #[serde(alias = "bug_id")]
    pub change_id: String,
    pub timestamp: DateTime<Utc>,
    #[serde(flatten)]
    pub data: EventData,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
}

impl Event {
    pub fn new(change_id: String, data: EventData) -> Self {
        Event {
            version: CURRENT_EVENT_VERSION,
            id: Uuid::new_v4().to_string(),
            change_id,
            timestamp: Utc::now(),
            data,
            actor: whoami::fallible::hostname().ok(),
        }
    }

    pub fn created(change_id: String, title: String, priority: Priority, body: String) -> Self {
        Self::new(
            change_id,
            EventData::Created {
                title,
                priority,
                body,
                is_epic: None,
                parent: None,
                parent_epic: None,
            },
        )
    }

    /// Create a new change with a parent (sub-change of another change)
    pub fn created_with_parent(
        change_id: String,
        title: String,
        priority: Priority,
        body: String,
        parent: String,
    ) -> Self {
        Self::new(
            change_id,
            EventData::Created {
                title,
                priority,
                body,
                is_epic: None,
                parent: Some(parent),
                parent_epic: None,
            },
        )
    }

    pub fn epic_created(
        change_id: String,
        title: String,
        priority: Priority,
        body: String,
    ) -> Self {
        Self::new(
            change_id,
            EventData::Created {
                title,
                priority,
                body,
                is_epic: Some(true),
                parent: None,
                parent_epic: None,
            },
        )
    }

    /// Legacy method for creating a child of an epic
    pub fn child_created(
        change_id: String,
        title: String,
        priority: Priority,
        body: String,
        parent_epic: String,
    ) -> Self {
        // Use the new parent field instead of parent_epic
        Self::created_with_parent(change_id, title, priority, body, parent_epic)
    }

    pub fn parent_changed(
        change_id: String,
        old_parent: Option<String>,
        new_parent: Option<String>,
    ) -> Self {
        Self::new(
            change_id,
            EventData::ParentChanged {
                old_parent,
                new_parent,
            },
        )
    }

    pub fn scratchpad_appended(change_id: String, content: String) -> Self {
        Self::new(change_id, EventData::ScratchpadAppended { content })
    }

    pub fn status_changed(change_id: String, from: Status, to: Status) -> Self {
        Self::new(change_id, EventData::StatusChanged { from, to })
    }

    pub fn updated(change_id: String, title: Option<String>, body: Option<String>) -> Self {
        Self::new(change_id, EventData::Updated { title, body })
    }

    pub fn priority_changed(change_id: String, from: Priority, to: Priority) -> Self {
        Self::new(change_id, EventData::PriorityChanged { from, to })
    }

    pub fn changelog_type_set(change_id: String, changelog_type: ChangelogType) -> Self {
        Self::new(change_id, EventData::ChangelogTypeSet { changelog_type })
    }

    pub fn version_added(change_id: String, version: String) -> Self {
        Self::new(change_id, EventData::VersionAdded { version })
    }

    pub fn version_removed(change_id: String, version: String) -> Self {
        Self::new(change_id, EventData::VersionRemoved { version })
    }

    pub fn tag_added(change_id: String, tag: String) -> Self {
        Self::new(change_id, EventData::TagAdded { tag })
    }

    pub fn tag_removed(change_id: String, tag: String) -> Self {
        Self::new(change_id, EventData::TagRemoved { tag })
    }

    pub fn blocked(change_id: String, from: Status, reason: Option<String>) -> Self {
        Self::new(change_id, EventData::Blocked { from, reason })
    }

    pub fn unblocked(change_id: String) -> Self {
        Self::new(change_id, EventData::Unblocked)
    }

    pub fn paused(change_id: String, from: Status, reason: Option<String>) -> Self {
        Self::new(change_id, EventData::Paused { from, reason })
    }

    pub fn resumed(change_id: String) -> Self {
        Self::new(change_id, EventData::Resumed)
    }

    pub fn dependency_added(change_id: String, blocked_by: String) -> Self {
        Self::new(change_id, EventData::DependencyAdded { blocked_by })
    }

    pub fn dependency_removed(change_id: String, blocked_by: String) -> Self {
        Self::new(change_id, EventData::DependencyRemoved { blocked_by })
    }
}

/// Append an event to a change's JSONL file
pub fn append_event(path: &Path, event: &Event) -> Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;

    let json = serde_json::to_string(event)?;
    writeln!(file, "{}", json)?;
    Ok(())
}

/// Read all events from a change's JSONL file
pub fn read_events(path: &Path) -> Result<Vec<Event>> {
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
        let event: Event = serde_json::from_str(&line)
            .with_context(|| format!("failed to parse event on line {}", line_num + 1))?;
        events.push(event);
    }

    // Sort by timestamp to ensure correct replay order
    events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

    Ok(events)
}

/// Derive current change state by replaying events
pub fn derive_change(events: &[Event]) -> Result<Change> {
    if events.is_empty() {
        return Err(anyhow!("no events to derive change from"));
    }

    // Find the Created event to get initial state
    let created = events
        .iter()
        .find(|e| matches!(e.data, EventData::Created { .. }))
        .ok_or_else(|| anyhow!("no Created event found"))?;

    let (
        initial_title,
        initial_priority,
        initial_body,
        initial_is_epic,
        initial_parent,
        initial_parent_epic,
    ) = match &created.data {
        EventData::Created {
            title,
            priority,
            body,
            is_epic,
            parent,
            parent_epic,
        } => (
            title.clone(),
            priority.clone(),
            body.clone(),
            *is_epic,
            parent.clone(),
            parent_epic.clone(),
        ),
        _ => unreachable!(),
    };

    let mut change = Change {
        metadata: ChangeMetadata {
            id: created.change_id.clone(),
            title: initial_title,
            status: Status::Draft,
            priority: initial_priority,
            created: created.timestamp,
            changelog_type: None,
            versions: Vec::new(),
            tags: HashSet::new(),
            is_epic: initial_is_epic,
            parent: initial_parent,
            parent_epic: initial_parent_epic,
            scratchpad: String::new(),
            blocked_reason: None,
            paused_reason: None,
            blocked_by: HashSet::new(),
        },
        body: initial_body,
    };

    // Replay all events in order
    for event in events {
        match &event.data {
            EventData::Created { .. } => {
                // Already handled above
            }
            EventData::StatusChanged { to, .. } => {
                change.metadata.status = to.clone();
            }
            EventData::Updated { title, body } => {
                if let Some(t) = title {
                    change.metadata.title = t.clone();
                }
                if let Some(b) = body {
                    change.body = b.clone();
                }
            }
            EventData::PriorityChanged { to, .. } => {
                change.metadata.priority = to.clone();
            }
            EventData::ChangeLinked { .. } => {
                // Deprecated: linked changes are no longer used, ignore
            }
            EventData::ChangelogTypeSet { changelog_type } => {
                change.metadata.changelog_type = Some(changelog_type.clone());
            }
            EventData::VersionAdded { version } => {
                if !change.metadata.versions.contains(version) {
                    change.metadata.versions.push(version.clone());
                }
            }
            EventData::VersionRemoved { version } => {
                change.metadata.versions.retain(|v| v != version);
            }
            EventData::TagAdded { tag } => {
                change.metadata.tags.insert(tag.clone());
            }
            EventData::TagRemoved { tag } => {
                change.metadata.tags.remove(tag);
            }
            EventData::Blocked { reason, .. } => {
                change.metadata.status = Status::Blocked;
                change.metadata.blocked_reason = reason.clone();
            }
            EventData::Unblocked => {
                change.metadata.status = Status::InProgress;
                change.metadata.blocked_reason = None;
            }
            EventData::Paused { reason, .. } => {
                change.metadata.status = Status::Paused;
                change.metadata.paused_reason = reason.clone();
            }
            EventData::Resumed => {
                change.metadata.status = Status::InProgress;
                change.metadata.paused_reason = None;
            }
            EventData::DependencyAdded { blocked_by } => {
                change.metadata.blocked_by.insert(blocked_by.clone());
            }
            EventData::DependencyRemoved { blocked_by } => {
                change.metadata.blocked_by.remove(blocked_by);
            }
            EventData::ParentChanged {
                old_parent: _,
                new_parent,
            } => {
                change.metadata.parent = new_parent.clone();
                // Clear legacy field when using new model
                change.metadata.parent_epic = None;
            }
            EventData::ScratchpadAppended { content } => {
                if !change.metadata.scratchpad.is_empty() {
                    change.metadata.scratchpad.push_str("\n\n");
                }
                change.metadata.scratchpad.push_str(content);
            }
        }
    }

    Ok(change)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn make_event(change_id: &str, data: EventData, timestamp: DateTime<Utc>) -> Event {
        Event {
            version: CURRENT_EVENT_VERSION,
            id: "test-event-id".to_string(),
            change_id: change_id.to_string(),
            timestamp,
            data,
            actor: Some("test".to_string()),
        }
    }

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    #[test]
    fn derive_change_from_created_event() {
        let events = vec![make_event(
            "abc1",
            EventData::Created {
                title: "Test Change".to_string(),
                priority: Priority::High,
                body: "Change body".to_string(),
                is_epic: None,
                parent: None,
                parent_epic: None,
            },
            ts(1000),
        )];

        let change = derive_change(&events).unwrap();

        assert_eq!(change.metadata.id, "abc1");
        assert_eq!(change.metadata.title, "Test Change");
        assert!(matches!(change.metadata.status, Status::Draft));
        assert_eq!(change.metadata.priority, Priority::High);
        assert_eq!(change.body, "Change body");
    }

    #[test]
    fn derive_change_with_status_changes() {
        let events = vec![
            make_event(
                "abc1",
                EventData::Created {
                    title: "Test Change".to_string(),
                    priority: Priority::Medium,
                    body: "Body".to_string(),
                    is_epic: None,
                    parent: None,
                    parent_epic: None,
                },
                ts(1000),
            ),
            make_event(
                "abc1",
                EventData::StatusChanged {
                    from: Status::Draft,
                    to: Status::Approved,
                },
                ts(2000),
            ),
            make_event(
                "abc1",
                EventData::StatusChanged {
                    from: Status::Approved,
                    to: Status::InProgress,
                },
                ts(3000),
            ),
        ];

        let change = derive_change(&events).unwrap();

        assert!(matches!(change.metadata.status, Status::InProgress));
    }

    #[test]
    fn derive_change_with_updates() {
        let events = vec![
            make_event(
                "abc1",
                EventData::Created {
                    title: "Original Title".to_string(),
                    priority: Priority::Low,
                    body: "Original body".to_string(),
                    is_epic: None,
                    parent: None,
                    parent_epic: None,
                },
                ts(1000),
            ),
            make_event(
                "abc1",
                EventData::Updated {
                    title: Some("New Title".to_string()),
                    body: None,
                },
                ts(2000),
            ),
            make_event(
                "abc1",
                EventData::Updated {
                    title: None,
                    body: Some("New body".to_string()),
                },
                ts(3000),
            ),
        ];

        let change = derive_change(&events).unwrap();

        assert_eq!(change.metadata.title, "New Title");
        assert_eq!(change.body, "New body");
    }

    #[test]
    fn derive_change_with_priority_change() {
        let events = vec![
            make_event(
                "abc1",
                EventData::Created {
                    title: "Test".to_string(),
                    priority: Priority::Low,
                    body: "Body".to_string(),
                    is_epic: None,
                    parent: None,
                    parent_epic: None,
                },
                ts(1000),
            ),
            make_event(
                "abc1",
                EventData::PriorityChanged {
                    from: Priority::Low,
                    to: Priority::High,
                },
                ts(2000),
            ),
        ];

        let change = derive_change(&events).unwrap();

        assert_eq!(change.metadata.priority, Priority::High);
    }

    #[test]
    fn derive_change_empty_events_fails() {
        let events: Vec<Event> = vec![];
        let result = derive_change(&events);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no events"));
    }

    #[test]
    fn derive_change_no_created_event_fails() {
        let events = vec![make_event(
            "abc1",
            EventData::StatusChanged {
                from: Status::Draft,
                to: Status::Approved,
            },
            ts(1000),
        )];

        let result = derive_change(&events);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no Created event"));
    }

    #[test]
    fn derive_change_replays_events_in_order() {
        // derive_change expects events to be pre-sorted (read_events does the sorting)
        // This test verifies events are applied in the order given
        let events = vec![
            make_event(
                "abc1",
                EventData::Created {
                    title: "Original".to_string(),
                    priority: Priority::Medium,
                    body: "Body".to_string(),
                    is_epic: None,
                    parent: None,
                    parent_epic: None,
                },
                ts(1000),
            ),
            make_event(
                "abc1",
                EventData::Updated {
                    title: Some("First Update".to_string()),
                    body: None,
                },
                ts(2000),
            ),
            make_event(
                "abc1",
                EventData::Updated {
                    title: Some("Second Update".to_string()),
                    body: None,
                },
                ts(3000),
            ),
        ];

        let change = derive_change(&events).unwrap();
        // Last update wins
        assert_eq!(change.metadata.title, "Second Update");
    }

    #[test]
    fn read_events_sorts_by_timestamp() {
        let mut file = NamedTempFile::new().unwrap();
        // Write events out of order
        let event2 = r#"{"id":"e2","bug_id":"abc1","timestamp":"2024-01-02T00:00:00Z","type":"status_changed","data":{"from":"draft","to":"approved"}}"#;
        let event1 = r#"{"id":"e1","bug_id":"abc1","timestamp":"2024-01-01T00:00:00Z","type":"created","data":{"title":"Test","priority":"medium","body":"Body"}}"#;
        writeln!(file, "{}", event2).unwrap();
        writeln!(file, "{}", event1).unwrap();

        let events = read_events(file.path()).unwrap();

        // read_events should sort them by timestamp
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0].data, EventData::Created { .. }));
        assert!(matches!(events[1].data, EventData::StatusChanged { .. }));
    }

    #[test]
    fn read_events_from_file() {
        let mut file = NamedTempFile::new().unwrap();
        let event1 = r#"{"id":"e1","bug_id":"abc1","timestamp":"2024-01-01T00:00:00Z","type":"created","data":{"title":"Test","priority":"medium","body":"Body"}}"#;
        let event2 = r#"{"id":"e2","bug_id":"abc1","timestamp":"2024-01-02T00:00:00Z","type":"status_changed","data":{"from":"draft","to":"approved"}}"#;
        writeln!(file, "{}", event1).unwrap();
        writeln!(file, "{}", event2).unwrap();

        let events = read_events(file.path()).unwrap();

        assert_eq!(events.len(), 2);
        assert!(matches!(events[0].data, EventData::Created { .. }));
        assert!(matches!(events[1].data, EventData::StatusChanged { .. }));
    }

    #[test]
    fn read_events_from_nonexistent_file_returns_empty() {
        let events = read_events(Path::new("/nonexistent/path.jsonl")).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn read_events_skips_empty_lines() {
        let mut file = NamedTempFile::new().unwrap();
        let event1 = r#"{"id":"e1","bug_id":"abc1","timestamp":"2024-01-01T00:00:00Z","type":"created","data":{"title":"Test","priority":"medium","body":"Body"}}"#;
        writeln!(file, "{}", event1).unwrap();
        writeln!(file, "").unwrap();
        writeln!(file, "   ").unwrap();

        let events = read_events(file.path()).unwrap();

        assert_eq!(events.len(), 1);
    }

    #[test]
    fn append_event_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");

        let event = Event::created(
            "abc1".to_string(),
            "Test".to_string(),
            Priority::Medium,
            "Body".to_string(),
        );

        append_event(&path, &event).unwrap();

        assert!(path.exists());
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("abc1"));
        assert!(content.contains("Test"));
    }

    #[test]
    fn append_event_appends_to_existing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");

        let event1 = Event::created(
            "abc1".to_string(),
            "Test".to_string(),
            Priority::Medium,
            "Body".to_string(),
        );
        let event2 = Event::status_changed("abc1".to_string(), Status::Draft, Status::Approved);

        append_event(&path, &event1).unwrap();
        append_event(&path, &event2).unwrap();

        let events = read_events(&path).unwrap();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn parse_event_without_version_field() {
        // Events created before versioning was added don't have a version field.
        // The parser should handle this by defaulting to version 1.
        // Note: Uses bug_id in JSON for backward compatibility (serde alias)
        let json_without_version = r#"{"id":"e1","bug_id":"abc1","timestamp":"2024-01-01T00:00:00Z","type":"created","data":{"title":"Test","priority":"medium","body":"Body"}}"#;

        let event: Event = serde_json::from_str(json_without_version).unwrap();

        assert_eq!(event.version, 1);
        assert_eq!(event.id, "e1");
        assert_eq!(event.change_id, "abc1");
        assert!(matches!(event.data, EventData::Created { .. }));
    }

    #[test]
    fn parse_event_with_version_field() {
        // Events with an explicit version field should use that version.
        // Note: Uses bug_id in JSON for backward compatibility (serde alias)
        let json_with_version = r#"{"version":2,"id":"e1","bug_id":"abc1","timestamp":"2024-01-01T00:00:00Z","type":"created","data":{"title":"Test","priority":"medium","body":"Body"}}"#;

        let event: Event = serde_json::from_str(json_with_version).unwrap();

        assert_eq!(event.version, 2);
    }

    #[test]
    fn new_event_has_current_version() {
        let event = Event::created(
            "abc1".to_string(),
            "Test".to_string(),
            Priority::Medium,
            "Body".to_string(),
        );

        assert_eq!(event.version, CURRENT_EVENT_VERSION);
    }

    #[test]
    fn serialized_event_includes_version() {
        let event = Event::created(
            "abc1".to_string(),
            "Test".to_string(),
            Priority::Medium,
            "Body".to_string(),
        );

        let json = serde_json::to_string(&event).unwrap();

        // The version field should be included in serialized output
        assert!(json.contains("\"version\":1"));
    }
}
