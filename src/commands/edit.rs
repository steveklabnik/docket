//! The `edit` command for editing a bug's body in an editor.

use anyhow::{Context, Result};
use colored::Colorize;
use dialoguer::Editor;

use crate::event::Event;
use crate::store::Store;

/// Edit a bug's body in the user's preferred editor.
pub fn edit(id: &str) -> Result<()> {
    let store = Store::open()?;
    let bug = store.get_bug(id)?;
    let bug_id = bug.metadata.id.clone();

    // Launch editor with current body content
    let edited = Editor::new()
        .extension(".md")
        .edit(&bug.body)
        .context("failed to open editor")?;

    match edited {
        Some(new_body) => {
            // Check if body actually changed
            if new_body == bug.body {
                println!("{} No changes made", "→".blue());
                return Ok(());
            }

            // Emit Updated event with new body
            let event = Event::updated(bug_id.clone(), None, Some(new_body));
            store.append_event(&event)?;

            println!("{} Updated bug {} body", "✓".green(), bug_id.cyan());
        }
        None => {
            println!("{} Edit cancelled", "→".blue());
        }
    }

    Ok(())
}
