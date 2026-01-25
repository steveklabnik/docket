use anyhow::{bail, Result};
use colored::Colorize;

use crate::event::Event;
use crate::store::Store;

pub fn untag(id: &str, tag: &str) -> Result<()> {
    let store = Store::open()?;
    let bug = store.get_change(id)?;

    // Check if the tag exists
    if !bug.has_tag(tag) {
        bail!(
            "Change {} does not have tag '{}'",
            bug.id().cyan(),
            tag.yellow()
        );
    }

    // Emit TagRemoved event
    let event = Event::tag_removed(bug.id().to_string(), tag.to_string());
    store.append_event(&event)?;

    println!(
        "{} Removed tag '{}' from {} - {}",
        "✓".green(),
        tag.yellow(),
        bug.id().cyan(),
        bug.title()
    );

    Ok(())
}
