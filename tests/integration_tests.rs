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
        assert!(dir.path().join(".docket/changes").exists());
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

    #[test]
    fn init_creates_state_branch_in_jj_repo() {
        let dir = TempDir::new().unwrap();

        // Initialize a jj repository first
        let jj_init = std::process::Command::new("jj")
            .args(["git", "init"])
            .current_dir(dir.path())
            .output();

        // Skip test if jj is not installed
        if jj_init.is_err() || !jj_init.as_ref().unwrap().status.success() {
            eprintln!("Skipping test: jj not installed or init failed");
            return;
        }

        // Get the current change id before init
        let before_output = std::process::Command::new("jj")
            .args(["log", "-r", "@", "--no-graph", "-T", "change_id"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let original_change = String::from_utf8_lossy(&before_output.stdout)
            .trim()
            .to_string();

        // Run docket init
        docket_cmd()
            .current_dir(dir.path())
            .arg("init")
            .assert()
            .success()
            .stdout(predicate::str::contains("docket-state"))
            .stdout(predicate::str::contains("orphan state branch"));

        // Verify the docket-state bookmark exists
        let bookmark_output = std::process::Command::new("jj")
            .args(["bookmark", "list"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let bookmarks = String::from_utf8_lossy(&bookmark_output.stdout);
        assert!(
            bookmarks.contains("docket-state"),
            "docket-state bookmark should exist. Got: {}",
            bookmarks
        );

        // Verify .docket/changes/.gitkeep exists on the state branch
        let file_list_output = std::process::Command::new("jj")
            .args(["file", "list", "-r", "docket-state"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let files = String::from_utf8_lossy(&file_list_output.stdout);
        // Normalize Windows backslashes to forward slashes for comparison
        let files_normalized = files.replace('\\', "/");
        assert!(
            files_normalized.contains(".docket/changes/.gitkeep"),
            ".docket/changes/.gitkeep should exist on state branch. Got: {}",
            files
        );

        // Verify we're back at the original change
        let after_output = std::process::Command::new("jj")
            .args(["log", "-r", "@", "--no-graph", "-T", "change_id"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let current_change = String::from_utf8_lossy(&after_output.stdout)
            .trim()
            .to_string();
        assert_eq!(
            original_change, current_change,
            "Should be back at original change after init"
        );
    }

    #[test]
    fn init_works_without_jj() {
        let dir = TempDir::new().unwrap();

        // Just run docket init without jj - should still work
        docket_cmd()
            .current_dir(dir.path())
            .arg("init")
            .assert()
            .success()
            .stdout(predicate::str::contains("Initialized docket"));

        // The basic .docket directory should exist
        assert!(dir.path().join(".docket").exists());
        assert!(dir.path().join(".docket/changes").exists());
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
            .stdout(predicate::str::contains("Created change"));

        // Verify a .jsonl file was created
        let bugs_dir = dir.path().join(".docket/changes");
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
            .stdout(predicate::str::contains("No changes found"));
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

    #[test]
    fn list_sorts_by_priority_by_default() {
        let dir = setup_docket_repo();

        // Create bugs in order: low, medium, high
        docket_cmd()
            .current_dir(dir.path())
            .args(["new", "--title", "Low bug", "--priority", "low"])
            .assert()
            .success();

        docket_cmd()
            .current_dir(dir.path())
            .args(["new", "--title", "Medium bug", "--priority", "medium"])
            .assert()
            .success();

        docket_cmd()
            .current_dir(dir.path())
            .args(["new", "--title", "High bug", "--priority", "high"])
            .assert()
            .success();

        // Default sort should show high first
        let output = docket_cmd()
            .current_dir(dir.path())
            .arg("list")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);

        let high_pos = stdout.find("High bug").unwrap();
        let medium_pos = stdout.find("Medium bug").unwrap();
        let low_pos = stdout.find("Low bug").unwrap();

        assert!(
            high_pos < medium_pos,
            "High priority should come before medium"
        );
        assert!(
            medium_pos < low_pos,
            "Medium priority should come before low"
        );
    }

    #[test]
    fn list_sort_by_created() {
        let dir = setup_docket_repo();

        // Create bugs
        docket_cmd()
            .current_dir(dir.path())
            .args(["new", "--title", "First created", "--priority", "low"])
            .assert()
            .success();

        docket_cmd()
            .current_dir(dir.path())
            .args(["new", "--title", "Second created", "--priority", "high"])
            .assert()
            .success();

        // Sort by created should show oldest first
        let output = docket_cmd()
            .current_dir(dir.path())
            .args(["list", "--sort", "created"])
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);

        let first_pos = stdout.find("First created").unwrap();
        let second_pos = stdout.find("Second created").unwrap();

        assert!(
            first_pos < second_pos,
            "First created should come before second"
        );
    }

    #[test]
    fn list_sort_by_status() {
        let dir = setup_docket_repo();

        // Create bugs with different statuses
        docket_cmd()
            .current_dir(dir.path())
            .args(["new", "--title", "Draft bug"])
            .assert()
            .success();

        // Get the ID of the draft bug
        let list_output = docket_cmd()
            .current_dir(dir.path())
            .arg("list")
            .output()
            .unwrap();
        let output_str = String::from_utf8_lossy(&list_output.stdout);
        let id = output_str
            .lines()
            .find(|l| l.contains("Draft bug"))
            .and_then(|l| l.split_whitespace().next())
            .unwrap()
            .to_string();

        // Approve it
        docket_cmd()
            .current_dir(dir.path())
            .args(["approve", &id])
            .assert()
            .success();

        // Create another draft bug
        docket_cmd()
            .current_dir(dir.path())
            .args(["new", "--title", "Another draft"])
            .assert()
            .success();

        // Sort by status should show approved before draft
        let output = docket_cmd()
            .current_dir(dir.path())
            .args(["list", "--sort", "status"])
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);

        let approved_pos = stdout.find("Draft bug").unwrap(); // This one is now approved
        let draft_pos = stdout.find("Another draft").unwrap();

        assert!(
            approved_pos < draft_pos,
            "Approved should come before draft"
        );
    }

    #[test]
    fn list_reverse_sort() {
        let dir = setup_docket_repo();

        docket_cmd()
            .current_dir(dir.path())
            .args(["new", "--title", "High bug", "--priority", "high"])
            .assert()
            .success();

        docket_cmd()
            .current_dir(dir.path())
            .args(["new", "--title", "Low bug", "--priority", "low"])
            .assert()
            .success();

        // Reverse sort should show low first
        let output = docket_cmd()
            .current_dir(dir.path())
            .args(["list", "--reverse"])
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);

        let high_pos = stdout.find("High bug").unwrap();
        let low_pos = stdout.find("Low bug").unwrap();

        assert!(
            low_pos < high_pos,
            "Low priority should come before high with --reverse"
        );
    }

    #[test]
    fn list_invalid_sort_field() {
        let dir = setup_docket_repo();

        docket_cmd()
            .current_dir(dir.path())
            .args(["list", "--sort", "invalid"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("unknown sort field"));
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

    #[test]
    fn show_supports_fuzzy_matching() {
        let dir = setup_docket_repo();

        docket_cmd()
            .current_dir(dir.path())
            .args(["new", "--title", "Fuzzy match test"])
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
            .find(|l| l.contains("Fuzzy match test"))
            .and_then(|l| l.split_whitespace().next())
            .unwrap();

        // Create a fuzzy query by removing the second character
        // e.g., "abc1" -> "ac1"
        let mut fuzzy_query: String = full_id.chars().collect();
        if fuzzy_query.len() >= 2 {
            fuzzy_query.remove(1);
        }

        docket_cmd()
            .current_dir(dir.path())
            .args(["show", &fuzzy_query])
            .assert()
            .success()
            .stdout(predicate::str::contains("Fuzzy match test"));
    }

    #[test]
    fn show_fuzzy_matching_with_missing_first_char() {
        let dir = setup_docket_repo();

        docket_cmd()
            .current_dir(dir.path())
            .args(["new", "--title", "Fuzzy first char test"])
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
            .find(|l| l.contains("Fuzzy first char test"))
            .and_then(|l| l.split_whitespace().next())
            .unwrap();

        // Create a fuzzy query by removing the first character
        // e.g., "abc1" -> "bc1"
        let fuzzy_query: String = full_id.chars().skip(1).collect();

        docket_cmd()
            .current_dir(dir.path())
            .args(["show", &fuzzy_query])
            .assert()
            .success()
            .stdout(predicate::str::contains("Fuzzy first char test"));
    }

    #[test]
    fn show_prefers_exact_over_fuzzy() {
        let dir = setup_docket_repo();

        // Create first bug
        docket_cmd()
            .current_dir(dir.path())
            .args(["new", "--title", "First bug for exact test"])
            .assert()
            .success();

        // Get the ID
        let list_output = docket_cmd()
            .current_dir(dir.path())
            .arg("list")
            .output()
            .unwrap();
        let output = String::from_utf8_lossy(&list_output.stdout);
        let full_id = output
            .lines()
            .find(|l| l.contains("First bug for exact test"))
            .and_then(|l| l.split_whitespace().next())
            .unwrap();

        // Exact match should work
        docket_cmd()
            .current_dir(dir.path())
            .args(["show", full_id])
            .assert()
            .success()
            .stdout(predicate::str::contains("First bug for exact test"));
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

mod ready_command {
    use super::*;

    fn create_bug_and_get_id(dir: &TempDir, title: &str, priority: &str) -> String {
        docket_cmd()
            .current_dir(dir.path())
            .args(["new", "--title", title, "--priority", priority])
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
    fn ready_shows_no_changes_when_none_approved() {
        let dir = setup_docket_repo();

        // Create a draft change
        docket_cmd()
            .current_dir(dir.path())
            .args(["new", "--title", "Draft change"])
            .assert()
            .success();

        // Ready should show no changes
        docket_cmd()
            .current_dir(dir.path())
            .arg("ready")
            .assert()
            .success()
            .stdout(predicate::str::contains("No approved changes"));
    }

    #[test]
    fn ready_shows_single_approved_bug() {
        let dir = setup_docket_repo();

        let id = create_bug_and_get_id(&dir, "Ready test bug", "high");

        // Approve it
        docket_cmd()
            .current_dir(dir.path())
            .args(["approve", &id])
            .assert()
            .success();

        // Ready should show this bug
        docket_cmd()
            .current_dir(dir.path())
            .arg("ready")
            .assert()
            .success()
            .stdout(predicate::str::contains("Ready to work"))
            .stdout(predicate::str::contains("Ready test bug"))
            .stdout(predicate::str::contains("high"));
    }

    #[test]
    fn ready_shows_highest_priority_first() {
        let dir = setup_docket_repo();

        // Create bugs with different priorities
        let low_id = create_bug_and_get_id(&dir, "Low priority bug", "low");
        let high_id = create_bug_and_get_id(&dir, "High priority bug", "high");
        let medium_id = create_bug_and_get_id(&dir, "Medium priority bug", "medium");

        // Approve all
        for id in [&low_id, &high_id, &medium_id] {
            docket_cmd()
                .current_dir(dir.path())
                .args(["approve", id])
                .assert()
                .success();
        }

        // Default ready should show high priority bug
        docket_cmd()
            .current_dir(dir.path())
            .arg("ready")
            .assert()
            .success()
            .stdout(predicate::str::contains("High priority bug"));

        // ready -n 3 should show all in priority order
        let output = docket_cmd()
            .current_dir(dir.path())
            .args(["ready", "-n", "3"])
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);

        let high_pos = stdout.find("High priority bug").unwrap();
        let medium_pos = stdout.find("Medium priority bug").unwrap();
        let low_pos = stdout.find("Low priority bug").unwrap();

        assert!(
            high_pos < medium_pos,
            "High priority should come before medium"
        );
        assert!(
            medium_pos < low_pos,
            "Medium priority should come before low"
        );
    }

    #[test]
    fn ready_count_flag_limits_output() {
        let dir = setup_docket_repo();

        // Create and approve multiple bugs
        for i in 1..=5 {
            let id = create_bug_and_get_id(&dir, &format!("Bug {}", i), "medium");
            docket_cmd()
                .current_dir(dir.path())
                .args(["approve", &id])
                .assert()
                .success();
        }

        // ready -n 2 should show exactly 2 bugs (numbered list format)
        let output = docket_cmd()
            .current_dir(dir.path())
            .args(["ready", "-n", "2"])
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Count numbered entries
        let numbered_lines = stdout
            .lines()
            .filter(|l| l.starts_with("1.") || l.starts_with("2."))
            .count();
        assert_eq!(numbered_lines, 2, "Should show exactly 2 bugs");

        // Should not have a third entry
        assert!(!stdout.contains("3."), "Should not show a third bug");
    }

    #[test]
    fn ready_excludes_non_approved_bugs() {
        let dir = setup_docket_repo();

        // Create bugs with different statuses
        let _draft_id = create_bug_and_get_id(&dir, "Draft bug", "high");
        let approved_id = create_bug_and_get_id(&dir, "Approved bug", "medium");

        // Only approve one
        docket_cmd()
            .current_dir(dir.path())
            .args(["approve", &approved_id])
            .assert()
            .success();

        // Ready should only show the approved bug
        docket_cmd()
            .current_dir(dir.path())
            .arg("ready")
            .assert()
            .success()
            .stdout(predicate::str::contains("Approved bug"))
            .stdout(predicate::str::contains("Draft bug").not());

        // If we mark the approved one as in-progress, it should not appear in ready
        docket_cmd()
            .current_dir(dir.path())
            .args(["update", &approved_id, "--status", "in-progress"])
            .assert()
            .success();

        docket_cmd()
            .current_dir(dir.path())
            .arg("ready")
            .assert()
            .success()
            .stdout(predicate::str::contains("No approved changes"));
    }
}

mod record_command {
    use super::*;

    #[test]
    fn record_creates_change_in_done_status() {
        let dir = setup_docket_repo();

        docket_cmd()
            .current_dir(dir.path())
            .args(["record", "Fix memory leak in parser"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Recorded completed change"))
            .stdout(predicate::str::contains("Status: done"));

        // Verify the change exists with done status
        let list_output = docket_cmd()
            .current_dir(dir.path())
            .args(["list", "--all"])
            .output()
            .unwrap();
        let output = String::from_utf8_lossy(&list_output.stdout);
        assert!(output.contains("Fix memory leak in parser"));
        assert!(output.contains("done"));
    }

    #[test]
    fn record_with_changelog_type() {
        let dir = setup_docket_repo();

        docket_cmd()
            .current_dir(dir.path())
            .args(["record", "Add new feature", "--changelog", "feature"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Changelog: feature"));

        // Show should display changelog type
        let list_output = docket_cmd()
            .current_dir(dir.path())
            .args(["list", "--all"])
            .output()
            .unwrap();
        let output = String::from_utf8_lossy(&list_output.stdout);
        let id = output
            .lines()
            .find(|l| l.contains("Add new feature"))
            .and_then(|l| l.split_whitespace().next())
            .unwrap();

        docket_cmd()
            .current_dir(dir.path())
            .args(["show", id])
            .assert()
            .success()
            .stdout(predicate::str::contains("feature"));
    }

    #[test]
    fn record_with_release() {
        let dir = setup_docket_repo();

        // Create a release first
        docket_cmd()
            .current_dir(dir.path())
            .args(["release", "new", "1.0.0"])
            .assert()
            .success();

        docket_cmd()
            .current_dir(dir.path())
            .args(["record", "Bug fix for release", "--release", "1.0.0"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Release: 1.0.0"));

        // Verify it appears in release show
        docket_cmd()
            .current_dir(dir.path())
            .args(["release", "show", "1.0.0"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Bug fix for release"));
    }

    #[test]
    fn record_with_pr_and_commit_references() {
        let dir = setup_docket_repo();

        docket_cmd()
            .current_dir(dir.path())
            .args([
                "record",
                "External contribution",
                "--pr",
                "123",
                "--commit",
                "abc1234",
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("References added to body"));

        // Show should display the references in the body
        let list_output = docket_cmd()
            .current_dir(dir.path())
            .args(["list", "--all"])
            .output()
            .unwrap();
        let output = String::from_utf8_lossy(&list_output.stdout);
        let id = output
            .lines()
            .find(|l| l.contains("External contribution"))
            .and_then(|l| l.split_whitespace().next())
            .unwrap();

        docket_cmd()
            .current_dir(dir.path())
            .args(["show", id])
            .assert()
            .success()
            .stdout(predicate::str::contains("PR: #123"))
            .stdout(predicate::str::contains("Commit: abc1234"));
    }

    #[test]
    fn record_with_body_from_stdin() {
        let dir = setup_docket_repo();

        docket_cmd()
            .current_dir(dir.path())
            .args(["record", "Change with description", "--body", "-"])
            .write_stdin("This is the detailed description of the work done.")
            .assert()
            .success();

        // Show should display the body
        let list_output = docket_cmd()
            .current_dir(dir.path())
            .args(["list", "--all"])
            .output()
            .unwrap();
        let output = String::from_utf8_lossy(&list_output.stdout);
        let id = output
            .lines()
            .find(|l| l.contains("Change with description"))
            .and_then(|l| l.split_whitespace().next())
            .unwrap();

        docket_cmd()
            .current_dir(dir.path())
            .args(["show", id])
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "This is the detailed description of the work done.",
            ));
    }

    #[test]
    fn record_with_parent() {
        let dir = setup_docket_repo();

        // Create a parent change first
        docket_cmd()
            .current_dir(dir.path())
            .args(["new", "--title", "Parent epic"])
            .assert()
            .success();

        // Get the parent ID
        let list_output = docket_cmd()
            .current_dir(dir.path())
            .arg("list")
            .output()
            .unwrap();
        let output = String::from_utf8_lossy(&list_output.stdout);
        let parent_id = output
            .lines()
            .find(|l| l.contains("Parent epic"))
            .and_then(|l| l.split_whitespace().next())
            .unwrap()
            .to_string();

        // Record a child change
        docket_cmd()
            .current_dir(dir.path())
            .args(["record", "Child task", "--parent", &parent_id])
            .assert()
            .success()
            .stdout(predicate::str::contains(&format!("under {}", parent_id)));
    }

    #[test]
    fn record_fails_for_nonexistent_release() {
        let dir = setup_docket_repo();

        docket_cmd()
            .current_dir(dir.path())
            .args(["record", "Some work", "--release", "9.9.9"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("does not exist"));
    }
}

mod migrate_command {
    use super::*;

    #[test]
    fn migrate_requires_jj_repo() {
        let dir = setup_docket_repo();

        // Without a jj repo, migrate should fail
        docket_cmd()
            .current_dir(dir.path())
            .arg("migrate")
            .assert()
            .failure()
            .stderr(predicate::str::contains("requires a jj repository"));
    }

    #[test]
    fn migrate_moves_changes_to_state_branch() {
        let dir = TempDir::new().unwrap();

        // Initialize a jj repository first
        let jj_init = std::process::Command::new("jj")
            .args(["git", "init"])
            .current_dir(dir.path())
            .output();

        // Skip test if jj is not installed
        if jj_init.is_err() || !jj_init.as_ref().unwrap().status.success() {
            eprintln!("Skipping test: jj not installed or init failed");
            return;
        }

        // Initialize docket (this creates .docket in working tree AND state branch)
        docket_cmd()
            .current_dir(dir.path())
            .arg("init")
            .assert()
            .success();

        // Create a couple of changes
        docket_cmd()
            .current_dir(dir.path())
            .args(["new", "--title", "First change"])
            .assert()
            .success();

        docket_cmd()
            .current_dir(dir.path())
            .args(["new", "--title", "Second change"])
            .assert()
            .success();

        // Get the change IDs from the list
        let list_output = docket_cmd()
            .current_dir(dir.path())
            .arg("list")
            .output()
            .unwrap();
        let list_stdout = String::from_utf8_lossy(&list_output.stdout);

        // Verify changes exist
        assert!(
            list_stdout.contains("First change"),
            "First change should exist before migration"
        );
        assert!(
            list_stdout.contains("Second change"),
            "Second change should exist before migration"
        );

        // Verify .docket exists in working tree before migration
        assert!(
            dir.path().join(".docket").exists(),
            ".docket should exist in working tree before migration"
        );

        // Run migrate
        docket_cmd()
            .current_dir(dir.path())
            .arg("migrate")
            .assert()
            .success()
            .stdout(predicate::str::contains("Migration complete"))
            .stdout(predicate::str::contains("changes"));

        // Verify .docket is removed from working tree
        assert!(
            !dir.path().join(".docket").exists(),
            ".docket should be removed from working tree after migration"
        );

        // Verify the state branch has the data
        let file_list = std::process::Command::new("jj")
            .args(["file", "list", "-r", "docket-state"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let files = String::from_utf8_lossy(&file_list.stdout);
        // Normalize Windows backslashes to forward slashes for comparison
        let files_normalized = files.replace('\\', "/");
        assert!(
            files_normalized.contains(".docket/changes/"),
            "State branch should contain .docket/changes/"
        );

        // Verify we can still list changes (now from state branch)
        let list_after = docket_cmd()
            .current_dir(dir.path())
            .arg("list")
            .output()
            .unwrap();
        let list_after_stdout = String::from_utf8_lossy(&list_after.stdout);

        assert!(
            list_after_stdout.contains("First change"),
            "First change should exist after migration"
        );
        assert!(
            list_after_stdout.contains("Second change"),
            "Second change should exist after migration"
        );
    }

    #[test]
    fn migrate_is_idempotent() {
        let dir = TempDir::new().unwrap();

        // Initialize a jj repository first
        let jj_init = std::process::Command::new("jj")
            .args(["git", "init"])
            .current_dir(dir.path())
            .output();

        // Skip test if jj is not installed
        if jj_init.is_err() || !jj_init.as_ref().unwrap().status.success() {
            eprintln!("Skipping test: jj not installed or init failed");
            return;
        }

        // Initialize docket
        docket_cmd()
            .current_dir(dir.path())
            .arg("init")
            .assert()
            .success();

        // Create a change
        docket_cmd()
            .current_dir(dir.path())
            .args(["new", "--title", "Test change"])
            .assert()
            .success();

        // Run migrate first time
        docket_cmd()
            .current_dir(dir.path())
            .arg("migrate")
            .assert()
            .success()
            .stdout(predicate::str::contains("Migration complete"));

        // Run migrate second time - should succeed but indicate already migrated
        docket_cmd()
            .current_dir(dir.path())
            .arg("migrate")
            .assert()
            .success()
            .stdout(predicate::str::contains("Already using state branch mode"));
    }

    #[test]
    fn migrate_preserves_event_history() {
        let dir = TempDir::new().unwrap();

        // Initialize a jj repository first
        let jj_init = std::process::Command::new("jj")
            .args(["git", "init"])
            .current_dir(dir.path())
            .output();

        // Skip test if jj is not installed
        if jj_init.is_err() || !jj_init.as_ref().unwrap().status.success() {
            eprintln!("Skipping test: jj not installed or init failed");
            return;
        }

        // Initialize docket
        docket_cmd()
            .current_dir(dir.path())
            .arg("init")
            .assert()
            .success();

        // Create a change and make some updates to create history
        let new_output = docket_cmd()
            .current_dir(dir.path())
            .args(["new", "--title", "Change with history"])
            .output()
            .unwrap();
        let new_stdout = String::from_utf8_lossy(&new_output.stdout);

        // Extract the change ID from output like "✓ Created change abcd - Title"
        let change_id = new_stdout
            .lines()
            .find(|l| l.contains("Created change"))
            .and_then(|l| l.split_whitespace().nth(3)) // Get the ID: ✓(0) Created(1) change(2) ID(3)
            .unwrap();

        // Update the change to create more events
        docket_cmd()
            .current_dir(dir.path())
            .args(["update", change_id, "--priority", "high"])
            .assert()
            .success();

        docket_cmd()
            .current_dir(dir.path())
            .args(["approve", change_id])
            .assert()
            .success();

        // Read the change file before migration to count events
        let change_path = dir
            .path()
            .join(".docket")
            .join("changes")
            .join(&change_id[..1])
            .join(format!("{}.jsonl", change_id));
        let content_before = fs::read_to_string(&change_path).unwrap();
        let events_before = content_before
            .lines()
            .filter(|l| !l.trim().is_empty())
            .count();

        // Run migrate
        docket_cmd()
            .current_dir(dir.path())
            .arg("migrate")
            .assert()
            .success();

        // Read the events directly from state branch after migration
        let state_file_output = std::process::Command::new("jj")
            .args([
                "file",
                "show",
                "-r",
                "docket-state",
                &format!(".docket/changes/{}/{}.jsonl", &change_id[..1], change_id),
            ])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let content_after = String::from_utf8_lossy(&state_file_output.stdout);
        let events_after = content_after
            .lines()
            .filter(|l| !l.trim().is_empty())
            .count();

        assert_eq!(
            events_before,
            events_after,
            "Event history should be preserved after migration.\n\
             Before ({} events): {}\n\
             After ({} events): {}",
            events_before,
            content_before.trim(),
            events_after,
            content_after.trim()
        );

        // Also verify we can list the change after migration (uses state branch)
        let list_after = docket_cmd()
            .current_dir(dir.path())
            .arg("list")
            .output()
            .unwrap();
        let list_stdout = String::from_utf8_lossy(&list_after.stdout);
        assert!(
            list_stdout.contains("Change with history"),
            "Change should be listed after migration"
        );
    }
}
