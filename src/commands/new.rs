use anyhow::{Context, Result};
use colored::Colorize;
use dialoguer::{Input, Select};
use std::fs;
use std::io::{self, Read};

use crate::bug::{ChangelogType, Priority};
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

pub fn new(
    title: Option<String>,
    priority_str: &str,
    body_source: Option<&str>,
    interactive: bool,
    changelog_type_str: Option<&str>,
    version: Option<&str>,
) -> Result<()> {
    let store = Store::open()?;

    // Get title interactively if not provided
    let title = match title {
        Some(t) => t,
        None => Input::new().with_prompt("Bug title").interact_text()?,
    };

    // Parse priority - prompt interactively only if in interactive mode and using default
    let priority: Priority = if interactive && priority_str == "medium" {
        let options = vec!["low", "medium", "high"];
        let selection = Select::new()
            .with_prompt("Priority")
            .items(&options)
            .default(1)
            .interact()?;
        options[selection].parse().unwrap_or(Priority::Medium)
    } else {
        priority_str.parse().unwrap_or_else(|_| {
            eprintln!(
                "{} Invalid priority '{}', using 'medium'",
                "!".yellow(),
                priority_str
            );
            Priority::Medium
        })
    };

    // Generate unique ID
    let id = store.generate_id()?;

    // Get body from source or use default template
    let body = match body_source {
        Some(source) => read_body_from_source(source)?,
        None => r#"## Goal

<!-- One-sentence description of success -->

## Acceptance Criteria

- [ ] First criterion

## Context

<!-- Background information, constraints, relevant details -->

## Log

<!-- Notes added during implementation -->"#
            .to_string(),
    };

    // Emit Created event
    let event = Event::created(id.clone(), title.clone(), priority, body);
    store.append_event(&event)?;

    // Emit ChangelogTypeSet event if changelog type was provided
    if let Some(ct_str) = changelog_type_str {
        let changelog_type: ChangelogType = ct_str.parse().with_context(|| {
            format!(
                "invalid changelog type '{}'. Valid options: feature, fix, change, deprecated, removed, security, internal",
                ct_str
            )
        })?;
        let event = Event::changelog_type_set(id.clone(), changelog_type);
        store.append_event(&event)?;
    }

    // Emit VersionAdded event if version was provided
    if let Some(ver) = version {
        let event = Event::version_added(id.clone(), ver.to_string());
        store.append_event(&event)?;
    }

    println!("{} Created bug {} - {}", "✓".green(), id.cyan(), title);
    println!("  Edit with: {} {}", "docket show".dimmed(), id.dimmed());

    Ok(())
}
