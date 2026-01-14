//! The `done` command workflow for completing bugs.

use anyhow::{anyhow, Result};
use colored::Colorize;

use crate::bug::Status;
use crate::event::Event;
use crate::store::Store;

use super::jj;

/// Mark a bug as done, handling workspace integration if applicable.
pub fn done(id: &str) -> Result<()> {
    let store = Store::open()?;
    let bug = store.get_bug(id)?;

    let old_status = bug.status().clone();
    let bug_id = bug.id().to_string();
    let bug_title = bug.title().to_string();

    if matches!(old_status, Status::Done) {
        return Err(anyhow!(
            "cannot change status of completed bug '{}'",
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

        // Set commit message (jj describe auto-snapshots, capturing the StatusChanged event)
        let commit_message = format!("Implement {} ({})", bug_title, bug_id);
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
