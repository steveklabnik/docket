use docket::change::{Priority, Status};
use docket::event::{derive_change, read_events};
use std::path::PathBuf;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn simple_bug_fixture() {
    let path = fixture_path("simple_bug.jsonl");
    let events = read_events(&path).unwrap();

    assert_eq!(events.len(), 1);

    let bug = derive_change(&events).unwrap();

    assert_eq!(bug.metadata.id, "abc1");
    assert_eq!(bug.metadata.title, "Simple test bug");
    assert!(matches!(bug.metadata.status, Status::Draft));
    assert_eq!(bug.metadata.priority, Priority::Medium);
    assert!(bug.body.contains("## Goal"));
}

#[test]
fn completed_bug_fixture() {
    let path = fixture_path("completed_bug.jsonl");
    let events = read_events(&path).unwrap();

    // 5 events: created + 3 status changes + 1 deprecated change_linked (ignored)
    assert_eq!(events.len(), 5);

    let bug = derive_change(&events).unwrap();

    assert_eq!(bug.metadata.id, "done");
    assert_eq!(bug.metadata.title, "Completed bug example");
    assert!(matches!(bug.metadata.status, Status::Done));
    assert_eq!(bug.metadata.priority, Priority::High);
}

#[test]
fn updated_bug_fixture() {
    let path = fixture_path("updated_bug.jsonl");
    let events = read_events(&path).unwrap();

    assert_eq!(events.len(), 4);

    let bug = derive_change(&events).unwrap();

    assert_eq!(bug.metadata.id, "updt");
    assert_eq!(bug.metadata.title, "Updated title");
    assert!(matches!(bug.metadata.status, Status::Draft));
    assert_eq!(bug.metadata.priority, Priority::High);
    assert!(bug.body.contains("Updated body"));
    assert!(bug.body.contains("## Goal"));
}
