use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::str::FromStr;

// Note: Utc is used in BugMetadata for created timestamp

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Draft,
    Approved,
    InProgress,
    Blocked,
    Review,
    Done,
    NotPlanned,
}

impl Status {
    fn sort_order(&self) -> u8 {
        match self {
            Status::Review => 0,     // Awaiting review
            Status::Blocked => 1,    // Blocked on external dependency
            Status::InProgress => 2, // Active work
            Status::Approved => 3,   // Ready to work
            Status::Draft => 4,      // Needs approval
            Status::Done => 5,       // Completed
            Status::NotPlanned => 6, // Won't do
        }
    }
}

impl Ord for Status {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.sort_order().cmp(&other.sort_order())
    }
}

impl PartialOrd for Status {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Status::Draft => write!(f, "draft"),
            Status::Approved => write!(f, "approved"),
            Status::InProgress => write!(f, "in-progress"),
            Status::Blocked => write!(f, "blocked"),
            Status::Review => write!(f, "review"),
            Status::Done => write!(f, "done"),
            Status::NotPlanned => write!(f, "not-planned"),
        }
    }
}

impl FromStr for Status {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "draft" => Ok(Status::Draft),
            "approved" => Ok(Status::Approved),
            "in-progress" | "in_progress" | "inprogress" => Ok(Status::InProgress),
            "blocked" => Ok(Status::Blocked),
            "review" => Ok(Status::Review),
            "done" => Ok(Status::Done),
            "not-planned" | "not_planned" | "notplanned" => Ok(Status::NotPlanned),
            _ => Err(anyhow!("unknown status: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Low,
    #[default]
    Medium,
    High,
}

impl Priority {
    fn sort_order(&self) -> u8 {
        match self {
            Priority::High => 0, // High priority first
            Priority::Medium => 1,
            Priority::Low => 2,
        }
    }
}

impl Ord for Priority {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.sort_order().cmp(&other.sort_order())
    }
}

impl PartialOrd for Priority {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Priority::Low => write!(f, "low"),
            Priority::Medium => write!(f, "medium"),
            Priority::High => write!(f, "high"),
        }
    }
}

impl FromStr for Priority {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "low" => Ok(Priority::Low),
            "medium" | "med" => Ok(Priority::Medium),
            "high" => Ok(Priority::High),
            _ => Err(anyhow!("unknown priority: {}", s)),
        }
    }
}

/// Changelog type determines which section a bug appears in when generating changelogs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ChangelogType {
    /// "Added" section - new features
    Feature,
    /// "Fixed" section - bug fixes
    Fix,
    /// "Changed" section - changes to existing functionality
    Change,
    /// "Deprecated" section - soon-to-be removed features
    Deprecated,
    /// "Removed" section - removed features
    Removed,
    /// "Security" section - security fixes
    Security,
    /// Internal change - does not appear in changelog
    Internal,
}

impl fmt::Display for ChangelogType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChangelogType::Feature => write!(f, "feature"),
            ChangelogType::Fix => write!(f, "fix"),
            ChangelogType::Change => write!(f, "change"),
            ChangelogType::Deprecated => write!(f, "deprecated"),
            ChangelogType::Removed => write!(f, "removed"),
            ChangelogType::Security => write!(f, "security"),
            ChangelogType::Internal => write!(f, "internal"),
        }
    }
}

impl FromStr for ChangelogType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "feature" | "feat" | "added" => Ok(ChangelogType::Feature),
            "fix" | "fixed" => Ok(ChangelogType::Fix),
            "change" | "changed" => Ok(ChangelogType::Change),
            "deprecated" | "deprecate" => Ok(ChangelogType::Deprecated),
            "removed" | "remove" => Ok(ChangelogType::Removed),
            "security" | "sec" => Ok(ChangelogType::Security),
            "internal" | "int" | "chore" => Ok(ChangelogType::Internal),
            _ => Err(anyhow!(
                "unknown changelog type: {}. Valid options: feature, fix, change, deprecated, removed, security, internal",
                s
            )),
        }
    }
}

impl ChangelogType {
    /// Returns the Keep a Changelog section header for this type
    pub fn section_header(&self) -> Option<&'static str> {
        match self {
            ChangelogType::Feature => Some("Added"),
            ChangelogType::Fix => Some("Fixed"),
            ChangelogType::Change => Some("Changed"),
            ChangelogType::Deprecated => Some("Deprecated"),
            ChangelogType::Removed => Some("Removed"),
            ChangelogType::Security => Some("Security"),
            ChangelogType::Internal => None, // Internal changes don't appear in changelog
        }
    }

    /// Sort order for changelog sections (follows Keep a Changelog convention)
    pub fn sort_order(&self) -> u8 {
        match self {
            ChangelogType::Security => 0, // Security first (important)
            ChangelogType::Feature => 1,  // Added
            ChangelogType::Change => 2,   // Changed
            ChangelogType::Deprecated => 3,
            ChangelogType::Removed => 4,
            ChangelogType::Fix => 5,      // Fixed
            ChangelogType::Internal => 6, // Not shown, but for completeness
        }
    }
}

/// Sort field for listing bugs
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SortBy {
    #[default]
    Priority,
    Created,
    Status,
}

impl fmt::Display for SortBy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SortBy::Priority => write!(f, "priority"),
            SortBy::Created => write!(f, "created"),
            SortBy::Status => write!(f, "status"),
        }
    }
}

impl FromStr for SortBy {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "priority" | "p" => Ok(SortBy::Priority),
            "created" | "c" | "date" => Ok(SortBy::Created),
            "status" | "s" => Ok(SortBy::Status),
            _ => Err(anyhow!(
                "unknown sort field: {}. Valid options: priority, created, status",
                s
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BugMetadata {
    pub id: String,
    pub title: String,
    pub status: Status,
    pub priority: Priority,
    pub created: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changelog_type: Option<ChangelogType>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<String>,
    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    pub tags: HashSet<String>,
    /// If true, this bug is an epic (parent container for ordered steps)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_epic: Option<bool>,
    /// If set, this bug is a child step of the specified epic
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_epic: Option<String>,
    /// If blocked, the reason why (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Bug {
    pub metadata: BugMetadata,
    pub body: String,
}

impl Bug {
    /// Parse a bug from markdown format (used for migration from legacy format)
    pub fn parse(content: &str) -> Result<Self> {
        let parts: Vec<&str> = content.splitn(3, "---").collect();

        if parts.len() < 3 {
            return Err(anyhow!(
                "invalid bug file format: expected YAML frontmatter between --- delimiters"
            ));
        }

        let yaml_content = parts[1].trim();
        let body = parts[2].trim().to_string();

        let metadata: BugMetadata = serde_yaml::from_str(yaml_content)?;

        Ok(Bug { metadata, body })
    }

    pub fn id(&self) -> &str {
        &self.metadata.id
    }

    pub fn title(&self) -> &str {
        &self.metadata.title
    }

    pub fn status(&self) -> &Status {
        &self.metadata.status
    }

    pub fn priority(&self) -> &Priority {
        &self.metadata.priority
    }

    pub fn created(&self) -> DateTime<Utc> {
        self.metadata.created
    }

    pub fn changelog_type(&self) -> Option<&ChangelogType> {
        self.metadata.changelog_type.as_ref()
    }

    pub fn versions(&self) -> &[String] {
        &self.metadata.versions
    }

    pub fn has_version(&self, version: &str) -> bool {
        self.metadata.versions.iter().any(|v| v == version)
    }

    pub fn tags(&self) -> &HashSet<String> {
        &self.metadata.tags
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        self.metadata.tags.contains(tag)
    }

    /// Returns true if this bug is an epic
    pub fn is_epic(&self) -> bool {
        self.metadata.is_epic.unwrap_or(false)
    }

    /// Returns the parent epic ID if this is a child bug
    pub fn parent_epic(&self) -> Option<&str> {
        self.metadata.parent_epic.as_deref()
    }

    /// Returns true if this is a child of an epic
    pub fn is_child(&self) -> bool {
        self.metadata.parent_epic.is_some()
    }

    /// Returns the blocked reason if set
    pub fn blocked_reason(&self) -> Option<&str> {
        self.metadata.blocked_reason.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_from_str_draft() {
        assert!(matches!(Status::from_str("draft").unwrap(), Status::Draft));
        assert!(matches!(Status::from_str("Draft").unwrap(), Status::Draft));
        assert!(matches!(Status::from_str("DRAFT").unwrap(), Status::Draft));
    }

    #[test]
    fn status_from_str_approved() {
        assert!(matches!(
            Status::from_str("approved").unwrap(),
            Status::Approved
        ));
    }

    #[test]
    fn status_from_str_in_progress_variants() {
        assert!(matches!(
            Status::from_str("in-progress").unwrap(),
            Status::InProgress
        ));
        assert!(matches!(
            Status::from_str("in_progress").unwrap(),
            Status::InProgress
        ));
        assert!(matches!(
            Status::from_str("inprogress").unwrap(),
            Status::InProgress
        ));
        assert!(matches!(
            Status::from_str("IN-PROGRESS").unwrap(),
            Status::InProgress
        ));
    }

    #[test]
    fn status_from_str_done() {
        assert!(matches!(Status::from_str("done").unwrap(), Status::Done));
    }

    #[test]
    fn status_from_str_not_planned_variants() {
        assert!(matches!(
            Status::from_str("not-planned").unwrap(),
            Status::NotPlanned
        ));
        assert!(matches!(
            Status::from_str("not_planned").unwrap(),
            Status::NotPlanned
        ));
        assert!(matches!(
            Status::from_str("notplanned").unwrap(),
            Status::NotPlanned
        ));
        assert!(matches!(
            Status::from_str("NOT-PLANNED").unwrap(),
            Status::NotPlanned
        ));
    }

    #[test]
    fn status_from_str_unknown_fails() {
        assert!(Status::from_str("invalid").is_err());
        assert!(Status::from_str("").is_err());
    }

    #[test]
    fn status_from_str_review() {
        assert!(matches!(
            Status::from_str("review").unwrap(),
            Status::Review
        ));
        assert!(matches!(
            Status::from_str("REVIEW").unwrap(),
            Status::Review
        ));
    }

    #[test]
    fn status_display() {
        assert_eq!(Status::Draft.to_string(), "draft");
        assert_eq!(Status::Approved.to_string(), "approved");
        assert_eq!(Status::InProgress.to_string(), "in-progress");
        assert_eq!(Status::Blocked.to_string(), "blocked");
        assert_eq!(Status::Review.to_string(), "review");
        assert_eq!(Status::Done.to_string(), "done");
        assert_eq!(Status::NotPlanned.to_string(), "not-planned");
    }

    #[test]
    fn status_from_str_blocked() {
        assert!(matches!(
            Status::from_str("blocked").unwrap(),
            Status::Blocked
        ));
        assert!(matches!(
            Status::from_str("BLOCKED").unwrap(),
            Status::Blocked
        ));
    }

    #[test]
    fn priority_from_str_low() {
        assert_eq!(Priority::from_str("low").unwrap(), Priority::Low);
        assert_eq!(Priority::from_str("LOW").unwrap(), Priority::Low);
    }

    #[test]
    fn priority_from_str_medium() {
        assert_eq!(Priority::from_str("medium").unwrap(), Priority::Medium);
        assert_eq!(Priority::from_str("med").unwrap(), Priority::Medium);
    }

    #[test]
    fn priority_from_str_high() {
        assert_eq!(Priority::from_str("high").unwrap(), Priority::High);
    }

    #[test]
    fn priority_from_str_unknown_fails() {
        assert!(Priority::from_str("critical").is_err());
        assert!(Priority::from_str("").is_err());
    }

    #[test]
    fn priority_display() {
        assert_eq!(Priority::Low.to_string(), "low");
        assert_eq!(Priority::Medium.to_string(), "medium");
        assert_eq!(Priority::High.to_string(), "high");
    }

    #[test]
    fn priority_default_is_medium() {
        assert_eq!(Priority::default(), Priority::Medium);
    }

    #[test]
    fn bug_parse_valid_markdown() {
        let content = r#"---
id: test1
title: Test Bug
status: draft
priority: high
created: 2024-01-01T00:00:00Z
---
This is the bug body.

## Goal
Fix the thing."#;

        let bug = Bug::parse(content).unwrap();

        assert_eq!(bug.metadata.id, "test1");
        assert_eq!(bug.metadata.title, "Test Bug");
        assert!(matches!(bug.metadata.status, Status::Draft));
        assert_eq!(bug.metadata.priority, Priority::High);
        assert!(bug.body.contains("This is the bug body"));
        assert!(bug.body.contains("## Goal"));
    }

    #[test]
    fn bug_parse_invalid_no_frontmatter() {
        let content = "Just some text without frontmatter";
        assert!(Bug::parse(content).is_err());
    }

    #[test]
    fn bug_parse_invalid_incomplete_frontmatter() {
        let content = "---\nid: test\n---";
        // Missing required fields should fail
        assert!(Bug::parse(content).is_err());
    }

    #[test]
    fn bug_accessor_methods() {
        let content = r#"---
id: abc1
title: Accessor Test
status: approved
priority: low
created: 2024-01-01T00:00:00Z
---
Body"#;

        let bug = Bug::parse(content).unwrap();

        assert_eq!(bug.id(), "abc1");
        assert_eq!(bug.title(), "Accessor Test");
        assert!(matches!(bug.status(), Status::Approved));
        assert_eq!(bug.priority(), &Priority::Low);
    }

    // Priority ordering tests
    #[test]
    fn priority_ord_high_first() {
        assert!(Priority::High < Priority::Medium);
        assert!(Priority::Medium < Priority::Low);
        assert!(Priority::High < Priority::Low);
    }

    #[test]
    fn priority_sort_order() {
        let mut priorities = vec![Priority::Low, Priority::High, Priority::Medium];
        priorities.sort();
        assert_eq!(
            priorities,
            vec![Priority::High, Priority::Medium, Priority::Low]
        );
    }

    // Status ordering tests
    #[test]
    fn status_ord_workflow_order() {
        // Review (awaiting review) should come first
        assert!(Status::Review < Status::Blocked);
        assert!(Status::Blocked < Status::InProgress);
        assert!(Status::InProgress < Status::Approved);
        assert!(Status::Approved < Status::Draft);
        assert!(Status::Draft < Status::Done);
        assert!(Status::Done < Status::NotPlanned);
    }

    #[test]
    fn status_sort_order() {
        let mut statuses = vec![
            Status::Done,
            Status::Draft,
            Status::InProgress,
            Status::NotPlanned,
            Status::Approved,
            Status::Review,
            Status::Blocked,
        ];
        statuses.sort();
        assert_eq!(
            statuses,
            vec![
                Status::Review,
                Status::Blocked,
                Status::InProgress,
                Status::Approved,
                Status::Draft,
                Status::Done,
                Status::NotPlanned,
            ]
        );
    }

    // SortBy tests
    #[test]
    fn sort_by_from_str_priority() {
        assert_eq!(SortBy::from_str("priority").unwrap(), SortBy::Priority);
        assert_eq!(SortBy::from_str("PRIORITY").unwrap(), SortBy::Priority);
        assert_eq!(SortBy::from_str("p").unwrap(), SortBy::Priority);
    }

    #[test]
    fn sort_by_from_str_created() {
        assert_eq!(SortBy::from_str("created").unwrap(), SortBy::Created);
        assert_eq!(SortBy::from_str("c").unwrap(), SortBy::Created);
        assert_eq!(SortBy::from_str("date").unwrap(), SortBy::Created);
    }

    #[test]
    fn sort_by_from_str_status() {
        assert_eq!(SortBy::from_str("status").unwrap(), SortBy::Status);
        assert_eq!(SortBy::from_str("s").unwrap(), SortBy::Status);
    }

    #[test]
    fn sort_by_from_str_unknown_fails() {
        assert!(SortBy::from_str("invalid").is_err());
        assert!(SortBy::from_str("").is_err());
    }

    #[test]
    fn sort_by_display() {
        assert_eq!(SortBy::Priority.to_string(), "priority");
        assert_eq!(SortBy::Created.to_string(), "created");
        assert_eq!(SortBy::Status.to_string(), "status");
    }

    #[test]
    fn sort_by_default_is_priority() {
        assert_eq!(SortBy::default(), SortBy::Priority);
    }
}
