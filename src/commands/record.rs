//! The `record` command for recording already-completed work.
//!
//! This command creates a change directly in Done status, useful for
//! capturing work that happened before docket tracking was in place.

use anyhow::{Context, Result};
use colored::Colorize;
use dialoguer::Editor;
use std::fs;
use std::io::{self, Read};

use crate::change::{ChangelogType, Priority, Status};
use crate::event::Event;
use crate::release::{validate_version, UNSCHEDULED_RELEASE};
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

/// Build a references section to append to the body
fn build_references_section(pr: Option<&str>, commit: Option<&str>) -> Option<String> {
    if pr.is_none() && commit.is_none() {
        return None;
    }

    let mut refs = Vec::new();
    if let Some(pr_num) = pr {
        refs.push(format!("- PR: #{}", pr_num));
    }
    if let Some(sha) = commit {
        refs.push(format!("- Commit: {}", sha));
    }

    Some(format!("\n---\nReferences:\n{}", refs.join("\n")))
}

#[allow(clippy::too_many_arguments)]
pub fn record(
    title: &str,
    body_source: Option<&str>,
    changelog_type_str: Option<&str>,
    release: Option<&str>,
    parent_id: Option<&str>,
    pr: Option<&str>,
    commit: Option<&str>,
    edit: bool,
) -> Result<()> {
    let store = Store::open()?;

    // Validate release version if provided
    let target_release = release.unwrap_or(UNSCHEDULED_RELEASE);
    validate_version(target_release)?;

    // Check that release exists
    if !store.release_exists(target_release) {
        store.ensure_unscheduled_release()?;
        if !store.release_exists(target_release) {
            return Err(anyhow::anyhow!(
                "release '{}' does not exist\n\
                 Create it with: docket release new {}",
                target_release,
                target_release
            ));
        }
    }

    // If creating a child, verify the parent change exists
    let parent_change = if let Some(parent_ref) = parent_id {
        let parent = store.get_change(parent_ref)?;
        Some(parent)
    } else {
        None
    };

    // Generate ID
    let id = store.generate_id()?;

    // Get body content
    let mut body = if let Some(source) = body_source {
        read_body_from_source(source)?
    } else if edit {
        // Open editor with empty content
        let edited = Editor::new()
            .extension(".md")
            .edit("")
            .context("failed to open editor")?;
        edited.unwrap_or_default()
    } else {
        String::new()
    };

    // Append references section if PR or commit provided
    if let Some(refs_section) = build_references_section(pr, commit) {
        body.push_str(&refs_section);
    }

    // Start a transaction for atomic multi-event writes
    let mut tx = store.begin_transaction(&id)?;

    // Add Created event (with parent if applicable)
    let event = if let Some(ref parent) = parent_change {
        Event::created_with_parent(
            id.clone(),
            title.to_string(),
            Priority::Medium,
            body,
            parent.id().to_string(),
        )
    } else {
        Event::created(id.clone(), title.to_string(), Priority::Medium, body)
    };
    tx.add_event(event);

    // Immediately transition to Done status
    let event = Event::status_changed(id.clone(), Status::Draft, Status::Done);
    tx.add_event(event);

    // Add ChangelogTypeSet event if changelog type was provided
    if let Some(ct_str) = changelog_type_str {
        let changelog_type: ChangelogType = ct_str.parse().with_context(|| {
            format!(
                "invalid changelog type '{}'. Valid options: feature, fix, change, deprecated, removed, security, internal",
                ct_str
            )
        })?;
        let event = Event::changelog_type_set(id.clone(), changelog_type);
        tx.add_event(event);
    }

    // Set the target release (if not unscheduled, add an explicit event)
    if target_release != UNSCHEDULED_RELEASE {
        let event = Event::release_set(id.clone(), target_release.to_string());
        tx.add_event(event);
    }

    // Commit all events atomically
    tx.commit()?;

    // Print success message
    if let Some(parent) = parent_change {
        println!(
            "{} Recorded completed change {} - {} (under {})",
            "✓".green(),
            id.cyan(),
            title,
            parent.id().cyan()
        );
    } else {
        println!(
            "{} Recorded completed change {} - {}",
            "✓".green(),
            id.cyan(),
            title
        );
    }

    println!("  Status: {}", "done".green());

    if target_release != UNSCHEDULED_RELEASE {
        println!("  Release: {}", target_release.cyan());
    }

    if changelog_type_str.is_some() {
        println!("  Changelog: {}", changelog_type_str.unwrap().yellow());
    }

    if pr.is_some() || commit.is_some() {
        println!("  References added to body");
    }

    Ok(())
}
