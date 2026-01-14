use anyhow::Result;
use colored::Colorize;

use crate::bug::{Priority, Status};
use crate::store::Store;

pub fn list(
    status_filter: Option<&str>,
    priority_filter: Option<&str>,
    show_all: bool,
) -> Result<()> {
    let store = Store::open()?;
    let bugs = store.list_bugs()?;

    if bugs.is_empty() {
        println!("{}", "No bugs found.".dimmed());
        return Ok(());
    }

    // Parse filters
    let status_filter: Option<Status> = status_filter.and_then(|s| s.parse().ok());
    let priority_filter: Option<Priority> = priority_filter.and_then(|p| p.parse().ok());

    // Filter bugs
    let filtered: Vec<_> = bugs
        .iter()
        .filter(|bug| {
            // By default, hide terminal states (done, not-planned) unless --all is specified
            if !show_all && matches!(bug.status(), Status::Done | Status::NotPlanned) {
                return false;
            }

            // Apply status filter
            if let Some(ref filter) = status_filter {
                if std::mem::discriminant(bug.status()) != std::mem::discriminant(filter) {
                    return false;
                }
            }

            // Apply priority filter
            if let Some(ref filter) = priority_filter {
                if std::mem::discriminant(bug.priority()) != std::mem::discriminant(filter) {
                    return false;
                }
            }

            true
        })
        .collect();

    if filtered.is_empty() {
        println!("{}", "No bugs match the filters.".dimmed());
        return Ok(());
    }

    // Print header
    println!(
        "{:6} {:12} {:8} {}",
        "ID".bold(),
        "STATUS".bold(),
        "PRIORITY".bold(),
        "TITLE".bold()
    );
    println!("{}", "-".repeat(60).dimmed());

    // Print bugs
    for bug in filtered {
        let status_str = format!("{}", bug.status());
        let status_colored = match bug.status() {
            Status::Draft => status_str.dimmed(),
            Status::Approved => status_str.green(),
            Status::InProgress => status_str.yellow(),
            Status::Done => status_str.blue(),
            Status::NotPlanned => status_str.red(),
        };

        let priority_str = format!("{}", bug.priority());
        let priority_colored = match bug.priority() {
            Priority::Low => priority_str.dimmed(),
            Priority::Medium => priority_str.normal(),
            Priority::High => priority_str.red(),
        };

        println!(
            "{:6} {:12} {:8} {}",
            bug.id().cyan(),
            status_colored,
            priority_colored,
            bug.title()
        );
    }

    Ok(())
}
