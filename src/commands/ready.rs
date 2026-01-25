use anyhow::Result;
use chrono::Utc;
use colored::Colorize;
use std::cmp::Ordering;

use crate::change::{Change, Priority, Status};
use crate::commands::epic;
use crate::release::UNSCHEDULED_RELEASE;
use crate::store::Store;

/// Show the highest priority approved changes ready for work.
///
/// Filters to only Approved status changes that have no unresolved dependencies,
/// then sorts by:
/// 1. Priority (High > Medium > Low)
/// 2. Created date (oldest first within same priority)
///
/// For parent changes, shows the next incomplete child if it's approved and unblocked.
/// If `release_filter` is provided, only shows changes targeting that release.
pub fn ready(count: usize, work: bool, release_filter: Option<&str>) -> Result<()> {
    let store = Store::open()?;
    let bugs = store.list_changes()?;

    // Build list of ready items, handling parent changes specially
    let mut approved: Vec<Change> = Vec::new();

    for bug in &bugs {
        if bug.is_epic() {
            // For parent changes, check if the next child is approved and unblocked
            if let Ok(Some(next)) = epic::next_step(&store, bug.id()) {
                if matches!(next.status(), Status::Approved)
                    && !has_unresolved_dependencies(&next, &bugs)
                {
                    // Include the child (not the parent itself)
                    approved.push(next);
                }
            }
        } else if !bug.is_child() {
            // Leaf changes (not children) - include if approved and unblocked
            if matches!(bug.status(), Status::Approved) && !has_unresolved_dependencies(bug, &bugs)
            {
                approved.push(bug.clone());
            }
        }
        // Note: children are handled via their parent above,
        // so we don't add them directly here to avoid duplicates
    }

    // Apply release filter if provided
    if let Some(release) = release_filter {
        approved.retain(|bug| bug.target_release() == release);
    }

    if approved.is_empty() {
        if let Some(release) = release_filter {
            println!(
                "{}",
                format!("No approved changes ready for release {}.", release).dimmed()
            );
        } else {
            println!("{}", "No approved changes ready for work.".dimmed());
        }
        println!(
            "{}",
            "Run 'docket list --status draft' to see changes awaiting approval.".dimmed()
        );
        return Ok(());
    }

    // Sort by priority (high first), then by created date (oldest first)
    // For unfiltered view, scheduled changes come before unscheduled
    approved.sort_by(|a, b| {
        // If no release filter, scheduled changes first
        if release_filter.is_none() {
            let a_scheduled = a.target_release() != UNSCHEDULED_RELEASE;
            let b_scheduled = b.target_release() != UNSCHEDULED_RELEASE;
            if a_scheduled && !b_scheduled {
                return Ordering::Less;
            }
            if !a_scheduled && b_scheduled {
                return Ordering::Greater;
            }
        }

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

        // Show parent context for child changes
        let epic_context = if let Some(parent_id) = bug.parent_epic() {
            if let Ok(parent) = store.get_change(parent_id) {
                let (completed, total) = epic::epic_progress(&store, parent_id).unwrap_or((0, 0));
                format!(" (step of {} [{}/{}])", parent.title(), completed, total)
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        // Show release info
        let release_info = format_release_info(bug.target_release());

        println!(
            "{} {} ({}) - {}{}",
            "Ready to work:".green(),
            bug.id().cyan(),
            format_priority(bug.priority()),
            bug.title(),
            epic_context.dimmed()
        );
        println!("  Created {}  {}", age.dimmed(), release_info);
    } else {
        // Multi-bug list format
        for (i, bug) in to_show.iter().enumerate() {
            let age = format_age_short(bug);

            // Show parent context for child changes
            let epic_info = if let Some(parent_id) = bug.parent_epic() {
                format!(" [{}]", parent_id)
            } else {
                String::new()
            };

            // Show release info
            let release_info = format_release_info_short(bug.target_release());

            println!(
                "{}. {} ({}) - {} ({}){}{}",
                (i + 1).to_string().bold(),
                bug.id().cyan(),
                format_priority(bug.priority()),
                bug.title(),
                age.dimmed(),
                release_info,
                epic_info.dimmed()
            );
        }
    }

    Ok(())
}

/// Format release info for single-item display
fn format_release_info(release: &str) -> colored::ColoredString {
    if release == UNSCHEDULED_RELEASE {
        "unscheduled".dimmed()
    } else {
        format!("release: {}", release).cyan()
    }
}

/// Format release info for list display
fn format_release_info_short(release: &str) -> String {
    if release == UNSCHEDULED_RELEASE {
        String::new()
    } else {
        format!(" [{}]", release.cyan())
    }
}

/// Format the age of a change as a human-readable string (e.g., "3 days ago")
fn format_age(bug: &Change) -> String {
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

/// Format the age of a change as a short string (e.g., "3 days")
fn format_age_short(bug: &Change) -> String {
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

/// Check if a change has unresolved dependencies (dependencies that are not Done)
fn has_unresolved_dependencies(bug: &Change, all_bugs: &[Change]) -> bool {
    if !bug.has_dependencies() {
        return false;
    }

    bug.blocked_by().iter().any(|blocker_id| {
        all_bugs
            .iter()
            .find(|b| b.id() == blocker_id)
            .map(|b| !matches!(b.status(), Status::Done))
            .unwrap_or(true) // If blocker not found, consider it unresolved
    })
}
