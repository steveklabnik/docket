//! The `reject` command for rejecting changes from review back to in progress.

use anyhow::{anyhow, Result};
use colored::Colorize;

use crate::change::Status;
use crate::event::Event;
use crate::store::Store;

/// Reject a change from review back to in progress (Review -> InProgress).
pub fn reject(id: &str) -> Result<()> {
    let store = Store::open()?;
    let bug = store.get_change(id)?;

    let old_status = bug.status().clone();
    let bug_id = bug.id().to_string();

    // Only allow transition from Review
    if !matches!(old_status, Status::Review) {
        return Err(anyhow!(
            "change '{}' is not in review (current status: {}).\n\
             Only changes that are in review can be rejected.",
            bug_id,
            old_status
        ));
    }

    // Emit StatusChanged event
    let event = Event::status_changed(bug_id.clone(), old_status.clone(), Status::InProgress);
    store.append_event(&event)?;

    println!(
        "{} Rejected change {} from review ({} -> {})",
        "✓".green(),
        bug_id.cyan(),
        format!("{}", old_status).dimmed(),
        format!("{}", Status::InProgress).yellow()
    );

    Ok(())
}
