use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use std::fs;
use std::io::{self, Read};

use crate::bug::{Priority, Status};
use crate::event::Event;
use crate::store::Store;

/// Read body content from a file path or stdin (if path is "-")
fn read_body_from_source(source: &str) -> Result<String> {
    if source == "-" {
        let mut content = String::new();
        io::stdin()
            .read_to_string(&mut content)
            .context("failed to read body from stdin")?;
        Ok(content)
    } else {
        fs::read_to_string(source).with_context(|| format!("failed to read body from '{}'", source))
    }
}

/// Validate status transition and return error message if invalid
fn validate_status_transition(from: &Status, to: &Status) -> Result<()> {
    // NotPlanned is a terminal state - can't transition from it
    if matches!(from, Status::NotPlanned) {
        return Err(anyhow!(
            "cannot change status from not-planned (terminal state)"
        ));
    }

    // Done is a terminal state - can't transition from it
    if matches!(from, Status::Done) {
        return Err(anyhow!("cannot change status from done (terminal state)"));
    }

    // Can't transition to Done via update --status (use 'done' command instead)
    if matches!(to, Status::Done) {
        return Err(anyhow!("use 'docket done' command to mark bugs as done"));
    }

    // Valid transitions to NotPlanned: Draft, Approved, InProgress
    // (already handled by the from checks above)

    // All other transitions are allowed
    Ok(())
}

pub fn update(
    id: &str,
    title: Option<String>,
    body_source: Option<&str>,
    priority_str: Option<&str>,
    status_str: Option<&str>,
) -> Result<()> {
    let store = Store::open()?;
    let bug = store.get_bug(id)?;
    let bug_id = bug.metadata.id.clone();

    // Parse priority if provided
    let priority: Option<Priority> = match priority_str {
        Some(p) => Some(
            p.parse()
                .map_err(|_| anyhow!("invalid priority '{}', must be low, medium, or high", p))?,
        ),
        None => None,
    };

    // Parse status if provided
    let status: Option<Status> = match status_str {
        Some(s) => Some(s.parse().map_err(|_| {
            anyhow!(
                "invalid status '{}', must be draft, approved, in-progress, done, or not-planned",
                s
            )
        })?),
        None => None,
    };

    // Read body if provided
    let body: Option<String> = match body_source {
        Some(source) => Some(read_body_from_source(source)?),
        None => None,
    };

    // Check if there's anything to update
    if title.is_none() && body.is_none() && priority.is_none() && status.is_none() {
        println!(
            "{} Nothing to update. Provide --title, --body, --priority, or --status.",
            "!".yellow()
        );
        return Ok(());
    }

    // Validate and apply status change if requested
    if let Some(ref new_status) = status {
        // Check if status actually changed
        if std::mem::discriminant(new_status) != std::mem::discriminant(&bug.metadata.status) {
            validate_status_transition(&bug.metadata.status, new_status)?;
            let event = Event::status_changed(
                bug_id.clone(),
                bug.metadata.status.clone(),
                new_status.clone(),
            );
            store.append_event(&event)?;
        }
    }

    // Emit Updated event for title/body changes
    if title.is_some() || body.is_some() {
        let event = Event::updated(bug_id.clone(), title.clone(), body);
        store.append_event(&event)?;
    }

    // Priority changes
    if let Some(new_priority) = priority {
        // Check if priority actually changed
        if new_priority != bug.metadata.priority {
            let event =
                Event::priority_changed(bug_id.clone(), bug.metadata.priority, new_priority);
            store.append_event(&event)?;
        }
    }

    // Build summary of changes
    let mut changes = Vec::new();
    if title.is_some() {
        changes.push("title");
    }
    if body_source.is_some() {
        changes.push("body");
    }
    if priority_str.is_some() {
        changes.push("priority");
    }
    if status_str.is_some() {
        changes.push("status");
    }

    println!(
        "{} Updated bug {} ({})",
        "✓".green(),
        bug_id.cyan(),
        changes.join(", ")
    );

    Ok(())
}
