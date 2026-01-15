//! Simple status transition functions.

use anyhow::{anyhow, Result};
use colored::Colorize;

use crate::bug::Status;
use crate::event::Event;
use crate::store::Store;

/// Change a bug's status with validation.
fn set_status(id: &str, new_status: Status, action: &str) -> Result<()> {
    let store = Store::open()?;
    let bug = store.get_bug(id)?;

    let old_status = bug.status().clone();
    let bug_id = bug.id().to_string();

    // Validate transition
    if matches!(old_status, Status::Done) {
        return Err(anyhow!(
            "bug '{}' is already marked as done.\n\
             Use 'docket show {}' to view the bug details.",
            bug_id,
            bug_id
        ));
    }

    // Emit StatusChanged event
    let event = Event::status_changed(bug_id.clone(), old_status.clone(), new_status.clone());
    store.append_event(&event)?;

    println!(
        "{} {} bug {} ({} -> {})",
        "✓".green(),
        action,
        bug_id.cyan(),
        format!("{}", old_status).dimmed(),
        format!("{}", new_status).green()
    );

    Ok(())
}

/// Mark a bug as approved for work.
pub fn approve(id: &str) -> Result<()> {
    let store = Store::open()?;
    let bug = store.get_bug(id)?;

    // Check for unresolved dependencies and warn
    if bug.has_dependencies() {
        let all_bugs = store.list_bugs()?;
        let unresolved: Vec<_> = bug
            .blocked_by()
            .iter()
            .filter_map(|blocker_id| {
                all_bugs
                    .iter()
                    .find(|b| b.id() == blocker_id)
                    .filter(|b| !matches!(b.status(), Status::Done))
            })
            .collect();

        if !unresolved.is_empty() {
            eprintln!(
                "{} Bug {} has {} unresolved dependenc{}:",
                "!".yellow(),
                bug.id().cyan(),
                unresolved.len(),
                if unresolved.len() == 1 { "y" } else { "ies" }
            );
            for blocker in &unresolved {
                eprintln!(
                    "  {} {} - {} ({})",
                    "○".yellow(),
                    blocker.id().cyan(),
                    blocker.title(),
                    blocker.status()
                );
            }
            eprintln!();
        }
    }

    set_status(id, Status::Approved, "Approved")
}
