//! Depend command - add a dependency between changes.

use anyhow::{anyhow, Result};
use colored::Colorize;

use crate::change::Status;
use crate::event::Event;
use crate::store::Store;

/// Add a dependency: change `id` is blocked by change `blocker_id`.
///
/// This creates an inter-change dependency without changing the change's status.
/// Use `docket block` to mark external blockers that change status to Blocked.
pub fn depend(id: &str, blocker_id: &str) -> Result<()> {
    let store = Store::open()?;

    // Resolve both change IDs
    let bug = store.get_change(id)?;
    let blocker_full_id = store.resolve_id(blocker_id)?;
    let blocker_bug = store.get_change(&blocker_full_id)?;

    // Prevent self-referential dependency
    if bug.id() == blocker_bug.id() {
        return Err(anyhow!("a change cannot depend on itself"));
    }

    // Check if dependency already exists
    if bug.is_blocked_by(&blocker_full_id) {
        return Err(anyhow!(
            "change {} already depends on {}",
            bug.id(),
            blocker_full_id
        ));
    }

    // Emit DependencyAdded event
    let event = Event::dependency_added(bug.id().to_string(), blocker_full_id.clone());
    store.append_event(&event)?;

    println!(
        "{} Change {} now depends on {} ({})",
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
