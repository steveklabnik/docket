use anyhow::Result;
use chrono::Utc;
use colored::Colorize;
use std::cmp::Ordering;

use crate::bug::{Bug, Priority, Status};
use crate::commands::epic;
use crate::store::Store;

/// Show the highest priority approved bugs ready for work.
///
/// Filters to only Approved status bugs, then sorts by:
/// 1. Priority (High > Medium > Low)
/// 2. Created date (oldest first within same priority)
///
/// For epics, shows the next incomplete step if it's approved.
pub fn ready(count: usize, work: bool) -> Result<()> {
    let store = Store::open()?;
    let bugs = store.list_bugs()?;

    // Build list of ready items, handling epics specially
    let mut approved: Vec<Bug> = Vec::new();

    for bug in bugs {
        if bug.is_epic() {
            // For epics, check if the next step is approved
            if let Ok(Some(next)) = epic::next_step(&store, bug.id()) {
                if matches!(next.status(), Status::Approved) {
                    // Include the child step (not the epic itself)
                    approved.push(next);
                }
            }
        } else if !bug.is_child() {
            // Regular bugs (not epic children) - include if approved
            if matches!(bug.status(), Status::Approved) {
                approved.push(bug);
            }
        }
        // Note: epic children are handled via their parent epic above,
        // so we don't add them directly here to avoid duplicates
    }

    if approved.is_empty() {
        println!("{}", "No approved bugs ready for work.".dimmed());
        println!(
            "{}",
            "Run 'docket list --status draft' to see bugs awaiting approval.".dimmed()
        );
        return Ok(());
    }

    // Sort by priority (high first), then by created date (oldest first)
    approved.sort_by(|a, b| {
        let priority_cmp = a.priority().cmp(b.priority());
        if priority_cmp == Ordering::Equal {
            // Oldest first within same priority
            a.created().cmp(&b.created())
        } else {
            priority_cmp
        }
    });

    // If --work flag, start work on the top bug
    if work {
        let top_bug = &approved[0];
        println!(
            "{} Starting work on top ready bug: {} - {}",
            "→".blue(),
            top_bug.id().cyan(),
            top_bug.title()
        );
        // Import and call the work command
        return crate::commands::work::work(top_bug.id(), false, false);
    }

    // Limit to requested count
    let to_show: Vec<_> = approved.into_iter().take(count).collect();

    if count == 1 {
        // Single bug output format
        let bug = &to_show[0];
        let age = format_age(bug);

        // Show epic context for child bugs
        let epic_context = if let Some(parent_id) = bug.parent_epic() {
            if let Ok(parent) = store.get_bug(parent_id) {
                let (completed, total) = epic::epic_progress(&store, parent_id).unwrap_or((0, 0));
                format!(" (step of {} [{}/{}])", parent.title(), completed, total)
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        println!(
            "{} {} ({}) - {}{}",
            "Ready to work:".green(),
            bug.id().cyan(),
            format_priority(bug.priority()),
            bug.title(),
            epic_context.dimmed()
        );
        println!("  Created {}", age.dimmed());
    } else {
        // Multi-bug list format
        for (i, bug) in to_show.iter().enumerate() {
            let age = format_age_short(bug);

            // Show epic context for child bugs
            let epic_info = if let Some(parent_id) = bug.parent_epic() {
                format!(" [{}]", parent_id)
            } else {
                String::new()
            };

            println!(
                "{}. {} ({}) - {} ({}){}",
                (i + 1).to_string().bold(),
                bug.id().cyan(),
                format_priority(bug.priority()),
                bug.title(),
                age.dimmed(),
                epic_info.dimmed()
            );
        }
    }

    Ok(())
}

/// Format the age of a bug as a human-readable string (e.g., "3 days ago")
fn format_age(bug: &Bug) -> String {
    let now = Utc::now();
    let created = bug.created();
    let duration = now.signed_duration_since(created);

    let days = duration.num_days();
    if days == 0 {
        let hours = duration.num_hours();
        if hours == 0 {
            "just now".to_string()
        } else if hours == 1 {
            "1 hour ago".to_string()
        } else {
            format!("{} hours ago", hours)
        }
    } else if days == 1 {
        "1 day ago".to_string()
    } else {
        format!("{} days ago", days)
    }
}

/// Format the age of a bug as a short string (e.g., "3 days")
fn format_age_short(bug: &Bug) -> String {
    let now = Utc::now();
    let created = bug.created();
    let duration = now.signed_duration_since(created);

    let days = duration.num_days();
    if days == 0 {
        let hours = duration.num_hours();
        if hours == 0 {
            "<1h".to_string()
        } else if hours == 1 {
            "1h".to_string()
        } else {
            format!("{}h", hours)
        }
    } else if days == 1 {
        "1 day".to_string()
    } else {
        format!("{} days", days)
    }
}

/// Format priority with color
fn format_priority(priority: &Priority) -> colored::ColoredString {
    let s = format!("{}", priority);
    match priority {
        Priority::High => s.red(),
        Priority::Medium => s.normal(),
        Priority::Low => s.dimmed(),
    }
}
