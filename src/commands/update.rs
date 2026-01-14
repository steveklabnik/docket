use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use std::fs;
use std::io::{self, Read};

use crate::bug::Priority;
use crate::event::Event;
use crate::store::Store;

/// Read body content from a file path or stdin (if path is "-")
fn read_body_from_source(source: &str) -> Result<String> {
    if source == "-" {
        let mut content = String::new();
        io::stdin()
            .read_to_string(&mut content)
            .context("failed to read body from stdin")?;
        Ok(content)
    } else {
        fs::read_to_string(source).with_context(|| format!("failed to read body from '{}'", source))
    }
}

pub fn update(
    id: &str,
    title: Option<String>,
    body_source: Option<&str>,
    priority_str: Option<&str>,
) -> Result<()> {
    let store = Store::open()?;
    let bug = store.get_bug(id)?;
    let bug_id = bug.metadata.id.clone();

    // Parse priority if provided
    let priority: Option<Priority> = match priority_str {
        Some(p) => Some(
            p.parse()
                .map_err(|_| anyhow!("invalid priority '{}', must be low, medium, or high", p))?,
        ),
        None => None,
    };

    // Read body if provided
    let body: Option<String> = match body_source {
        Some(source) => Some(read_body_from_source(source)?),
        None => None,
    };

    // Check if there's anything to update
    if title.is_none() && body.is_none() && priority.is_none() {
        println!(
            "{} Nothing to update. Provide --title, --body, or --priority.",
            "!".yellow()
        );
        return Ok(());
    }

    // Handle priority change separately (needs its own event type in the future,
    // but for now we emit an Updated event with the priority in the title)
    // Actually, looking at the event system, Updated only supports title and body.
    // We need to handle priority differently.

    // For now, emit Updated event for title/body changes
    if title.is_some() || body.is_some() {
        let event = Event::updated(bug_id.clone(), title.clone(), body);
        store.append_event(&event)?;
    }

    // Priority changes require extending the event system
    if let Some(new_priority) = priority {
        // Check if priority actually changed
        if new_priority != bug.metadata.priority {
            let event =
                Event::priority_changed(bug_id.clone(), bug.metadata.priority, new_priority);
            store.append_event(&event)?;
        }
    }

    // Build summary of changes
    let mut changes = Vec::new();
    if title.is_some() {
        changes.push("title");
    }
    if body_source.is_some() {
        changes.push("body");
    }
    if priority_str.is_some() {
        changes.push("priority");
    }

    println!(
        "{} Updated bug {} ({})",
        "✓".green(),
        bug_id.cyan(),
        changes.join(", ")
    );

    Ok(())
}
