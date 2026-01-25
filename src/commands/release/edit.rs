use anyhow::{Context, Result};
use colored::Colorize;

use crate::release::ReleaseEvent;
use crate::store::Store;

pub fn edit(version: &str) -> Result<()> {
    let store = Store::open()?;
    let release = store.get_release(version)?;

    // Prepare the content for editing
    // Format: title on first line, blank line, description
    let current_content = format!(
        "{}\n\n{}",
        release.title().unwrap_or(""),
        release.description()
    );

    // Open editor
    let edited = dialoguer::Editor::new()
        .edit(&current_content)
        .context("failed to open editor")?;

    let edited = match edited {
        Some(content) => content,
        None => {
            println!("{} No changes made", "→".blue());
            return Ok(());
        }
    };

    // Parse the edited content
    // First non-empty line is title, rest is description
    let lines: Vec<&str> = edited.lines().collect();
    let new_title = lines
        .iter()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim().to_string());

    // Find where title ends and description begins
    let title_end = lines
        .iter()
        .position(|l| !l.trim().is_empty())
        .map(|i| i + 1)
        .unwrap_or(0);

    // Skip any blank lines after title
    let desc_start = lines[title_end..]
        .iter()
        .position(|l| !l.trim().is_empty())
        .map(|i| title_end + i)
        .unwrap_or(lines.len());

    let new_description: String = if desc_start < lines.len() {
        lines[desc_start..].join("\n").trim().to_string()
    } else {
        String::new()
    };

    // Check if anything changed
    let title_changed = new_title.as_deref() != release.title();
    let description_changed = new_description != release.description();

    if !title_changed && !description_changed {
        println!("{} No changes made", "→".blue());
        return Ok(());
    }

    // Create update event
    let event = ReleaseEvent::updated(
        version.to_string(),
        if title_changed { new_title } else { None },
        if description_changed {
            Some(new_description)
        } else {
            None
        },
        None, // Don't change target_date via edit
    );

    store.append_release_event(&event)?;

    println!("{} Updated release {}", "✓".green(), version.cyan().bold());

    Ok(())
}
