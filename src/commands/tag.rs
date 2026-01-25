use anyhow::{bail, Result};
use colored::Colorize;

use crate::event::Event;
use crate::store::Store;

pub fn tag(id: &str, tag: &str) -> Result<()> {
    let store = Store::open()?;
    let bug = store.get_change(id)?;

    // Check if the tag already exists
    if bug.has_tag(tag) {
        bail!(
            "Change {} already has tag '{}'",
            bug.id().cyan(),
            tag.yellow()
        );
    }

    // Emit TagAdded event
    let event = Event::tag_added(bug.id().to_string(), tag.to_string());
    store.append_event(&event)?;

    println!(
        "{} Added tag '{}' to {} - {}",
        "✓".green(),
        tag.yellow(),
        bug.id().cyan(),
        bug.title()
    );

    Ok(())
}
