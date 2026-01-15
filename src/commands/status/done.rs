//! The `done` command workflow for completing bugs.

use std::process::Command;

use anyhow::{anyhow, Result};
use colored::Colorize;

use crate::bug::Status;
use crate::event::Event;
use crate::store::Store;

use super::jj;

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
pub fn done(id: &str) -> Result<()> {
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
}
