use anyhow::Result;
use colored::Colorize;

use crate::bug::{Bug, Status};
use crate::store::Store;

pub fn show(id: &str) -> Result<()> {
    let store = Store::open()?;
    let bug = store.get_bug(id)?;
    let all_bugs = store.list_bugs()?;

    // Print header
    println!("{} {}", bug.id().cyan().bold(), bug.title().bold());
    println!("{}", "-".repeat(60).dimmed());

    // Print metadata
    let status_str = format!("{}", bug.status());
    let status_colored = match bug.status() {
        Status::Draft => status_str.dimmed(),
        Status::Approved => status_str.green(),
        Status::InProgress => status_str.yellow(),
        Status::Blocked => status_str.red().bold(),
        Status::Paused => status_str.cyan(),
        Status::Review => status_str.magenta(),
        Status::Done => status_str.blue(),
        Status::NotPlanned => status_str.red(),
    };

    println!("{:12} {}", "Status:".dimmed(), status_colored);

    // Show blocked reason if blocked
    if let Some(reason) = bug.blocked_reason() {
        println!("{:12} {}", "Blocked:".dimmed(), reason.red());
    }

    // Show paused reason if paused
    if let Some(reason) = bug.paused_reason() {
        println!("{:12} {}", "Paused:".dimmed(), reason.cyan());
    }

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

    // Show dependencies (blocked by)
    if bug.has_dependencies() {
        let blocked_by = format_blocked_by(&bug, &all_bugs);
        println!("{:12} {}", "Blocked by:".dimmed(), blocked_by);
    }

    // Show reverse dependencies (blocks)
    let blocks = find_bugs_blocked_by(bug.id(), &all_bugs);
    if !blocks.is_empty() {
        let blocks_str = blocks
            .iter()
            .map(|b| {
                let status_indicator = if matches!(b.status(), Status::Done) {
                    "✓".green().to_string()
                } else {
                    "○".dimmed().to_string()
                };
                format!("{} {} ({})", status_indicator, b.id().cyan(), b.title())
            })
            .collect::<Vec<_>>()
            .join(", ");
        println!("{:12} {}", "Blocks:".dimmed(), blocks_str);
    }

    println!();

    // Print body
    println!("{}", bug.body);

    Ok(())
}

/// Format blocked-by dependencies with status indicators
fn format_blocked_by(bug: &Bug, all_bugs: &[Bug]) -> String {
    let mut deps: Vec<_> = bug.blocked_by().iter().collect();
    deps.sort();

    deps.iter()
        .map(|blocker_id| {
            // Find the blocker bug to check its status
            if let Some(blocker) = all_bugs.iter().find(|b| b.id() == *blocker_id) {
                let status_indicator = if matches!(blocker.status(), Status::Done) {
                    "✓".green().to_string()
                } else {
                    "○".yellow().to_string()
                };
                format!(
                    "{} {} ({})",
                    status_indicator,
                    blocker_id.cyan(),
                    blocker.title()
                )
            } else {
                format!("{} (not found)", blocker_id.cyan())
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Find all bugs that are blocked by the given bug ID
fn find_bugs_blocked_by<'a>(blocker_id: &str, all_bugs: &'a [Bug]) -> Vec<&'a Bug> {
    all_bugs
        .iter()
        .filter(|bug| bug.is_blocked_by(blocker_id))
        .collect()
}
