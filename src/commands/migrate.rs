use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::path::Path;

use crate::bug::Bug;
use crate::event::Event;
use crate::store::Store;

/// Migrate markdown bugs to JSONL format
pub fn migrate() -> Result<()> {
    let store = Store::open()?;
    let bugs_dir = store.root().join("bugs");

    // Find all .md files
    let pattern = bugs_dir.join("*.md");
    let pattern_str = pattern
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("invalid path encoding"))?;

    let md_files: Vec<_> = glob::glob(pattern_str)?.collect::<Result<Vec<_>, _>>()?;

    if md_files.is_empty() {
        println!("{} No markdown bugs to migrate", "→".blue());
        return Ok(());
    }

    println!(
        "{} Found {} markdown bug(s) to migrate\n",
        "→".blue(),
        md_files.len()
    );

    let mut migrated = 0;
    let mut skipped = 0;

    for md_path in md_files {
        let bug_id = md_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow::anyhow!("invalid file name"))?;

        // Check if JSONL already exists
        let jsonl_path = bugs_dir.join(format!("{}.jsonl", bug_id));
        if jsonl_path.exists() {
            println!(
                "{} Skipping {} (JSONL already exists)",
                "→".blue(),
                bug_id.cyan()
            );
            skipped += 1;
            continue;
        }

        // Parse the markdown file
        let content = fs::read_to_string(&md_path)
            .with_context(|| format!("failed to read {}", md_path.display()))?;

        let bug = Bug::parse(&content)
            .with_context(|| format!("failed to parse {}", md_path.display()))?;

        // Create a Created event with the original timestamp
        let mut event = Event::created(
            bug.id().to_string(),
            bug.title().to_string(),
            bug.priority().clone(),
            bug.body.clone(),
        );
        // Use the original creation timestamp
        event.timestamp = bug.metadata.created;

        // Write the Created event
        crate::event::append_event(&jsonl_path, &event)?;

        // If status is not Draft, emit StatusChanged events
        migrate_status(&jsonl_path, &bug)?;

        // If there are linked changes, emit ChangeLinked events
        for change_id in bug.changes() {
            let link_event = Event::change_linked(bug.id().to_string(), change_id.clone());
            crate::event::append_event(&jsonl_path, &link_event)?;
        }

        println!(
            "{} Migrated {} - {}",
            "✓".green(),
            bug_id.cyan(),
            bug.title()
        );
        migrated += 1;
    }

    println!(
        "\n{} Migration complete: {} migrated, {} skipped",
        "✓".green(),
        migrated,
        skipped
    );

    if migrated > 0 {
        println!("\n{} You can now delete the .md files with:", "→".blue());
        println!("    rm {}/*.md", bugs_dir.display());
    }

    Ok(())
}

/// Emit StatusChanged events to reach the bug's current status
fn migrate_status(jsonl_path: &Path, bug: &Bug) -> Result<()> {
    use crate::bug::Status;

    let transitions: Vec<(Status, Status)> = match bug.status() {
        Status::Draft => vec![],
        Status::Approved => vec![(Status::Draft, Status::Approved)],
        Status::InProgress => vec![
            (Status::Draft, Status::Approved),
            (Status::Approved, Status::InProgress),
        ],
        Status::Done => vec![
            (Status::Draft, Status::Approved),
            (Status::Approved, Status::InProgress),
            (Status::InProgress, Status::Done),
        ],
    };

    for (from, to) in transitions {
        let event = Event::status_changed(bug.id().to_string(), from, to);
        crate::event::append_event(jsonl_path, &event)?;
    }

    Ok(())
}
