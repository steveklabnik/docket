use anyhow::Result;
use colored::Colorize;

use crate::bug::Status;
use crate::store::Store;

pub fn show(id: &str) -> Result<()> {
    let store = Store::open()?;
    let bug = store.get_bug(id)?;

    // Print header
    println!("{} {}", bug.id().cyan().bold(), bug.title().bold());
    println!("{}", "-".repeat(60).dimmed());

    // Print metadata
    let status_str = format!("{}", bug.status());
    let status_colored = match bug.status() {
        Status::Draft => status_str.dimmed(),
        Status::Approved => status_str.green(),
        Status::InProgress => status_str.yellow(),
        Status::Done => status_str.blue(),
        Status::NotPlanned => status_str.red(),
    };

    println!("{:12} {}", "Status:".dimmed(), status_colored);
    println!("{:12} {}", "Priority:".dimmed(), bug.priority());
    println!(
        "{:12} {}",
        "Created:".dimmed(),
        bug.metadata.created.format("%Y-%m-%d %H:%M")
    );

    // Show changelog type if set
    if let Some(ct) = bug.changelog_type() {
        println!("{:12} {}", "Changelog:".dimmed(), ct);
    }

    // Show versions if any
    if !bug.versions().is_empty() {
        println!("{:12} {}", "Versions:".dimmed(), bug.versions().join(", "));
    }

    // Show tags if any
    if !bug.tags().is_empty() {
        let mut tags: Vec<_> = bug.tags().iter().collect();
        tags.sort();
        println!(
            "{:12} {}",
            "Tags:".dimmed(),
            tags.iter()
                .map(|t| t.yellow().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    println!();

    // Print body
    println!("{}", bug.body);

    Ok(())
}
