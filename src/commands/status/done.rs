//! The `done` command workflow for completing bugs.

use std::process::Command;

use anyhow::{anyhow, Result};
use colored::Colorize;

use crate::bug::Status;
use crate::commands::epic;
use crate::event::Event;
use crate::store::Store;

use super::jj;

/// Result of parsing checkboxes from bug body
#[derive(Debug, PartialEq)]
pub struct CheckboxStatus {
    pub checked: usize,
    pub unchecked: usize,
}

impl CheckboxStatus {
    /// Returns true if all checkboxes are checked (and there is at least one)
    pub fn all_checked(&self) -> bool {
        self.unchecked == 0 && self.checked > 0
    }

    /// Returns true if there are no checkboxes at all
    pub fn has_no_checkboxes(&self) -> bool {
        self.checked == 0 && self.unchecked == 0
    }
}

/// Parse checkbox markers from a bug body.
/// Looks for `- [ ]` (unchecked) and `- [x]` or `- [X]` (checked) patterns.
pub fn parse_checkboxes(body: &str) -> CheckboxStatus {
    let mut checked = 0;
    let mut unchecked = 0;

    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("- [ ]") {
            unchecked += 1;
        } else if trimmed.starts_with("- [x]") || trimmed.starts_with("- [X]") {
            checked += 1;
        }
    }

    CheckboxStatus { checked, unchecked }
}

/// Extract content from between `<commit>` tags in Claude's output.
/// Falls back to the full text if tags aren't found.
fn extract_commit_message(output: &str) -> String {
    let trimmed = output.trim();

    // Look for <commit>...</commit> tags
    if let Some(start) = trimmed.find("<commit>") {
        let after_tag = &trimmed[start + 8..]; // 8 = len("<commit>")
        if let Some(end) = after_tag.find("</commit>") {
            return after_tag[..end].trim().to_string();
        }
    }

    // Fallback: return trimmed output as-is
    trimmed.to_string()
}

/// Generate a commit message using Claude Code.
/// Returns None if Claude fails (caller should fall back to simple message).
fn generate_commit_message(bug_id: &str) -> Option<String> {
    println!("{} Generating commit message with Claude...", "→".blue());

    let output = Command::new("claude")
        .args([
            "-p",
            "/docket-describe",
            "--model",
            "haiku",
            "--allowedTools",
            "Bash,Read",
        ])
        .env("DOCKET_BUG", bug_id)
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let raw_output = String::from_utf8_lossy(&output.stdout);
            let message = extract_commit_message(&raw_output);
            if message.is_empty() {
                eprintln!(
                    "{} Claude returned empty message, using fallback",
                    "!".yellow()
                );
                None
            } else {
                Some(message)
            }
        }
        Ok(output) => {
            eprintln!(
                "{} Claude failed (exit {}), using fallback message",
                "!".yellow(),
                output.status.code().unwrap_or(-1)
            );
            if !output.stderr.is_empty() {
                eprintln!("{}", String::from_utf8_lossy(&output.stderr).dimmed());
            }
            None
        }
        Err(e) => {
            eprintln!(
                "{} Failed to run Claude ({}), using fallback message",
                "!".yellow(),
                e
            );
            None
        }
    }
}

fn fallback_commit_message(bug_title: &str, bug_id: &str) -> String {
    format!("Implement {} ({})", bug_title, bug_id)
}

/// Mark a bug as done, handling workspace integration if applicable.
///
/// If `auto` is true, validates that all acceptance criteria checkboxes are checked
/// before marking the bug as done. If `force` is true along with `auto`, marks
/// the bug as done even if unchecked criteria remain.
pub fn done(id: &str, auto: bool, force: bool) -> Result<()> {
    let store = Store::open()?;
    let bug = store.get_bug(id)?;

    let old_status = bug.status().clone();
    let bug_id = bug.id().to_string();
    let bug_title = bug.title().to_string();

    if matches!(old_status, Status::Done) {
        return Err(anyhow!(
            "bug '{}' is already marked as done.\n\
             Use 'docket show {}' to view the bug details.",
            bug_id,
            bug_id
        ));
    }

    // If --auto is set, validate that all checkboxes are checked
    if auto {
        let checkbox_status = parse_checkboxes(&bug.body);

        if checkbox_status.has_no_checkboxes() {
            return Err(anyhow!(
                "cannot use --auto: bug '{}' has no acceptance criteria checkboxes.\n\
                 Add checkboxes to the bug body using `- [ ]` format, or omit --auto.",
                bug_id
            ));
        }

        if !checkbox_status.all_checked() {
            if force {
                println!(
                    "{} Forcing completion with {} unchecked criteria",
                    "!".yellow(),
                    checkbox_status.unchecked
                );
            } else {
                return Err(anyhow!(
                    "cannot mark bug '{}' as done: {} of {} acceptance criteria are unchecked.\n\
                     Check all criteria with `- [x]` or use --force to override.",
                    bug_id,
                    checkbox_status.unchecked,
                    checkbox_status.checked + checkbox_status.unchecked
                ));
            }
        }
    }

    // Track if we switched directories (so we can switch back)
    let original_dir = std::env::current_dir().ok();
    let mut switched_to_workspace = false;

    // Check if we're running from a workspace
    let mut in_workspace = jj::is_in_workspace(&bug_id);

    // If not in workspace, check if one exists and switch to it
    if !in_workspace {
        if let Some(workspace_dir) = jj::find_workspace_dir(&bug_id) {
            println!(
                "{} Found workspace at {}, switching...",
                "→".blue(),
                workspace_dir.display()
            );
            std::env::set_current_dir(&workspace_dir)?;
            switched_to_workspace = true;
            in_workspace = true;
        }
    }

    // Re-open store after potential directory switch so events are written to the
    // workspace's .docket (which will be part of the jj commit history when merged)
    let store = Store::open()?;

    // Emit StatusChanged event BEFORE snapshot so it's captured in the jj commit
    let status_event = Event::status_changed(bug_id.clone(), old_status.clone(), Status::Done);
    store.append_event(&status_event)?;

    if in_workspace {
        // Running from workspace - do the full workflow
        println!(
            "{} Running from workspace for bug {}",
            "→".blue(),
            bug_id.cyan()
        );

        // Generate commit message using Claude, with fallback to simple message
        let commit_message = generate_commit_message(&bug_id)
            .unwrap_or_else(|| fallback_commit_message(&bug_title, &bug_id));
        jj::describe(&commit_message)?;
    }

    println!(
        "{} Completed bug {} ({} -> {})",
        "✓".green(),
        bug_id.cyan(),
        format!("{}", old_status).dimmed(),
        format!("{}", Status::Done).green()
    );

    // Check if this was a child of an epic and if all siblings are now done
    if let Some(parent_epic_id) = bug.parent_epic() {
        if epic::all_children_done(&store, parent_epic_id)? {
            // Auto-close the parent epic
            let parent_epic = store.get_bug(parent_epic_id)?;
            if !matches!(parent_epic.status(), Status::Done) {
                let epic_event = Event::status_changed(
                    parent_epic_id.to_string(),
                    parent_epic.status().clone(),
                    Status::Done,
                );
                store.append_event(&epic_event)?;
                println!(
                    "{} All steps complete - epic {} is now done!",
                    "✓".green(),
                    parent_epic_id.cyan()
                );
            }
        } else {
            // Show progress
            let (completed, total) = epic::epic_progress(&store, parent_epic_id)?;
            println!(
                "{} Epic {} progress: {}/{}",
                "→".blue(),
                parent_epic_id.cyan(),
                completed,
                total
            );
        }
    }

    // Return to original directory if we switched
    if switched_to_workspace {
        if let Some(ref orig) = original_dir {
            std::env::set_current_dir(orig)?;
            println!("{} Returned to {}", "→".blue(), orig.display());
        }
    }

    // Only create a fresh jj change if NOT in a workspace
    // (workspace changes stay as-is for review/submission)
    if !in_workspace {
        jj::create_fresh_change_if_needed()?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_commit_message_with_tags() {
        let output = r#"Based on the changes, here's the commit message:

<commit>
Add user authentication

This implements OAuth2 login flow for the application.
</commit>

Let me know if you need changes!"#;

        let result = extract_commit_message(output);
        assert_eq!(
            result,
            "Add user authentication\n\nThis implements OAuth2 login flow for the application."
        );
    }

    #[test]
    fn extract_commit_message_without_tags() {
        let output = "Add user authentication\n\nSimple commit message.";
        let result = extract_commit_message(output);
        assert_eq!(result, output);
    }

    #[test]
    fn extract_commit_message_empty_tags() {
        let output = "<commit></commit>";
        let result = extract_commit_message(output);
        assert_eq!(result, "");
    }

    #[test]
    fn parse_checkboxes_all_checked() {
        let body = r#"## Acceptance Criteria

- [x] First criterion
- [x] Second criterion
- [x] Third criterion
"#;
        let status = parse_checkboxes(body);
        assert_eq!(status.checked, 3);
        assert_eq!(status.unchecked, 0);
        assert!(status.all_checked());
    }

    #[test]
    fn parse_checkboxes_all_unchecked() {
        let body = r#"## Acceptance Criteria

- [ ] First criterion
- [ ] Second criterion
"#;
        let status = parse_checkboxes(body);
        assert_eq!(status.checked, 0);
        assert_eq!(status.unchecked, 2);
        assert!(!status.all_checked());
    }

    #[test]
    fn parse_checkboxes_mixed() {
        let body = r#"## Acceptance Criteria

- [x] Completed item
- [ ] Pending item
- [X] Another completed (uppercase X)
- [ ] Another pending
"#;
        let status = parse_checkboxes(body);
        assert_eq!(status.checked, 2);
        assert_eq!(status.unchecked, 2);
        assert!(!status.all_checked());
    }

    #[test]
    fn parse_checkboxes_none() {
        let body = r#"## Goal

Just some text without checkboxes.

## Context

More text here.
"#;
        let status = parse_checkboxes(body);
        assert_eq!(status.checked, 0);
        assert_eq!(status.unchecked, 0);
        assert!(status.has_no_checkboxes());
        assert!(!status.all_checked()); // all_checked requires at least one checkbox
    }

    #[test]
    fn parse_checkboxes_with_indentation() {
        let body = r#"## Acceptance Criteria

  - [x] Indented checked
    - [ ] More indented unchecked
"#;
        let status = parse_checkboxes(body);
        assert_eq!(status.checked, 1);
        assert_eq!(status.unchecked, 1);
    }

    #[test]
    fn parse_checkboxes_ignores_non_checkbox_lists() {
        let body = r#"## Notes

- Regular list item
- Another regular item
- [x] This is a checkbox
- [not a checkbox]
"#;
        let status = parse_checkboxes(body);
        assert_eq!(status.checked, 1);
        assert_eq!(status.unchecked, 0);
    }

    #[test]
    fn checkbox_status_methods() {
        let all_checked = CheckboxStatus {
            checked: 3,
            unchecked: 0,
        };
        assert!(all_checked.all_checked());
        assert!(!all_checked.has_no_checkboxes());

        let none = CheckboxStatus {
            checked: 0,
            unchecked: 0,
        };
        assert!(!none.all_checked());
        assert!(none.has_no_checkboxes());

        let some_unchecked = CheckboxStatus {
            checked: 2,
            unchecked: 1,
        };
        assert!(!some_unchecked.all_checked());
        assert!(!some_unchecked.has_no_checkboxes());
    }
}
