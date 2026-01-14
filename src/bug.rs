use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

// Note: Utc is used in BugMetadata for created timestamp

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Draft,
    Approved,
    InProgress,
    Done,
    NotPlanned,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Status::Draft => write!(f, "draft"),
            Status::Approved => write!(f, "approved"),
            Status::InProgress => write!(f, "in-progress"),
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
            "done" => Ok(Status::Done),
            "not-planned" | "not_planned" | "notplanned" => Ok(Status::NotPlanned),
            _ => Err(anyhow!("unknown status: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Low,
    #[default]
    Medium,
    High,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BugMetadata {
    pub id: String,
    pub title: String,
    pub status: Status,
    pub priority: Priority,
    pub created: DateTime<Utc>,
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
    fn status_display() {
        assert_eq!(Status::Draft.to_string(), "draft");
        assert_eq!(Status::Approved.to_string(), "approved");
        assert_eq!(Status::InProgress.to_string(), "in-progress");
        assert_eq!(Status::Done.to_string(), "done");
        assert_eq!(Status::NotPlanned.to_string(), "not-planned");
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
}
