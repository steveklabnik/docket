use anyhow::Result;
use colored::Colorize;
use std::process::Command;

use crate::bug::Status;
use crate::event::Event;
use crate::store::Store;

pub fn sweep() -> Result<()> {
    let store = Store::open()?;
    let bugs = store.list_bugs()?;

    let mut closed_count = 0;

    for bug in bugs {
        // Skip bugs that are already done or have no linked changes
        if matches!(bug.status(), Status::Done) {
            continue;
        }

        let changes = bug.changes();
        if changes.is_empty() {
            continue;
        }

        // Check if all linked changes are in trunk
        let all_merged = changes
            .iter()
            .all(|change_id| is_merged_to_trunk(change_id));

        if all_merged {
            let old_status = bug.status().clone();
            let bug_id = bug.id().to_string();
            let bug_title = bug.title().to_string();

            // Emit StatusChanged event
            let event = Event::status_changed(bug_id.clone(), old_status.clone(), Status::Done);
            store.append_event(&event)?;

            println!(
                "{} Closed bug {} - {} ({} -> {})",
                "✓".green(),
                bug_id.cyan(),
                bug_title,
                format!("{}", old_status).dimmed(),
                "done".green()
            );
            closed_count += 1;
        }
    }

    if closed_count == 0 {
        println!("{} No bugs to close", "→".blue());
    } else {
        println!(
            "\n{} Closed {} bug{}",
            "✓".green(),
            closed_count,
            if closed_count == 1 { "" } else { "s" }
        );
    }

    Ok(())
}

/// Check if a change ID has been merged to trunk
fn is_merged_to_trunk(change_id: &str) -> bool {
    // Use jj to check if the change is an ancestor of trunk
    // Check if change is reachable from trunk bookmark using ::trunk & change_id
    let output = Command::new("jj")
        .args([
            "log",
            "-r",
            &format!("::trunk & {}", change_id),
            "--no-graph",
            "-T",
            "change_id",
        ])
        .output();

    match output {
        Ok(output) => {
            // If we get any output, the change is in trunk
            !output.stdout.is_empty()
        }
        Err(_) => false,
    }
}
