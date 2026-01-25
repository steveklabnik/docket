//! Undepend command - remove a dependency between changes.

use anyhow::{anyhow, Result};
use colored::Colorize;

use crate::event::Event;
use crate::store::Store;

/// Remove a dependency: change `id` is no longer blocked by change `blocker_id`.
///
/// This removes an inter-change dependency without changing the change's status.
pub fn undepend(id: &str, blocker_id: &str) -> Result<()> {
    let store = Store::open()?;

    // Resolve both change IDs
    let bug = store.get_change(id)?;
    let blocker_full_id = store.resolve_id(blocker_id)?;

    // Check if dependency exists
    if !bug.is_blocked_by(&blocker_full_id) {
        return Err(anyhow!(
            "change {} does not depend on {}",
            bug.id(),
            blocker_full_id
        ));
    }

    // Emit DependencyRemoved event
    let event = Event::dependency_removed(bug.id().to_string(), blocker_full_id.clone());
    store.append_event(&event)?;

    println!(
        "{} Change {} no longer depends on {}",
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
