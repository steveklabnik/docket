//! Block command - mark a bug as blocked on external dependency or another bug.

use anyhow::{anyhow, Result};
use colored::Colorize;

use crate::bug::Status;
use crate::event::Event;
use crate::store::Store;

/// Mark a bug as blocked on external dependency or another bug.
///
/// If `by` is provided, this adds an inter-bug dependency (bug is blocked by another bug).
/// If `by` is not provided, this changes the bug's status to Blocked (external blocker).
pub fn block(id: &str, reason: Option<String>, by: Option<String>) -> Result<()> {
    let store = Store::open()?;
    let bug = store.get_bug(id)?;
    let bug_id = bug.id().to_string();

    // Handle inter-bug dependency
    if let Some(blocker_id) = by {
        return add_dependency(&store, &bug_id, &blocker_id);
    }

    // Handle external blocker (status change)
    let old_status = bug.status().clone();

    // Validate transition - can only block from InProgress
    if !matches!(old_status, Status::InProgress) {
        return Err(anyhow!(
            "can only block bugs that are in-progress.\n\
             Bug '{}' is currently {}.",
            bug_id,
            old_status
        ));
    }

    // Emit Blocked event
    let event = Event::blocked(bug_id.clone(), old_status.clone(), reason.clone());
    store.append_event(&event)?;

    if let Some(ref reason) = reason {
        println!(
            "{} Blocked bug {} ({} -> {})",
            "✓".green(),
            bug_id.cyan(),
            format!("{}", old_status).dimmed(),
            "blocked".red().bold()
        );
        println!("  {} {}", "Reason:".dimmed(), reason);
    } else {
        println!(
            "{} Blocked bug {} ({} -> {})",
            "✓".green(),
            bug_id.cyan(),
            format!("{}", old_status).dimmed(),
            "blocked".red().bold()
        );
    }

    Ok(())
}

/// Add an inter-bug dependency (bug is blocked by another bug).
fn add_dependency(store: &Store, bug_id: &str, blocker_id: &str) -> Result<()> {
    // Resolve the blocker ID
    let blocker_full_id = store.resolve_id(blocker_id)?;

    // Verify the blocker bug exists
    let blocker_bug = store.get_bug(&blocker_full_id)?;

    // Get the bug again to check existing dependencies
    let bug = store.get_bug(bug_id)?;

    // Prevent self-referential dependency
    if bug.id() == blocker_bug.id() {
        return Err(anyhow!("a bug cannot block itself"));
    }

    // Check if dependency already exists
    if bug.is_blocked_by(&blocker_full_id) {
        return Err(anyhow!(
            "bug {} is already blocked by {}",
            bug.id(),
            blocker_full_id
        ));
    }

    // Emit DependencyAdded event
    let event = Event::dependency_added(bug.id().to_string(), blocker_full_id.clone());
    store.append_event(&event)?;

    println!(
        "{} Bug {} is now blocked by {} ({})",
        "✓".green(),
        bug.id().cyan(),
        blocker_full_id.cyan(),
        blocker_bug.title().dimmed()
    );

    // Show warning if blocker is not done
    if !matches!(blocker_bug.status(), Status::Done) {
        println!(
            "  {} {} is {} (not done yet)",
            "!".yellow(),
            blocker_full_id.cyan(),
            blocker_bug.status()
        );
    }

    Ok(())
}
