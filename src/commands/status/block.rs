//! Block command - mark a bug as blocked on external dependency.

use anyhow::{anyhow, Result};
use colored::Colorize;

use crate::bug::Status;
use crate::event::Event;
use crate::store::Store;

/// Mark a bug as blocked on external dependency.
pub fn block(id: &str, reason: Option<String>) -> Result<()> {
    let store = Store::open()?;
    let bug = store.get_bug(id)?;

    let old_status = bug.status().clone();
    let bug_id = bug.id().to_string();

    // Validate transition - can only block from InProgress
    if !matches!(old_status, Status::InProgress) {
        return Err(anyhow!(
            "can only block bugs that are in-progress.\n\
             Bug '{}' is currently {}.",
            bug_id,
            old_status
        ));
    }

    // Emit Blocked event
    let event = Event::blocked(bug_id.clone(), old_status.clone(), reason.clone());
    store.append_event(&event)?;

    if let Some(ref reason) = reason {
        println!(
            "{} Blocked bug {} ({} -> {})",
            "✓".green(),
            bug_id.cyan(),
            format!("{}", old_status).dimmed(),
            "blocked".red().bold()
        );
        println!("  {} {}", "Reason:".dimmed(), reason);
    } else {
        println!(
            "{} Blocked bug {} ({} -> {})",
            "✓".green(),
            bug_id.cyan(),
            format!("{}", old_status).dimmed(),
            "blocked".red().bold()
        );
    }

    Ok(())
}
