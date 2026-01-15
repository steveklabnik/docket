//! The `review` command for submitting bugs for code review.

use anyhow::{anyhow, Result};
use colored::Colorize;

use crate::bug::Status;
use crate::event::Event;
use crate::store::Store;

/// Submit a bug for code review (InProgress -> Review).
pub fn review(id: &str) -> Result<()> {
    let store = Store::open()?;
    let bug = store.get_bug(id)?;

    let old_status = bug.status().clone();
    let bug_id = bug.id().to_string();

    // Only allow transition from InProgress
    if !matches!(old_status, Status::InProgress) {
        return Err(anyhow!(
            "bug '{}' is not in progress (current status: {}).\n\
             Only bugs that are in progress can be submitted for review.",
            bug_id,
            old_status
        ));
    }

    // Emit StatusChanged event
    let event = Event::status_changed(bug_id.clone(), old_status.clone(), Status::Review);
    store.append_event(&event)?;

    println!(
        "{} Submitted bug {} for review ({} -> {})",
        "✓".green(),
        bug_id.cyan(),
        format!("{}", old_status).dimmed(),
        format!("{}", Status::Review).magenta()
    );

    Ok(())
}
