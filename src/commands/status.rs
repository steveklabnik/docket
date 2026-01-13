use anyhow::{anyhow, Result};
use colored::Colorize;
use std::path::Path;

use crate::bug::Status;
use crate::event::Event;
use crate::store::Store;

fn set_status(id: &str, new_status: Status, action: &str) -> Result<()> {
    let store = Store::open()?;
    let bug = store.get_bug(id)?;

    let old_status = bug.status().clone();
    let bug_id = bug.id().to_string();

    // Validate transition
    if matches!(old_status, Status::Done) {
        return Err(anyhow!(
            "cannot change status of completed bug '{}'",
            bug_id
        ));
    }

    // Emit StatusChanged event
    let event = Event::status_changed(bug_id.clone(), old_status.clone(), new_status.clone());
    store.append_event(&event)?;

    println!(
        "{} {} bug {} ({} -> {})",
        "✓".green(),
        action,
        bug_id.cyan(),
        format!("{}", old_status).dimmed(),
        format!("{}", new_status).green()
    );

    Ok(())
}

pub fn approve(id: &str) -> Result<()> {
    set_status(id, Status::Approved, "Approved")
}

pub fn start(id: &str) -> Result<()> {
    set_status(id, Status::InProgress, "Started")
}

pub fn done(id: &str) -> Result<()> {
    let store = Store::open()?;
    let bug = store.get_bug(id)?;

    let old_status = bug.status().clone();
    let bug_id = bug.id().to_string();
    let bug_title = bug.title().to_string();

    if matches!(old_status, Status::Done) {
        return Err(anyhow!(
            "cannot change status of completed bug '{}'",
            bug_id
        ));
    }

    // Track if we switched directories (so we can switch back)
    let original_dir = std::env::current_dir().ok();
    let mut switched_to_workspace = false;

    // Check if we're running from a workspace
    let mut in_workspace = is_in_workspace(&bug_id);

    // If not in workspace, check if one exists and switch to it
    if !in_workspace {
        if let Some(workspace_dir) = find_workspace_dir(&bug_id) {
            println!(
                "{} Found workspace at {}, switching...",
                "→".blue(),
                workspace_dir.display()
            );
            std::env::set_current_dir(&workspace_dir)?;
            switched_to_workspace = true;
            in_workspace = true;
        }
    }

    // Re-open store after potential directory switch so events are written to the right location
    let store = Store::open()?;

    if in_workspace {
        // Running from workspace - do the full workflow
        println!(
            "{} Running from workspace for bug {}",
            "→".blue(),
            bug_id.cyan()
        );

        // 1. Snapshot any uncommitted changes
        jj_snapshot()?;

        // 2. Generate and set commit message
        let commit_message = format!("Implement {} ({})", bug_title, bug_id);
        jj_describe(&commit_message)?;

        // 3. Link the change to the bug
        if let Some(change_id) = get_current_change_id()? {
            let link_event = Event::change_linked(bug_id.clone(), change_id.clone());
            store.append_event(&link_event)?;
            println!(
                "{} Linked change {} to bug {}",
                "✓".green(),
                change_id.cyan(),
                bug_id.cyan()
            );
        }
    }

    // Emit StatusChanged event (while still in workspace if we switched)
    let status_event = Event::status_changed(bug_id.clone(), old_status.clone(), Status::Done);
    store.append_event(&status_event)?;

    println!(
        "{} Completed bug {} ({} -> {})",
        "✓".green(),
        bug_id.cyan(),
        format!("{}", old_status).dimmed(),
        format!("{}", Status::Done).green()
    );

    // Return to original directory if we switched
    if switched_to_workspace {
        if let Some(ref orig) = original_dir {
            std::env::set_current_dir(orig)?;
            println!("{} Returned to {}", "→".blue(), orig.display());
        }
    }

    // Only create a fresh jj change if NOT in a workspace
    // (workspace changes stay as-is for review/submission)
    if !in_workspace {
        create_fresh_change_if_needed()?;
    }

    Ok(())
}

/// Find workspace directory for a bug, looking at ../ws-{bug_id} relative to repo root
fn find_workspace_dir(bug_id: &str) -> Option<std::path::PathBuf> {
    // Try to find workspace relative to repo root
    if let Ok(output) = std::process::Command::new("jj")
        .args(["workspace", "root"])
        .output()
    {
        if output.status.success() {
            let repo_root = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let workspace_dir = Path::new(&repo_root)
                .parent()
                .map(|p| p.join(format!("ws-{}", bug_id)));

            if let Some(ref path) = workspace_dir {
                if path.exists() {
                    return workspace_dir;
                }
            }
        }
    }

    None
}

/// Check if current jj change is empty and create a new one if needed
fn create_fresh_change_if_needed() -> Result<()> {
    // Check if current change is empty
    let output = std::process::Command::new("jj")
        .args([
            "log",
            "-r",
            "@",
            "--no-graph",
            "-T",
            "if(empty, \"empty\", \"has_changes\")",
        ])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if result == "has_changes" {
                // Current change has content, create a fresh one
                let new_output = std::process::Command::new("jj").args(["new"]).output()?;

                if new_output.status.success() {
                    println!("{} Created fresh change for next task", "→".blue());
                } else {
                    // Log the error but don't fail the done command
                    let stderr = String::from_utf8_lossy(&new_output.stderr);
                    eprintln!(
                        "{} Failed to create new change: {}",
                        "!".yellow(),
                        stderr.trim()
                    );
                }
            } else {
                println!(
                    "{} Current change is empty, ready for next task",
                    "→".blue()
                );
            }
        }
        Ok(output) => {
            // jj command failed - might not be in a jj repo
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.is_empty() {
                // Only warn if there's an actual error message, skip silently if not in jj repo
                eprintln!(
                    "{} Could not check change status: {}",
                    "!".yellow(),
                    stderr.trim()
                );
            }
        }
        Err(_) => {
            // jj not available, silently skip
        }
    }

    Ok(())
}

/// Check if we're running from a workspace directory for the given bug
/// Returns true if in workspace ws-{bug_id}, false otherwise
fn is_in_workspace(bug_id: &str) -> bool {
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(dir_name) = cwd.file_name().and_then(|n| n.to_str()) {
            return dir_name == format!("ws-{}", bug_id);
        }
    }
    false
}

/// Trigger jj to snapshot any uncommitted changes
/// jj auto-snapshots on most commands, so we run `jj log -n0` (no output, just snapshot)
fn jj_snapshot() -> Result<()> {
    let output = std::process::Command::new("jj")
        .args(["log", "-n0"])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            println!("{} Snapshotted working copy changes", "→".blue());
            Ok(())
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow!("jj log failed: {}", stderr.trim()))
        }
        Err(e) => Err(anyhow!("failed to run jj log: {}", e)),
    }
}

/// Set the commit description using jj describe
fn jj_describe(message: &str) -> Result<()> {
    let output = std::process::Command::new("jj")
        .args(["describe", "-m", message])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            println!("{} Set commit message", "→".blue());
            Ok(())
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow!("jj describe failed: {}", stderr.trim()))
        }
        Err(e) => Err(anyhow!("failed to run jj describe: {}", e)),
    }
}

/// Get the current change ID
fn get_current_change_id() -> Result<Option<String>> {
    let output = std::process::Command::new("jj")
        .args(["log", "-r", "@", "--no-graph", "-T", "change_id"])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let change_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if change_id.is_empty() {
                Ok(None)
            } else {
                Ok(Some(change_id))
            }
        }
        _ => Ok(None),
    }
}
