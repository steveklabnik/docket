//! jj (Jujutsu) integration helpers for workspace detection and operations.

use anyhow::{anyhow, Result};
use colored::Colorize;
use std::path::Path;

/// Check if we're running from a workspace directory for the given bug.
/// Returns true if in workspace ws-{bug_id}, false otherwise.
pub fn is_in_workspace(bug_id: &str) -> bool {
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(dir_name) = cwd.file_name().and_then(|n| n.to_str()) {
            return dir_name == format!("ws-{}", bug_id);
        }
    }
    false
}

/// Find workspace directory for a bug, looking at ../ws-{bug_id} relative to repo root.
pub fn find_workspace_dir(bug_id: &str) -> Option<std::path::PathBuf> {
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

/// Set the commit description using jj describe.
pub fn describe(message: &str) -> Result<()> {
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
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(anyhow!(
            "jj is not installed or not in PATH.\n\
             Install jj from https://martinvonz.github.io/jj/latest/install-and-setup/"
        )),
        Err(e) => Err(anyhow!("failed to run jj describe: {}", e)),
    }
}

/// Check if current jj change is empty and create a new one if needed.
pub fn create_fresh_change_if_needed() -> Result<()> {
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
