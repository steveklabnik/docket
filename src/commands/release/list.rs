use anyhow::Result;
use colored::Colorize;

use crate::release::ReleaseStatus;
use crate::store::Store;

pub fn list(show_all: bool) -> Result<()> {
    let store = Store::open()?;
    let releases = store.list_releases()?;

    if releases.is_empty() {
        println!("{}", "No releases found.".dimmed());
        return Ok(());
    }

    // Filter out Released/Cancelled unless --all is specified
    let filtered: Vec<_> = releases
        .into_iter()
        .filter(|r| {
            show_all
                || !matches!(
                    r.status(),
                    ReleaseStatus::Released | ReleaseStatus::Cancelled
                )
        })
        .collect();

    if filtered.is_empty() {
        println!("{}", "No active releases found.".dimmed());
        println!(
            "{}",
            "Use --all to see released and cancelled releases.".dimmed()
        );
        return Ok(());
    }

    // Print header
    println!(
        "{:14} {:12} {:12} {:10} {}",
        "VERSION".bold(),
        "STATUS".bold(),
        "TARGET".bold(),
        "PROGRESS".bold(),
        "TITLE".bold()
    );
    println!("{}", "-".repeat(70).dimmed());

    for release in &filtered {
        let status_str = format!("{}", release.status());
        let status_colored = match release.status() {
            ReleaseStatus::Planning => status_str.dimmed(),
            ReleaseStatus::Active => status_str.green(),
            ReleaseStatus::Frozen => status_str.cyan().bold(),
            ReleaseStatus::Released => status_str.blue(),
            ReleaseStatus::Cancelled => status_str.red(),
        };

        let target_str = release
            .target_date()
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "-".to_string());

        // Get progress
        let progress_str = match store.release_progress(release.version()) {
            Ok((completed, total)) => {
                if total == 0 {
                    "-".to_string()
                } else {
                    let pct = (completed * 100) / total;
                    if completed == total {
                        format!("{}/{}", completed, total).green().to_string()
                    } else {
                        format!("{}/{} ({}%)", completed, total, pct)
                    }
                }
            }
            Err(_) => "?".to_string(),
        };

        let title = release.title().unwrap_or("-");

        // Highlight unscheduled differently
        let version_display = if release.is_unscheduled() {
            release.version().yellow().to_string()
        } else {
            release.version().cyan().to_string()
        };

        println!(
            "{:14} {:12} {:12} {:10} {}",
            version_display, status_colored, target_str, progress_str, title
        );
    }

    Ok(())
}
