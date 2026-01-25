//! Pause command - intentionally set aside a bug.

use anyhow::{anyhow, Result};
use colored::Colorize;

use crate::change::Status;
use crate::event::Event;
use crate::store::Store;

/// Pause a bug that is in progress.
///
/// This transitions a bug from InProgress to Paused status.
/// Unlike Blocked, Paused indicates an intentional choice to set work aside.
pub fn pause(id: &str, reason: Option<String>) -> Result<()> {
    let store = Store::open()?;
    let bug = store.get_change(id)?;
    let bug_id = bug.id().to_string();

    let old_status = bug.status().clone();

    // Validate transition - can only pause from InProgress
    if !matches!(old_status, Status::InProgress) {
        return Err(anyhow!(
            "can only pause bugs that are in-progress.\n\
             Bug '{}' is currently {}.",
            bug_id,
            old_status
        ));
    }

    // Emit Paused event
    let event = Event::paused(bug_id.clone(), old_status.clone(), reason.clone());
    store.append_event(&event)?;

    if let Some(ref reason) = reason {
        println!(
            "{} Paused bug {} ({} -> {})",
            "✓".green(),
            bug_id.cyan(),
            format!("{}", old_status).dimmed(),
            "paused".yellow().bold()
        );
        println!("  {} {}", "Reason:".dimmed(), reason);
    } else {
        println!(
            "{} Paused bug {} ({} -> {})",
            "✓".green(),
            bug_id.cyan(),
            format!("{}", old_status).dimmed(),
            "paused".yellow().bold()
        );
    }

    Ok(())
}
