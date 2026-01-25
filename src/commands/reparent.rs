use anyhow::{bail, Result};
use colored::Colorize;

use crate::event::Event;
use crate::store::Store;

/// Move a change under a different parent (or make it top-level)
pub fn reparent(id: &str, new_parent: Option<&str>) -> Result<()> {
    let store = Store::open()?;
    let bug = store.get_change(id)?;
    let full_id = bug.id().to_string();
    let old_parent = bug.parent().map(|s| s.to_string());

    // Resolve new parent if provided
    let new_parent_id = if let Some(parent_ref) = new_parent {
        let parent = store.get_change(parent_ref)?;

        // Prevent circular references
        if parent.id() == full_id {
            bail!("cannot make a change its own parent");
        }

        // Check if the new parent is a descendant of this change (would create a cycle)
        if is_descendant(&store, parent.id(), &full_id)? {
            bail!(
                "cannot reparent: {} is a descendant of {}",
                parent.id(),
                full_id
            );
        }

        Some(parent.id().to_string())
    } else {
        None
    };

    // Check if this is actually a change
    if old_parent == new_parent_id {
        if let Some(parent) = &old_parent {
            println!(
                "{} {} is already under {}",
                "→".blue(),
                full_id.cyan(),
                parent.cyan()
            );
        } else {
            println!(
                "{} {} is already a top-level change",
                "→".blue(),
                full_id.cyan()
            );
        }
        return Ok(());
    }

    // Create and append the ParentChanged event
    let event = Event::parent_changed(full_id.clone(), old_parent.clone(), new_parent_id.clone());
    store.append_event(&event)?;

    match (&old_parent, &new_parent_id) {
        (None, Some(new)) => {
            println!(
                "{} Moved {} under {}",
                "✓".green(),
                full_id.cyan(),
                new.cyan()
            );
        }
        (Some(old), None) => {
            println!(
                "{} Moved {} to top-level (was under {})",
                "✓".green(),
                full_id.cyan(),
                old.cyan()
            );
        }
        (Some(old), Some(new)) => {
            println!(
                "{} Moved {} from {} to {}",
                "✓".green(),
                full_id.cyan(),
                old.cyan(),
                new.cyan()
            );
        }
        (None, None) => unreachable!(), // Already handled above
    }

    Ok(())
}

/// Check if `potential_descendant` is a descendant of `ancestor`
fn is_descendant(store: &Store, potential_descendant: &str, ancestor: &str) -> Result<bool> {
    let all_bugs = store.list_changes()?;

    let mut current = Some(potential_descendant.to_string());
    while let Some(id) = current {
        if id == ancestor {
            return Ok(true);
        }
        // Find this bug and get its parent
        current = all_bugs
            .iter()
            .find(|b| b.id() == id)
            .and_then(|b| b.parent().map(|s| s.to_string()));
    }

    Ok(false)
}
