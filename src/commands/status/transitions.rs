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
            "cannot change status of completed bug '{}'",
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
    set_status(id, Status::Approved, "Approved")
}

/// Mark a bug as in-progress.
pub fn start(id: &str) -> Result<()> {
    set_status(id, Status::InProgress, "Started")
}
