//! Depend command - add a dependency between bugs.

use anyhow::{anyhow, Result};
use colored::Colorize;

use crate::bug::Status;
use crate::event::Event;
use crate::store::Store;

/// Add a dependency: bug `id` is blocked by bug `blocker_id`.
///
/// This creates an inter-bug dependency without changing the bug's status.
/// Use `docket block` to mark external blockers that change status to Blocked.
pub fn depend(id: &str, blocker_id: &str) -> Result<()> {
    let store = Store::open()?;

    // Resolve both bug IDs
    let bug = store.get_bug(id)?;
    let blocker_full_id = store.resolve_id(blocker_id)?;
    let blocker_bug = store.get_bug(&blocker_full_id)?;

    // Prevent self-referential dependency
    if bug.id() == blocker_bug.id() {
        return Err(anyhow!("a bug cannot depend on itself"));
    }

    // Check if dependency already exists
    if bug.is_blocked_by(&blocker_full_id) {
        return Err(anyhow!(
            "bug {} already depends on {}",
            bug.id(),
            blocker_full_id
        ));
    }

    // Emit DependencyAdded event
    let event = Event::dependency_added(bug.id().to_string(), blocker_full_id.clone());
    store.append_event(&event)?;

    println!(
        "{} Bug {} now depends on {} ({})",
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
