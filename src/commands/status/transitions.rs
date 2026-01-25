//! Simple status transition functions.

use anyhow::Result;
use colored::Colorize;

use crate::change::{Change, Status};
use crate::event::Event;
use crate::store::Store;

/// Mark a change as approved for work.
/// When approving a change that has children, all children are auto-approved too.
pub fn approve(id: &str) -> Result<()> {
    let store = Store::open()?;
    let bug = store.get_change(id)?;
    let all_bugs = store.list_changes()?;

    // Check for unresolved dependencies and warn
    if bug.has_dependencies() {
        let unresolved: Vec<_> = bug
            .blocked_by()
            .iter()
            .filter_map(|blocker_id| {
                all_bugs
                    .iter()
                    .find(|b| b.id() == blocker_id)
                    .filter(|b| !matches!(b.status(), Status::Done))
            })
            .collect();

        if !unresolved.is_empty() {
            eprintln!(
                "{} Change {} has {} unresolved dependenc{}:",
                "!".yellow(),
                bug.id().cyan(),
                unresolved.len(),
                if unresolved.len() == 1 { "y" } else { "ies" }
            );
            for blocker in &unresolved {
                eprintln!(
                    "  {} {} - {} ({})",
                    "○".yellow(),
                    blocker.id().cyan(),
                    blocker.title(),
                    blocker.status()
                );
            }
            eprintln!();
        }
    }

    // Approve the main change
    approve_single(&store, &bug)?;

    // Find and approve all children recursively
    let children = find_children(bug.id(), &all_bugs);
    let mut approved_count = 0;
    for child in children {
        if !matches!(
            child.status(),
            Status::Done | Status::Approved | Status::InProgress | Status::Review
        ) {
            approve_single(&store, child)?;
            approved_count += 1;
        }
    }

    if approved_count > 0 {
        println!(
            "{} Auto-approved {} child change{}",
            "✓".green(),
            approved_count,
            if approved_count == 1 { "" } else { "s" }
        );
    }

    Ok(())
}

/// Approve a single change without recursing into children.
fn approve_single(store: &Store, bug: &Change) -> Result<()> {
    let old_status = bug.status().clone();
    let bug_id = bug.id().to_string();

    // Skip if already done
    if matches!(old_status, Status::Done) {
        return Ok(());
    }

    // Emit StatusChanged event
    let event = Event::status_changed(bug_id.clone(), old_status.clone(), Status::Approved);
    store.append_event(&event)?;

    println!(
        "{} Approved change {} ({} -> {})",
        "✓".green(),
        bug_id.cyan(),
        format!("{}", old_status).dimmed(),
        format!("{}", Status::Approved).green()
    );

    Ok(())
}

/// Find all children of a change (recursively).
fn find_children<'a>(parent_id: &str, all_bugs: &'a [Change]) -> Vec<&'a Change> {
    let mut children = Vec::new();
    for bug in all_bugs {
        if bug.parent() == Some(parent_id) {
            children.push(bug);
            // Recursively find grandchildren
            children.extend(find_children(bug.id(), all_bugs));
        }
    }
    children
}
