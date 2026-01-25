use anyhow::Result;
use colored::Colorize;

use crate::change::{Change, Priority, Status};
use crate::event::Event;
use crate::store::Store;

/// Create a new epic bug
pub fn create(title: &str, priority: &str) -> Result<()> {
    let store = Store::open()?;

    let priority: Priority = priority.parse().unwrap_or_else(|_| {
        eprintln!(
            "{} Invalid priority '{}', using 'medium'",
            "!".yellow(),
            priority
        );
        Priority::Medium
    });

    let id = store.generate_id()?;

    let body = r#"## Goal

<!-- One-sentence description of what this epic delivers -->

## Steps

<!-- Steps will be created with: docket new "Step description" --epic ID -->

## Context

<!-- Background information, constraints, relevant details -->

## Log

<!-- Notes added during implementation -->"#
        .to_string();

    let event = Event::epic_created(id.clone(), title.to_string(), priority, body);
    store.append_event(&event)?;

    println!("{} Created epic {} - {}", "✓".green(), id.cyan(), title);
    println!(
        "  Add steps with: {} --epic {}",
        "docket new \"Step description\"".dimmed(),
        id.dimmed()
    );

    Ok(())
}

/// Show epic detail with step progress
pub fn show(store: &Store, epic: &Change) -> Result<()> {
    let children = get_children(store, epic.id())?;

    let completed = children
        .iter()
        .filter(|b| matches!(b.status(), Status::Done))
        .count();
    let total = children.len();

    // Header
    println!(
        "Epic {}: {} ({}/{} complete)",
        epic.id().cyan(),
        epic.title(),
        completed,
        total
    );
    println!();

    if children.is_empty() {
        println!("  {}", "No steps yet".dimmed());
        println!(
            "  Add steps with: {} --epic {}",
            "docket new \"Step description\"".dimmed(),
            epic.id().dimmed()
        );
    } else {
        // Find current step (first non-done step)
        let current_idx = children
            .iter()
            .position(|b| !matches!(b.status(), Status::Done));

        for (idx, child) in children.iter().enumerate() {
            let is_current = Some(idx) == current_idx;
            let status = child.status();

            let marker = if matches!(status, Status::Done) {
                "✓".green().to_string()
            } else if is_current {
                "→".blue().to_string()
            } else {
                " ".to_string()
            };

            let status_str = format!("{:12}", status.to_string());
            let status_colored = match status {
                Status::Done => status_str.green(),
                Status::InProgress => status_str.yellow(),
                Status::Approved => status_str.blue(),
                _ => status_str.dimmed(),
            };

            let current_indicator = if is_current { " ← current" } else { "" };

            println!(
                "  {} {}  {}  {}{}",
                marker,
                child.id().cyan(),
                status_colored,
                child.title(),
                current_indicator.dimmed()
            );
        }
    }

    Ok(())
}

/// Get all children of a change, sorted by their step number.
/// Uses the unified parent() method which supports both parent and parent_epic fields.
pub fn get_children(store: &Store, parent_id: &str) -> Result<Vec<Change>> {
    let all_bugs = store.list_changes()?;

    let mut children: Vec<Change> = all_bugs
        .into_iter()
        .filter(|b| b.parent() == Some(parent_id))
        .collect();

    // Sort by step number (extract N from "epic_id.N")
    children.sort_by(|a, b| {
        let a_num = extract_step_number(a.id());
        let b_num = extract_step_number(b.id());
        a_num.cmp(&b_num)
    });

    Ok(children)
}

/// Extract step number from a child ID (e.g., "abc1.3" -> 3)
fn extract_step_number(id: &str) -> u32 {
    id.rsplit('.')
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

/// Calculate progress for an epic (completed, total)
pub fn epic_progress(store: &Store, epic_id: &str) -> Result<(usize, usize)> {
    let children = get_children(store, epic_id)?;
    let completed = children
        .iter()
        .filter(|b| matches!(b.status(), Status::Done))
        .count();
    Ok((completed, children.len()))
}

/// Get the next incomplete step of an epic
pub fn next_step(store: &Store, epic_id: &str) -> Result<Option<Change>> {
    let children = get_children(store, epic_id)?;

    Ok(children
        .into_iter()
        .find(|b| !matches!(b.status(), Status::Done)))
}

/// Check if all children of an epic are done
pub fn all_children_done(store: &Store, epic_id: &str) -> Result<bool> {
    let children = get_children(store, epic_id)?;

    if children.is_empty() {
        return Ok(false); // No children means not complete
    }

    Ok(children.iter().all(|b| matches!(b.status(), Status::Done)))
}

/// Derive the effective status of an epic from its children
pub fn derive_epic_status(store: &Store, epic: &Change) -> Result<Status> {
    let children = get_children(store, epic.id())?;

    if children.is_empty() {
        // No children, use the epic's own status
        return Ok(epic.status().clone());
    }

    // Check if all children are done
    let all_done = children.iter().all(|b| matches!(b.status(), Status::Done));
    if all_done {
        return Ok(Status::Done);
    }

    // Check if any child is in progress
    let any_in_progress = children
        .iter()
        .any(|b| matches!(b.status(), Status::InProgress));
    if any_in_progress {
        return Ok(Status::InProgress);
    }

    // Otherwise use epic's own status (Draft or Approved)
    Ok(epic.status().clone())
}
