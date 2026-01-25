use anyhow::Result;
use colored::Colorize;

use crate::change::Status;
use crate::release::ReleaseStatus;
use crate::store::Store;

pub fn show(version: &str) -> Result<()> {
    let store = Store::open()?;
    let release = store.get_release(version)?;

    // Print header
    println!(
        "{} {}",
        release.version().cyan().bold(),
        release
            .title()
            .map(|t| format!("- {}", t))
            .unwrap_or_default()
            .bold()
    );
    println!("{}", "-".repeat(60).dimmed());

    // Print metadata
    let status_str = format!("{}", release.status());
    let status_colored = match release.status() {
        ReleaseStatus::Planning => status_str.dimmed(),
        ReleaseStatus::Active => status_str.green(),
        ReleaseStatus::Frozen => status_str.cyan().bold(),
        ReleaseStatus::Released => status_str.blue(),
        ReleaseStatus::Cancelled => status_str.red(),
    };

    println!("{:12} {}", "Status:".dimmed(), status_colored);
    println!(
        "{:12} {}",
        "Created:".dimmed(),
        release.created().format("%Y-%m-%d %H:%M")
    );

    if let Some(target) = release.target_date() {
        println!("{:12} {}", "Target:".dimmed(), target.format("%Y-%m-%d"));
    }

    if let Some(released) = release.released_date() {
        println!(
            "{:12} {}",
            "Released:".dimmed(),
            released.format("%Y-%m-%d")
        );
    }

    // Get and display progress
    let changes = store.get_changes_for_release(version)?;
    let total = changes.len();
    let completed = changes
        .iter()
        .filter(|c| matches!(c.status(), Status::Done))
        .count();

    if total > 0 {
        let pct = (completed * 100) / total;
        let progress_str = if completed == total {
            format!("{}/{} (100%)", completed, total)
                .green()
                .to_string()
        } else {
            format!("{}/{} ({}%)", completed, total, pct)
        };
        println!("{:12} {}", "Progress:".dimmed(), progress_str);
    } else {
        println!("{:12} {}", "Changes:".dimmed(), "none".dimmed());
    }

    // Print description if present
    if !release.description().is_empty() {
        println!();
        println!("{}", release.description());
    }

    // Print changes by status
    if !changes.is_empty() {
        println!();
        println!("{}", "Changes:".bold());
        println!("{}", "-".repeat(60).dimmed());

        // Group by status
        let mut by_status: std::collections::HashMap<Status, Vec<_>> =
            std::collections::HashMap::new();
        for change in &changes {
            by_status
                .entry(change.status().clone())
                .or_default()
                .push(change);
        }

        // Display order: Review, InProgress, Blocked, Approved, Draft, Done
        let status_order = [
            Status::Review,
            Status::InProgress,
            Status::Blocked,
            Status::Approved,
            Status::Draft,
            Status::Paused,
            Status::Done,
            Status::NotPlanned,
        ];

        for status in &status_order {
            if let Some(status_changes) = by_status.get(status) {
                for change in status_changes {
                    let change_status = change.status();
                    let status_indicator = match change_status {
                        Status::Done => "✓".green().to_string(),
                        Status::Review => "⏳".to_string(),
                        Status::InProgress => "→".yellow().to_string(),
                        Status::Blocked => "✗".red().to_string(),
                        Status::Paused => "⏸".cyan().to_string(),
                        _ => "○".dimmed().to_string(),
                    };

                    let status_str = format!("{}", change_status);
                    let status_colored = match change_status {
                        Status::Draft => status_str.dimmed(),
                        Status::Approved => status_str.green(),
                        Status::InProgress => status_str.yellow(),
                        Status::Blocked => status_str.red(),
                        Status::Paused => status_str.cyan(),
                        Status::Review => status_str.magenta(),
                        Status::Done => status_str.blue(),
                        Status::NotPlanned => status_str.red(),
                    };

                    println!(
                        "  {} {} [{}] {}",
                        status_indicator,
                        change.id().cyan(),
                        status_colored,
                        change.title()
                    );
                }
            }
        }
    }

    Ok(())
}
