//! Undepend command - remove a dependency between bugs.

use anyhow::{anyhow, Result};
use colored::Colorize;

use crate::event::Event;
use crate::store::Store;

/// Remove a dependency: bug `id` is no longer blocked by bug `blocker_id`.
///
/// This removes an inter-bug dependency without changing the bug's status.
pub fn undepend(id: &str, blocker_id: &str) -> Result<()> {
    let store = Store::open()?;

    // Resolve both bug IDs
    let bug = store.get_bug(id)?;
    let blocker_full_id = store.resolve_id(blocker_id)?;

    // Check if dependency exists
    if !bug.is_blocked_by(&blocker_full_id) {
        return Err(anyhow!(
            "bug {} does not depend on {}",
            bug.id(),
            blocker_full_id
        ));
    }

    // Emit DependencyRemoved event
    let event = Event::dependency_removed(bug.id().to_string(), blocker_full_id.clone());
    store.append_event(&event)?;

    println!(
        "{} Bug {} no longer depends on {}",
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
            "  {} Still depends on: {}",
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
