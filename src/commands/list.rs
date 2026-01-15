use anyhow::{Context, Result};
use colored::Colorize;
use std::cmp::Ordering;

use crate::bug::{Bug, Priority, SortBy, Status};
use crate::store::Store;

pub fn list(
    status_filter: Option<&str>,
    priority_filter: Option<&str>,
    show_all: bool,
    sort_by: &str,
    reverse: bool,
) -> Result<()> {
    // Parse sort field early to catch invalid input
    let sort_by: SortBy = sort_by.parse().context("invalid sort field")?;

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
    let mut filtered: Vec<_> = bugs
        .into_iter()
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

    // Sort bugs
    filtered.sort_by(|a, b| {
        let ordering = compare_bugs(a, b, sort_by);
        if reverse {
            ordering.reverse()
        } else {
            ordering
        }
    });

    if filtered.is_empty() {
        println!("{}", "No bugs match the filters.".dimmed());
        return Ok(());
    }

    // Print header
    println!(
        "{:6} {:12} {:8} {:10} {}",
        "ID".bold(),
        "STATUS".bold(),
        "PRIORITY".bold(),
        "WORKSPACE".bold(),
        "TITLE".bold()
    );
    println!("{}", "-".repeat(72).dimmed());

    // Print bugs
    for bug in &filtered {
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

        let workspace_str = store
            .workspace_name(bug.id())
            .unwrap_or_else(|| "-".to_string());

        println!(
            "{:6} {:12} {:8} {:10} {}",
            bug.id().cyan(),
            status_colored,
            priority_colored,
            workspace_str,
            bug.title()
        );
    }

    Ok(())
}

/// Compare two bugs for sorting.
/// Secondary sort is always by created date (oldest first) within the same primary field.
fn compare_bugs(a: &Bug, b: &Bug, sort_by: SortBy) -> Ordering {
    let primary = match sort_by {
        SortBy::Priority => a.priority().cmp(b.priority()),
        SortBy::Created => a.created().cmp(&b.created()),
        SortBy::Status => a.status().cmp(b.status()),
    };

    // Secondary sort: oldest first (FIFO within same primary)
    if primary == Ordering::Equal {
        a.created().cmp(&b.created())
    } else {
        primary
    }
}
