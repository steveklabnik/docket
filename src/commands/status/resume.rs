//! Resume command - resume work on a paused bug.

use anyhow::{anyhow, Result};
use colored::Colorize;

use crate::bug::Status;
use crate::event::Event;
use crate::store::Store;

/// Resume a paused bug.
///
/// This transitions a bug from Paused back to InProgress status.
pub fn resume(id: &str) -> Result<()> {
    let store = Store::open()?;
    let bug = store.get_bug(id)?;
    let bug_id = bug.id().to_string();

    let old_status = bug.status().clone();

    // Validate transition - can only resume from Paused
    if !matches!(old_status, Status::Paused) {
        return Err(anyhow!(
            "can only resume bugs that are paused.\n\
             Bug '{}' is currently {}.",
            bug_id,
            old_status
        ));
    }

    // Emit Resumed event
    let event = Event::resumed(bug_id.clone());
    store.append_event(&event)?;

    println!(
        "{} Resumed bug {} ({} -> {})",
        "✓".green(),
        bug_id.cyan(),
        format!("{}", old_status).dimmed(),
        "in-progress".yellow().bold()
    );

    Ok(())
}
