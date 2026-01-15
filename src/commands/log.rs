use anyhow::Result;
use colored::Colorize;

use crate::event::EventData;
use crate::store::Store;

pub fn log(id: &str, json: bool) -> Result<()> {
    let store = Store::open()?;

    // Resolve prefix to full ID
    let full_id = store.resolve_id(id)?;

    // Get all events for this bug
    let events = store.get_events(&full_id)?;

    if json {
        let json_output = serde_json::to_string_pretty(&events)?;
        println!("{}", json_output);
        return Ok(());
    }

    if events.is_empty() {
        println!("{} No events found for bug {}", "→".blue(), full_id.cyan());
        return Ok(());
    }

    println!("{} Event history for bug {}\n", "→".blue(), full_id.cyan());

    for event in &events {
        let timestamp = event.timestamp.format("%Y-%m-%d %H:%M:%S UTC");
        let actor = event.actor.as_deref().unwrap_or("unknown");

        match &event.data {
            EventData::Created {
                title, priority, ..
            } => {
                println!(
                    "{} {} [{}]",
                    timestamp.to_string().dimmed(),
                    "created".green(),
                    actor.dimmed()
                );
                println!("    Title: {}", title);
                println!("    Priority: {}", priority);
            }
            EventData::StatusChanged { from, to } => {
                println!(
                    "{} {} [{}]",
                    timestamp.to_string().dimmed(),
                    "status_changed".yellow(),
                    actor.dimmed()
                );
                println!(
                    "    {} -> {}",
                    format!("{}", from).dimmed(),
                    format!("{}", to).green()
                );
            }
            EventData::Updated { title, body } => {
                println!(
                    "{} {} [{}]",
                    timestamp.to_string().dimmed(),
                    "updated".blue(),
                    actor.dimmed()
                );
                if let Some(t) = title {
                    println!("    Title: {}", t);
                }
                if body.is_some() {
                    println!("    Body updated");
                }
            }
            EventData::PriorityChanged { from, to } => {
                println!(
                    "{} {} [{}]",
                    timestamp.to_string().dimmed(),
                    "priority_changed".yellow(),
                    actor.dimmed()
                );
                println!(
                    "    {} -> {}",
                    format!("{}", from).dimmed(),
                    format!("{}", to).green()
                );
            }
            EventData::ChangeLinked { change_id } => {
                println!(
                    "{} {} [{}]",
                    timestamp.to_string().dimmed(),
                    "change_linked".magenta(),
                    actor.dimmed()
                );
                println!("    Change: {}", change_id.cyan());
            }
            EventData::ChangelogTypeSet { changelog_type } => {
                println!(
                    "{} {} [{}]",
                    timestamp.to_string().dimmed(),
                    "changelog_type_set".cyan(),
                    actor.dimmed()
                );
                println!("    Changelog type: {}", changelog_type);
            }
            EventData::VersionAdded { version } => {
                println!(
                    "{} {} [{}]",
                    timestamp.to_string().dimmed(),
                    "version_added".green(),
                    actor.dimmed()
                );
                println!("    Version: {}", version.cyan());
            }
            EventData::VersionRemoved { version } => {
                println!(
                    "{} {} [{}]",
                    timestamp.to_string().dimmed(),
                    "version_removed".red(),
                    actor.dimmed()
                );
                println!("    Version: {}", version);
            }
            EventData::TagAdded { tag } => {
                println!(
                    "{} {} [{}]",
                    timestamp.to_string().dimmed(),
                    "tag_added".green(),
                    actor.dimmed()
                );
                println!("    Tag: {}", tag.cyan());
            }
            EventData::TagRemoved { tag } => {
                println!(
                    "{} {} [{}]",
                    timestamp.to_string().dimmed(),
                    "tag_removed".red(),
                    actor.dimmed()
                );
                println!("    Tag: {}", tag);
            }
        }
        println!();
    }

    println!("{} {} event(s) total", "→".blue(), events.len());

    Ok(())
}
