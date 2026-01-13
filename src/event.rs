use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use uuid::Uuid;

use crate::bug::{Bug, BugMetadata, Priority, Status};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "data")]
pub enum EventData {
    Created {
        title: String,
        priority: Priority,
        body: String,
    },
    StatusChanged {
        from: Status,
        to: Status,
    },
    Updated {
        title: Option<String>,
        body: Option<String>,
    },
    PriorityChanged {
        from: Priority,
        to: Priority,
    },
    ChangeLinked {
        change_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub bug_id: String,
    pub timestamp: DateTime<Utc>,
    #[serde(flatten)]
    pub data: EventData,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
}

impl Event {
    pub fn new(bug_id: String, data: EventData) -> Self {
        Event {
            id: Uuid::new_v4().to_string(),
            bug_id,
            timestamp: Utc::now(),
            data,
            actor: whoami::fallible::hostname().ok(),
        }
    }

    pub fn created(bug_id: String, title: String, priority: Priority, body: String) -> Self {
        Self::new(
            bug_id,
            EventData::Created {
                title,
                priority,
                body,
            },
        )
    }

    pub fn status_changed(bug_id: String, from: Status, to: Status) -> Self {
        Self::new(bug_id, EventData::StatusChanged { from, to })
    }

    pub fn updated(bug_id: String, title: Option<String>, body: Option<String>) -> Self {
        Self::new(bug_id, EventData::Updated { title, body })
    }

    pub fn change_linked(bug_id: String, change_id: String) -> Self {
        Self::new(bug_id, EventData::ChangeLinked { change_id })
    }

    pub fn priority_changed(bug_id: String, from: Priority, to: Priority) -> Self {
        Self::new(bug_id, EventData::PriorityChanged { from, to })
    }
}

/// Append an event to a bug's JSONL file
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

/// Read all events from a bug's JSONL file
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

/// Derive current bug state by replaying events
pub fn derive_bug(events: &[Event]) -> Result<Bug> {
    if events.is_empty() {
        return Err(anyhow::anyhow!("no events to derive bug from"));
    }

    // Find the Created event to get initial state
    let created = events
        .iter()
        .find(|e| matches!(e.data, EventData::Created { .. }))
        .ok_or_else(|| anyhow::anyhow!("no Created event found"))?;

    let (initial_title, initial_priority, initial_body) = match &created.data {
        EventData::Created {
            title,
            priority,
            body,
        } => (title.clone(), priority.clone(), body.clone()),
        _ => unreachable!(),
    };

    let mut bug = Bug {
        metadata: BugMetadata {
            id: created.bug_id.clone(),
            title: initial_title,
            status: Status::Draft,
            priority: initial_priority,
            created: created.timestamp,
            changes: Vec::new(),
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
                bug.metadata.status = to.clone();
            }
            EventData::Updated { title, body } => {
                if let Some(t) = title {
                    bug.metadata.title = t.clone();
                }
                if let Some(b) = body {
                    bug.body = b.clone();
                }
            }
            EventData::PriorityChanged { to, .. } => {
                bug.metadata.priority = to.clone();
            }
            EventData::ChangeLinked { change_id } => {
                if !bug.metadata.changes.contains(change_id) {
                    bug.metadata.changes.push(change_id.clone());
                }
            }
        }
    }

    Ok(bug)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn make_event(bug_id: &str, data: EventData, timestamp: DateTime<Utc>) -> Event {
        Event {
            id: "test-event-id".to_string(),
            bug_id: bug_id.to_string(),
            timestamp,
            data,
            actor: Some("test".to_string()),
        }
    }

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    #[test]
    fn derive_bug_from_created_event() {
        let events = vec![make_event(
            "abc1",
            EventData::Created {
                title: "Test Bug".to_string(),
                priority: Priority::High,
                body: "Bug body".to_string(),
            },
            ts(1000),
        )];

        let bug = derive_bug(&events).unwrap();

        assert_eq!(bug.metadata.id, "abc1");
        assert_eq!(bug.metadata.title, "Test Bug");
        assert!(matches!(bug.metadata.status, Status::Draft));
        assert_eq!(bug.metadata.priority, Priority::High);
        assert_eq!(bug.body, "Bug body");
        assert!(bug.metadata.changes.is_empty());
    }

    #[test]
    fn derive_bug_with_status_changes() {
        let events = vec![
            make_event(
                "abc1",
                EventData::Created {
                    title: "Test Bug".to_string(),
                    priority: Priority::Medium,
                    body: "Body".to_string(),
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

        let bug = derive_bug(&events).unwrap();

        assert!(matches!(bug.metadata.status, Status::InProgress));
    }

    #[test]
    fn derive_bug_with_updates() {
        let events = vec![
            make_event(
                "abc1",
                EventData::Created {
                    title: "Original Title".to_string(),
                    priority: Priority::Low,
                    body: "Original body".to_string(),
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

        let bug = derive_bug(&events).unwrap();

        assert_eq!(bug.metadata.title, "New Title");
        assert_eq!(bug.body, "New body");
    }

    #[test]
    fn derive_bug_with_priority_change() {
        let events = vec![
            make_event(
                "abc1",
                EventData::Created {
                    title: "Test".to_string(),
                    priority: Priority::Low,
                    body: "Body".to_string(),
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

        let bug = derive_bug(&events).unwrap();

        assert_eq!(bug.metadata.priority, Priority::High);
    }

    #[test]
    fn derive_bug_with_linked_changes() {
        let events = vec![
            make_event(
                "abc1",
                EventData::Created {
                    title: "Test".to_string(),
                    priority: Priority::Medium,
                    body: "Body".to_string(),
                },
                ts(1000),
            ),
            make_event(
                "abc1",
                EventData::ChangeLinked {
                    change_id: "change1".to_string(),
                },
                ts(2000),
            ),
            make_event(
                "abc1",
                EventData::ChangeLinked {
                    change_id: "change2".to_string(),
                },
                ts(3000),
            ),
        ];

        let bug = derive_bug(&events).unwrap();

        assert_eq!(bug.metadata.changes, vec!["change1", "change2"]);
    }

    #[test]
    fn derive_bug_deduplicates_linked_changes() {
        let events = vec![
            make_event(
                "abc1",
                EventData::Created {
                    title: "Test".to_string(),
                    priority: Priority::Medium,
                    body: "Body".to_string(),
                },
                ts(1000),
            ),
            make_event(
                "abc1",
                EventData::ChangeLinked {
                    change_id: "change1".to_string(),
                },
                ts(2000),
            ),
            make_event(
                "abc1",
                EventData::ChangeLinked {
                    change_id: "change1".to_string(),
                },
                ts(3000),
            ),
        ];

        let bug = derive_bug(&events).unwrap();

        assert_eq!(bug.metadata.changes, vec!["change1"]);
    }

    #[test]
    fn derive_bug_empty_events_fails() {
        let events: Vec<Event> = vec![];
        let result = derive_bug(&events);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no events"));
    }

    #[test]
    fn derive_bug_no_created_event_fails() {
        let events = vec![make_event(
            "abc1",
            EventData::StatusChanged {
                from: Status::Draft,
                to: Status::Approved,
            },
            ts(1000),
        )];

        let result = derive_bug(&events);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no Created event"));
    }

    #[test]
    fn derive_bug_replays_events_in_order() {
        // derive_bug expects events to be pre-sorted (read_events does the sorting)
        // This test verifies events are applied in the order given
        let events = vec![
            make_event(
                "abc1",
                EventData::Created {
                    title: "Original".to_string(),
                    priority: Priority::Medium,
                    body: "Body".to_string(),
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

        let bug = derive_bug(&events).unwrap();
        // Last update wins
        assert_eq!(bug.metadata.title, "Second Update");
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
}
