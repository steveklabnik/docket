use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn docket_cmd() -> Command {
    cargo_bin_cmd!("docket")
}

fn setup_docket_repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    docket_cmd()
        .current_dir(dir.path())
        .arg("init")
        .assert()
        .success();
    dir
}

mod init_command {
    use super::*;

    #[test]
    fn init_creates_docket_directory() {
        let dir = TempDir::new().unwrap();

        docket_cmd()
            .current_dir(dir.path())
            .arg("init")
            .assert()
            .success()
            .stdout(predicate::str::contains("Initialized docket"));

        assert!(dir.path().join(".docket").exists());
        assert!(dir.path().join(".docket/bugs").exists());
    }

    #[test]
    fn init_fails_if_already_initialized() {
        let dir = setup_docket_repo();

        docket_cmd()
            .current_dir(dir.path())
            .arg("init")
            .assert()
            .failure()
            .stderr(predicate::str::contains("already initialized"));
    }
}

mod new_command {
    use super::*;

    #[test]
    fn new_creates_bug_with_title() {
        let dir = setup_docket_repo();

        docket_cmd()
            .current_dir(dir.path())
            .args(["new", "--title", "Test bug title"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Created bug"));

        // Verify a .jsonl file was created
        let bugs_dir = dir.path().join(".docket/bugs");
        let entries: Vec<_> = fs::read_dir(&bugs_dir).unwrap().collect();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn new_creates_bug_with_priority() {
        let dir = setup_docket_repo();

        docket_cmd()
            .current_dir(dir.path())
            .args(["new", "--title", "High priority bug", "--priority", "high"])
            .assert()
            .success();

        // List should show the bug with high priority
        docket_cmd()
            .current_dir(dir.path())
            .arg("list")
            .assert()
            .success()
            .stdout(predicate::str::contains("high"))
            .stdout(predicate::str::contains("High priority bug"));
    }

    #[test]
    fn new_creates_bug_with_body_from_stdin() {
        let dir = setup_docket_repo();

        docket_cmd()
            .current_dir(dir.path())
            .args(["new", "--title", "Bug with body", "--body", "-"])
            .write_stdin("This is the bug body content")
            .assert()
            .success();

        // Show should display the body
        let list_output = docket_cmd()
            .current_dir(dir.path())
            .arg("list")
            .output()
            .unwrap();
        let output = String::from_utf8_lossy(&list_output.stdout);
        // Extract the bug ID from list output (first 4 chars of a line)
        let id = output
            .lines()
            .find(|l| l.contains("Bug with body"))
            .and_then(|l| l.split_whitespace().next())
            .unwrap();

        docket_cmd()
            .current_dir(dir.path())
            .args(["show", id])
            .assert()
            .success()
            .stdout(predicate::str::contains("This is the bug body content"));
    }
}

mod list_command {
    use super::*;

    #[test]
    fn list_empty_repo() {
        let dir = setup_docket_repo();

        docket_cmd()
            .current_dir(dir.path())
            .arg("list")
            .assert()
            .success()
            .stdout(predicate::str::contains("No bugs found"));
    }

    #[test]
    fn list_shows_bugs() {
        let dir = setup_docket_repo();

        // Create a bug
        docket_cmd()
            .current_dir(dir.path())
            .args(["new", "--title", "First bug"])
            .assert()
            .success();

        docket_cmd()
            .current_dir(dir.path())
            .arg("list")
            .assert()
            .success()
            .stdout(predicate::str::contains("First bug"))
            .stdout(predicate::str::contains("draft"));
    }

    #[test]
    fn list_filters_by_status() {
        let dir = setup_docket_repo();

        // Create two bugs
        docket_cmd()
            .current_dir(dir.path())
            .args(["new", "--title", "Draft bug"])
            .assert()
            .success();

        // Get the bug ID
        let list_output = docket_cmd()
            .current_dir(dir.path())
            .arg("list")
            .output()
            .unwrap();
        let output = String::from_utf8_lossy(&list_output.stdout);
        let id = output
            .lines()
            .find(|l| l.contains("Draft bug"))
            .and_then(|l| l.split_whitespace().next())
            .unwrap();

        // Approve the bug
        docket_cmd()
            .current_dir(dir.path())
            .args(["approve", id])
            .assert()
            .success();

        // Create another draft bug
        docket_cmd()
            .current_dir(dir.path())
            .args(["new", "--title", "Another draft"])
            .assert()
            .success();

        // Filter by draft status
        docket_cmd()
            .current_dir(dir.path())
            .args(["list", "--status", "draft"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Another draft"))
            .stdout(predicate::str::contains("Draft bug").not());

        // Filter by approved status
        docket_cmd()
            .current_dir(dir.path())
            .args(["list", "--status", "approved"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Draft bug"))
            .stdout(predicate::str::contains("Another draft").not());
    }

    #[test]
    fn list_filters_by_priority() {
        let dir = setup_docket_repo();

        docket_cmd()
            .current_dir(dir.path())
            .args(["new", "--title", "Low priority", "--priority", "low"])
            .assert()
            .success();

        docket_cmd()
            .current_dir(dir.path())
            .args(["new", "--title", "High priority", "--priority", "high"])
            .assert()
            .success();

        docket_cmd()
            .current_dir(dir.path())
            .args(["list", "--priority", "high"])
            .assert()
            .success()
            .stdout(predicate::str::contains("High priority"))
            .stdout(predicate::str::contains("Low priority").not());
    }
}

mod show_command {
    use super::*;

    #[test]
    fn show_displays_bug_details() {
        let dir = setup_docket_repo();

        docket_cmd()
            .current_dir(dir.path())
            .args(["new", "--title", "Show test bug", "--priority", "high"])
            .assert()
            .success();

        // Get the bug ID
        let list_output = docket_cmd()
            .current_dir(dir.path())
            .arg("list")
            .output()
            .unwrap();
        let output = String::from_utf8_lossy(&list_output.stdout);
        let id = output
            .lines()
            .find(|l| l.contains("Show test bug"))
            .and_then(|l| l.split_whitespace().next())
            .unwrap();

        docket_cmd()
            .current_dir(dir.path())
            .args(["show", id])
            .assert()
            .success()
            .stdout(predicate::str::contains("Show test bug"))
            .stdout(predicate::str::contains("draft"))
            .stdout(predicate::str::contains("high"));
    }

    #[test]
    fn show_supports_prefix_matching() {
        let dir = setup_docket_repo();

        docket_cmd()
            .current_dir(dir.path())
            .args(["new", "--title", "Prefix test"])
            .assert()
            .success();

        // Get the full bug ID
        let list_output = docket_cmd()
            .current_dir(dir.path())
            .arg("list")
            .output()
            .unwrap();
        let output = String::from_utf8_lossy(&list_output.stdout);
        let full_id = output
            .lines()
            .find(|l| l.contains("Prefix test"))
            .and_then(|l| l.split_whitespace().next())
            .unwrap();

        // Use just the first character as prefix
        let prefix = &full_id[..1];

        docket_cmd()
            .current_dir(dir.path())
            .args(["show", prefix])
            .assert()
            .success()
            .stdout(predicate::str::contains("Prefix test"));
    }

    #[test]
    fn show_not_found() {
        let dir = setup_docket_repo();

        docket_cmd()
            .current_dir(dir.path())
            .args(["show", "xxxx"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("not found"));
    }
}

mod status_commands {
    use super::*;

    fn create_bug_and_get_id(dir: &TempDir, title: &str) -> String {
        docket_cmd()
            .current_dir(dir.path())
            .args(["new", "--title", title])
            .assert()
            .success();

        let list_output = docket_cmd()
            .current_dir(dir.path())
            .arg("list")
            .output()
            .unwrap();
        let output = String::from_utf8_lossy(&list_output.stdout);
        output
            .lines()
            .find(|l| l.contains(title))
            .and_then(|l| l.split_whitespace().next())
            .unwrap()
            .to_string()
    }

    #[test]
    fn approve_changes_status() {
        let dir = setup_docket_repo();
        let id = create_bug_and_get_id(&dir, "Approve test");

        docket_cmd()
            .current_dir(dir.path())
            .args(["approve", &id])
            .assert()
            .success();

        docket_cmd()
            .current_dir(dir.path())
            .args(["show", &id])
            .assert()
            .success()
            .stdout(predicate::str::contains("approved"));
    }

    #[test]
    fn update_status_to_in_progress() {
        let dir = setup_docket_repo();
        let id = create_bug_and_get_id(&dir, "In-progress test");

        // First approve
        docket_cmd()
            .current_dir(dir.path())
            .args(["approve", &id])
            .assert()
            .success();

        // Then set status to in-progress via update
        docket_cmd()
            .current_dir(dir.path())
            .args(["update", &id, "--status", "in-progress"])
            .assert()
            .success();

        docket_cmd()
            .current_dir(dir.path())
            .args(["show", &id])
            .assert()
            .success()
            .stdout(predicate::str::contains("in-progress"));
    }

    #[test]
    fn done_changes_status() {
        let dir = setup_docket_repo();
        let id = create_bug_and_get_id(&dir, "Done test");

        // Approve then set to in-progress
        docket_cmd()
            .current_dir(dir.path())
            .args(["approve", &id])
            .assert()
            .success();

        docket_cmd()
            .current_dir(dir.path())
            .args(["update", &id, "--status", "in-progress"])
            .assert()
            .success();

        // Then done
        docket_cmd()
            .current_dir(dir.path())
            .args(["done", &id])
            .assert()
            .success();

        // Need --all flag to see done bugs
        docket_cmd()
            .current_dir(dir.path())
            .args(["list", "--all"])
            .assert()
            .success()
            .stdout(predicate::str::contains("done"));
    }

    #[test]
    fn done_bugs_hidden_by_default() {
        let dir = setup_docket_repo();
        let id = create_bug_and_get_id(&dir, "Hidden when done");

        // Move through all statuses
        docket_cmd()
            .current_dir(dir.path())
            .args(["approve", &id])
            .assert()
            .success();

        docket_cmd()
            .current_dir(dir.path())
            .args(["update", &id, "--status", "in-progress"])
            .assert()
            .success();

        docket_cmd()
            .current_dir(dir.path())
            .args(["done", &id])
            .assert()
            .success();

        // Should not show in default list
        docket_cmd()
            .current_dir(dir.path())
            .arg("list")
            .assert()
            .success()
            .stdout(predicate::str::contains("Hidden when done").not());
    }
}

mod update_command {
    use super::*;

    #[test]
    fn update_title() {
        let dir = setup_docket_repo();

        docket_cmd()
            .current_dir(dir.path())
            .args(["new", "--title", "Original title"])
            .assert()
            .success();

        let list_output = docket_cmd()
            .current_dir(dir.path())
            .arg("list")
            .output()
            .unwrap();
        let output = String::from_utf8_lossy(&list_output.stdout);
        let id = output
            .lines()
            .find(|l| l.contains("Original title"))
            .and_then(|l| l.split_whitespace().next())
            .unwrap();

        docket_cmd()
            .current_dir(dir.path())
            .args(["update", id, "--title", "Updated title"])
            .assert()
            .success();

        docket_cmd()
            .current_dir(dir.path())
            .args(["show", id])
            .assert()
            .success()
            .stdout(predicate::str::contains("Updated title"));
    }

    #[test]
    fn update_priority() {
        let dir = setup_docket_repo();

        docket_cmd()
            .current_dir(dir.path())
            .args([
                "new",
                "--title",
                "Priority update test",
                "--priority",
                "low",
            ])
            .assert()
            .success();

        let list_output = docket_cmd()
            .current_dir(dir.path())
            .arg("list")
            .output()
            .unwrap();
        let output = String::from_utf8_lossy(&list_output.stdout);
        let id = output
            .lines()
            .find(|l| l.contains("Priority update test"))
            .and_then(|l| l.split_whitespace().next())
            .unwrap();

        docket_cmd()
            .current_dir(dir.path())
            .args(["update", id, "--priority", "high"])
            .assert()
            .success();

        docket_cmd()
            .current_dir(dir.path())
            .args(["show", id])
            .assert()
            .success()
            .stdout(predicate::str::contains("high"));
    }
}

mod log_command {
    use super::*;

    #[test]
    fn log_shows_event_history() {
        let dir = setup_docket_repo();

        docket_cmd()
            .current_dir(dir.path())
            .args(["new", "--title", "Log test bug"])
            .assert()
            .success();

        let list_output = docket_cmd()
            .current_dir(dir.path())
            .arg("list")
            .output()
            .unwrap();
        let output = String::from_utf8_lossy(&list_output.stdout);
        let id = output
            .lines()
            .find(|l| l.contains("Log test bug"))
            .and_then(|l| l.split_whitespace().next())
            .unwrap();

        // Make some changes
        docket_cmd()
            .current_dir(dir.path())
            .args(["approve", id])
            .assert()
            .success();

        docket_cmd()
            .current_dir(dir.path())
            .args(["update", id, "--title", "Updated log test"])
            .assert()
            .success();

        // Check log
        docket_cmd()
            .current_dir(dir.path())
            .args(["log", id])
            .assert()
            .success()
            .stdout(predicate::str::contains("created"))
            .stdout(predicate::str::contains("approved"));
    }
}
