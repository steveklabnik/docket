//! Unblock command - mark a blocked change as back in progress or remove a dependency.

use anyhow::{anyhow, Result};
use colored::Colorize;

use crate::change::Status;
use crate::event::Event;
use crate::store::Store;

/// Mark a blocked change as back in progress or remove a dependency.
///
/// If `by` is provided, this removes an inter-change dependency.
/// If `by` is not provided, this changes the change's status from Blocked to InProgress.
pub fn unblock(id: &str, by: Option<String>) -> Result<()> {
    let store = Store::open()?;
    let bug = store.get_change(id)?;
    let bug_id = bug.id().to_string();

    // Handle removing inter-change dependency
    if let Some(blocker_id) = by {
        return remove_dependency(&store, &bug_id, &blocker_id);
    }

    // Handle external unblock (status change)
    let old_status = bug.status().clone();

    // Validate transition - can only unblock from Blocked
    if !matches!(old_status, Status::Blocked) {
        return Err(anyhow!(
            "can only unblock changes that are blocked.\n\
             Change '{}' is currently {}.",
            bug_id,
            old_status
        ));
    }

    // Emit Unblocked event
    let event = Event::unblocked(bug_id.clone());
    store.append_event(&event)?;

    println!(
        "{} Unblocked change {} ({} -> {})",
        "✓".green(),
        bug_id.cyan(),
        "blocked".red(),
        "in-progress".yellow()
    );

    Ok(())
}

/// Remove an inter-change dependency.
fn remove_dependency(store: &Store, bug_id: &str, blocker_id: &str) -> Result<()> {
    // Resolve the blocker ID
    let blocker_full_id = store.resolve_id(blocker_id)?;

    // Get the change to check existing dependencies
    let bug = store.get_change(bug_id)?;

    // Check if dependency exists
    if !bug.is_blocked_by(&blocker_full_id) {
        return Err(anyhow!(
            "change {} is not blocked by {}",
            bug.id(),
            blocker_full_id
        ));
    }

    // Emit DependencyRemoved event
    let event = Event::dependency_removed(bug.id().to_string(), blocker_full_id.clone());
    store.append_event(&event)?;

    println!(
        "{} Change {} is no longer blocked by {}",
        "✓".green(),
        bug.id().cyan(),
        blocker_full_id.cyan()
    );

    // Show remaining dependencies
    let remaining: Vec<_> = bug
        .blocked_by()
        .iter()
        .filter(|id| *id != &blocker_full_id)
        .collect();
    if !remaining.is_empty() {
        println!(
            "  {} Still blocked by: {}",
            "→".blue(),
            remaining
                .iter()
                .map(|id| id.cyan().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    Ok(())
}
