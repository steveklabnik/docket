use anyhow::{anyhow, Result};
use colored::Colorize;

use crate::event::Event;
use crate::release::{validate_version, ReleaseStatus, UNSCHEDULED_RELEASE};
use crate::store::Store;

pub fn schedule(change_id: &str, version: &str) -> Result<()> {
    // Validate version format
    validate_version(version)?;

    let store = Store::open()?;

    // Resolve change ID
    let full_id = store.resolve_id(change_id)?;
    let change = store.get_change(&full_id)?;

    // Check if release exists
    let release = store.get_release(version)?;

    // Don't allow scheduling to Released/Cancelled releases
    match release.status() {
        ReleaseStatus::Released => {
            return Err(anyhow!(
                "cannot schedule change to released version '{}'\n\
                 Create a new release version instead.",
                version
            ));
        }
        ReleaseStatus::Cancelled => {
            return Err(anyhow!(
                "cannot schedule change to cancelled release '{}'",
                version
            ));
        }
        ReleaseStatus::Frozen => {
            // Allow but warn
            println!(
                "{} Release '{}' is frozen - change scope may require approval",
                "!".yellow(),
                version
            );
        }
        _ => {}
    }

    // Check if already at this release
    if change.target_release() == version {
        println!(
            "{} Change {} is already scheduled for {}",
            "→".blue(),
            full_id.cyan(),
            version.cyan()
        );
        return Ok(());
    }

    let old_release = change.target_release().to_string();

    // Create the event
    let event = Event::release_set(full_id.clone(), version.to_string());
    store.append_event(&event)?;

    if old_release == UNSCHEDULED_RELEASE {
        println!(
            "{} Scheduled change {} for release {}",
            "✓".green(),
            full_id.cyan(),
            version.cyan().bold()
        );
    } else {
        println!(
            "{} Moved change {} from {} to {}",
            "✓".green(),
            full_id.cyan(),
            old_release.dimmed(),
            version.cyan().bold()
        );
    }

    println!("  {}", change.title());

    Ok(())
}
