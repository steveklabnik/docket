//! Unblock command - mark a blocked bug as back in progress.

use anyhow::{anyhow, Result};
use colored::Colorize;

use crate::bug::Status;
use crate::event::Event;
use crate::store::Store;

/// Mark a blocked bug as back in progress.
pub fn unblock(id: &str) -> Result<()> {
    let store = Store::open()?;
    let bug = store.get_bug(id)?;

    let old_status = bug.status().clone();
    let bug_id = bug.id().to_string();

    // Validate transition - can only unblock from Blocked
    if !matches!(old_status, Status::Blocked) {
        return Err(anyhow!(
            "can only unblock bugs that are blocked.\n\
             Bug '{}' is currently {}.",
            bug_id,
            old_status
        ));
    }

    // Emit Unblocked event
    let event = Event::unblocked(bug_id.clone());
    store.append_event(&event)?;

    println!(
        "{} Unblocked bug {} ({} -> {})",
        "✓".green(),
        bug_id.cyan(),
        "blocked".red(),
        "in-progress".yellow()
    );

    Ok(())
}
