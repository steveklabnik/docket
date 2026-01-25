use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use std::fs;
use std::io::{self, Read};

use crate::change::{ChangelogType, Priority, Status};
use crate::event::Event;
use crate::release::validate_version;
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

#[allow(clippy::too_many_arguments)]
pub fn update(
    id: &str,
    title: Option<String>,
    body_source: Option<&str>,
    priority_str: Option<&str>,
    status_str: Option<&str>,
    changelog_type_str: Option<&str>,
    version: Option<&str>,
    remove_version: Option<&str>,
    release: Option<&str>,
) -> Result<()> {
    let store = Store::open()?;
    let bug = store.get_change(id)?;
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

    // Parse changelog type if provided
    let changelog_type: Option<ChangelogType> = match changelog_type_str {
        Some(ct) => Some(ct.parse().map_err(|_| {
            anyhow!(
                "invalid changelog type '{}'. Valid options: feature, fix, change, deprecated, removed, security, internal",
                ct
            )
        })?),
        None => None,
    };

    // Validate release if provided (must be valid semver or "unscheduled")
    if let Some(rel) = release {
        validate_version(rel)?;
        // Check if it's the same as current release
        if rel == bug.target_release() {
            println!(
                "{} Change is already scheduled for release {}",
                "!".yellow(),
                rel
            );
        }
    }

    // Check if there's anything to update
    if title.is_none()
        && body.is_none()
        && priority.is_none()
        && status.is_none()
        && changelog_type.is_none()
        && version.is_none()
        && remove_version.is_none()
        && release.is_none()
    {
        println!(
            "{} Nothing to update. Provide --title, --body, --priority, --status, --changelog, --version, --remove-version, or --release.",
            "!".yellow()
        );
        return Ok(());
    }

    // Start a transaction for atomic multi-event writes
    let mut tx = store.begin_transaction(&bug_id)?;
    let mut version_not_found = false;

    // Validate and add status change event if requested
    if let Some(ref new_status) = status {
        // Check if status actually changed
        if std::mem::discriminant(new_status) != std::mem::discriminant(&bug.metadata.status) {
            validate_status_transition(&bug.metadata.status, new_status)?;
            let event = Event::status_changed(
                bug_id.clone(),
                bug.metadata.status.clone(),
                new_status.clone(),
            );
            tx.add_event(event);
        }
    }

    // Add Updated event for title/body changes
    if title.is_some() || body.is_some() {
        let event = Event::updated(bug_id.clone(), title.clone(), body);
        tx.add_event(event);
    }

    // Priority changes
    if let Some(new_priority) = priority {
        // Check if priority actually changed
        if new_priority != bug.metadata.priority {
            let event = Event::priority_changed(
                bug_id.clone(),
                bug.metadata.priority.clone(),
                new_priority,
            );
            tx.add_event(event);
        }
    }

    // Changelog type changes
    if let Some(new_changelog_type) = changelog_type {
        // Check if changelog type actually changed
        let changed = match &bug.metadata.changelog_type {
            Some(existing) => *existing != new_changelog_type,
            None => true,
        };
        if changed {
            let event = Event::changelog_type_set(bug_id.clone(), new_changelog_type);
            tx.add_event(event);
        }
    }

    // Version additions
    if let Some(ver) = version {
        // Check if version already exists
        if !bug.metadata.versions.contains(&ver.to_string()) {
            let event = Event::version_added(bug_id.clone(), ver.to_string());
            tx.add_event(event);
        }
    }

    // Version removals
    if let Some(ver) = remove_version {
        // Check if version exists to remove
        if bug.metadata.versions.contains(&ver.to_string()) {
            let event = Event::version_removed(bug_id.clone(), ver.to_string());
            tx.add_event(event);
        } else {
            version_not_found = true;
        }
    }

    // Release scheduling
    if let Some(rel) = release {
        // Only add event if release actually changed
        if rel != bug.target_release() {
            let event = Event::release_set(bug_id.clone(), rel.to_string());
            tx.add_event(event);
        }
    }

    // Commit all events atomically
    tx.commit()?;

    // Print warning about version not found after commit
    if version_not_found {
        if let Some(ver) = remove_version {
            println!(
                "{} Version '{}' not found on bug {}",
                "!".yellow(),
                ver,
                bug_id
            );
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
    if changelog_type_str.is_some() {
        changes.push("changelog");
    }
    if version.is_some() {
        changes.push("version added");
    }
    if remove_version.is_some() && !version_not_found {
        changes.push("version removed");
    }
    if release.is_some() && release != Some(bug.target_release()) {
        changes.push("release");
    }

    println!(
        "{} Updated bug {} ({})",
        "✓".green(),
        bug_id.cyan(),
        changes.join(", ")
    );

    Ok(())
}
