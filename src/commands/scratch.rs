use anyhow::Result;
use colored::Colorize;

use crate::event::Event;
use crate::store::Store;

/// Append a note to a change's scratchpad
pub fn scratch(id: &str, content: &str) -> Result<()> {
    let store = Store::open()?;
    let bug = store.get_change(id)?;
    let full_id = bug.id().to_string();

    // Create and append the scratchpad event
    let event = Event::scratchpad_appended(full_id.clone(), content.to_string());
    store.append_event(&event)?;

    println!(
        "{} Appended note to {} scratchpad",
        "✓".green(),
        full_id.cyan()
    );

    Ok(())
}
