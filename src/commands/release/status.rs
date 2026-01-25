use anyhow::{anyhow, Result};
use chrono::Utc;
use colored::Colorize;

use crate::release::{ReleaseEvent, ReleaseStatus, UNSCHEDULED_RELEASE};
use crate::store::Store;

/// Helper to check if a transition is valid and perform it
fn transition(
    store: &Store,
    version: &str,
    expected_from: &[ReleaseStatus],
    to: ReleaseStatus,
    action_verb: &str,
) -> Result<()> {
    // Prevent operations on unscheduled release
    if version == UNSCHEDULED_RELEASE {
        return Err(anyhow!(
            "cannot {} the '{}' release - it is a special backlog release",
            action_verb,
            UNSCHEDULED_RELEASE
        ));
    }

    let release = store.get_release(version)?;
    let current = release.status();

    // Check if transition is valid
    if !expected_from.contains(current) {
        let expected_str = expected_from
            .iter()
            .map(|s| format!("{}", s))
            .collect::<Vec<_>>()
            .join(" or ");
        return Err(anyhow!(
            "cannot {} release '{}': current status is '{}', expected {}\n\
             Run 'docket release show {}' to see current status.",
            action_verb,
            version,
            current,
            expected_str,
            version
        ));
    }

    // Create status change event
    let event = ReleaseEvent::status_changed(version.to_string(), current.clone(), to.clone());
    store.append_release_event(&event)?;

    let status_str = format!("{}", to);
    let status_colored = match to {
        ReleaseStatus::Planning => status_str.dimmed(),
        ReleaseStatus::Active => status_str.green(),
        ReleaseStatus::Frozen => status_str.cyan().bold(),
        ReleaseStatus::Released => status_str.blue(),
        ReleaseStatus::Cancelled => status_str.red(),
    };

    println!(
        "{} Release {} is now {}",
        "✓".green(),
        version.cyan().bold(),
        status_colored
    );

    Ok(())
}

/// Transition Planning -> Active
pub fn activate(version: &str) -> Result<()> {
    let store = Store::open()?;
    transition(
        &store,
        version,
        &[ReleaseStatus::Planning],
        ReleaseStatus::Active,
        "activate",
    )
}

/// Transition Active -> Frozen
pub fn freeze(version: &str) -> Result<()> {
    let store = Store::open()?;
    transition(
        &store,
        version,
        &[ReleaseStatus::Active],
        ReleaseStatus::Frozen,
        "freeze",
    )
}

/// Transition Active/Frozen -> Released
pub fn ship(version: &str) -> Result<()> {
    let store = Store::open()?;

    // Prevent operations on unscheduled release
    if version == UNSCHEDULED_RELEASE {
        return Err(anyhow!(
            "cannot ship the '{}' release - it is a special backlog release",
            UNSCHEDULED_RELEASE
        ));
    }

    let release = store.get_release(version)?;
    let current = release.status();

    // Check if transition is valid
    let valid_from = [ReleaseStatus::Active, ReleaseStatus::Frozen];
    if !valid_from.contains(current) {
        return Err(anyhow!(
            "cannot ship release '{}': current status is '{}', expected active or frozen\n\
             Run 'docket release show {}' to see current status.",
            version,
            current,
            version
        ));
    }

    // Create status change event
    let status_event = ReleaseEvent::status_changed(
        version.to_string(),
        current.clone(),
        ReleaseStatus::Released,
    );
    store.append_release_event(&status_event)?;

    // Record the release date
    let released_event = ReleaseEvent::released_at(version.to_string(), Utc::now());
    store.append_release_event(&released_event)?;

    println!(
        "{} Release {} has been shipped!",
        "✓".green(),
        version.cyan().bold()
    );

    // Show summary
    let (completed, total) = store.release_progress(version)?;
    if total > 0 {
        println!("  {} changes included ({} completed)", total, completed);
    }

    Ok(())
}

/// Transition any -> Cancelled
pub fn cancel(version: &str) -> Result<()> {
    let store = Store::open()?;

    // Prevent operations on unscheduled release
    if version == UNSCHEDULED_RELEASE {
        return Err(anyhow!(
            "cannot cancel the '{}' release - it is a special backlog release",
            UNSCHEDULED_RELEASE
        ));
    }

    let release = store.get_release(version)?;
    let current = release.status();

    // Can't cancel if already released or cancelled
    if matches!(current, ReleaseStatus::Released) {
        return Err(anyhow!(
            "cannot cancel release '{}': it has already been released",
            version
        ));
    }

    if matches!(current, ReleaseStatus::Cancelled) {
        println!(
            "{} Release {} is already cancelled",
            "→".blue(),
            version.cyan()
        );
        return Ok(());
    }

    // Create status change event
    let event = ReleaseEvent::status_changed(
        version.to_string(),
        current.clone(),
        ReleaseStatus::Cancelled,
    );
    store.append_release_event(&event)?;

    println!(
        "{} Release {} has been cancelled",
        "✓".green(),
        version.cyan().bold()
    );

    // Warn about changes that were scheduled for this release
    let (_, total) = store.release_progress(version)?;
    if total > 0 {
        println!(
            "  {} {} changes were scheduled for this release",
            "!".yellow(),
            total
        );
        println!("  Use 'docket release schedule <id> <new-version>' to reschedule them");
    }

    Ok(())
}
