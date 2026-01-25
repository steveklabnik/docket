use anyhow::{anyhow, Context, Result};
use chrono::{NaiveDate, Utc};
use colored::Colorize;
use std::fs;
use std::io::{self, Read};

use crate::release::{validate_version, ReleaseEvent, UNSCHEDULED_RELEASE};
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

/// Parse a date string in YYYY-MM-DD format
fn parse_date(date_str: &str) -> Result<chrono::DateTime<Utc>> {
    let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        .with_context(|| format!("invalid date format '{}', expected YYYY-MM-DD", date_str))?;
    Ok(date.and_hms_opt(0, 0, 0).unwrap().and_utc())
}

pub fn new(
    version: &str,
    title: Option<&str>,
    body_source: Option<&str>,
    target_date: Option<&str>,
    edit: bool,
) -> Result<()> {
    // Validate version format
    validate_version(version)?;

    // Don't allow creating "unscheduled" manually
    if version == UNSCHEDULED_RELEASE {
        return Err(anyhow!(
            "cannot create '{}' release - it is automatically created by docket",
            UNSCHEDULED_RELEASE
        ));
    }

    let store = Store::open()?;

    // Check if release already exists
    if store.release_exists(version) {
        return Err(anyhow!(
            "release '{}' already exists\n\
             Use 'docket release show {}' to view it.",
            version,
            version
        ));
    }

    // Get description from source if provided
    let mut description = if let Some(source) = body_source {
        read_body_from_source(source)?
    } else {
        String::new()
    };

    // Open editor if requested
    if edit {
        description = dialoguer::Editor::new()
            .edit(&description)
            .context("failed to open editor")?
            .unwrap_or(description);
    }

    // Parse target date if provided
    let target_date_parsed = target_date.map(parse_date).transpose()?;

    // Create the release
    let event = ReleaseEvent::created(
        version.to_string(),
        title.map(|s| s.to_string()),
        description,
        target_date_parsed,
    );

    store.append_release_event(&event)?;

    println!("{} Created release {}", "✓".green(), version.cyan().bold());

    if let Some(t) = title {
        println!("  Title: {}", t);
    }

    if let Some(date) = target_date {
        println!("  Target: {}", date);
    }

    println!(
        "  View with: {} {}",
        "docket release show".dimmed(),
        version.dimmed()
    );

    Ok(())
}
