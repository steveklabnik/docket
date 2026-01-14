use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::bug::Status;
use crate::store::Store;

/// Main entry point - routes to explicit or auto cleanup
pub fn cleanup(id: Option<&str>) -> Result<()> {
    match id {
        Some(id) => cleanup_explicit(id),
        None => cleanup_done(),
    }
}

/// Clean up a specific workspace by bug ID
fn cleanup_explicit(id: &str) -> Result<()> {
    let store = Store::open()?;
    let bug = store.get_bug(id)?;

    let bug_id = bug.id().to_string();
    cleanup_workspace(&bug_id)
}

/// Find and clean up all workspaces whose bugs are marked done
fn cleanup_done() -> Result<()> {
    let repo_root = get_repo_root()?;
    let parent_dir = Path::new(&repo_root)
        .parent()
        .ok_or_else(|| anyhow!("could not determine parent directory of repo"))?;

    // Find all ws-* directories
    let workspaces = find_workspaces(parent_dir)?;

    if workspaces.is_empty() {
        println!("{} No workspaces found", "→".blue());
        return Ok(());
    }

    println!(
        "{} Found {} workspace{}",
        "→".blue(),
        workspaces.len(),
        if workspaces.len() == 1 { "" } else { "s" }
    );

    let mut cleaned_count = 0;
    let mut skipped_count = 0;

    let store = Store::open()?;

    for workspace_name in workspaces {
        // Extract bug ID from workspace name (ws-{bug_id} -> bug_id)
        let bug_id = workspace_name
            .strip_prefix("ws-")
            .unwrap_or(&workspace_name);

        match should_cleanup_workspace(bug_id, &store) {
            Ok(true) => match cleanup_workspace(bug_id) {
                Ok(()) => cleaned_count += 1,
                Err(e) => {
                    eprintln!(
                        "{} Failed to clean up {}: {}",
                        "!".yellow(),
                        workspace_name.cyan(),
                        e
                    );
                    skipped_count += 1;
                }
            },
            Ok(false) => {
                println!(
                    "{} Skipping {} - bug not done",
                    "→".blue(),
                    workspace_name.cyan()
                );
                skipped_count += 1;
            }
            Err(e) => {
                println!(
                    "{} Skipping {} - {}",
                    "→".blue(),
                    workspace_name.cyan(),
                    e.to_string().dimmed()
                );
                skipped_count += 1;
            }
        }
    }

    // Summary
    println!();
    if cleaned_count > 0 {
        println!(
            "{} Cleaned up {} workspace{}",
            "✓".green(),
            cleaned_count,
            if cleaned_count == 1 { "" } else { "s" }
        );
    }
    if skipped_count > 0 && cleaned_count == 0 {
        println!("{} No workspaces ready for cleanup", "→".blue());
    }

    Ok(())
}

/// Find all ws-* directories in the given parent directory
fn find_workspaces(parent_dir: &Path) -> Result<Vec<String>> {
    let mut workspaces = Vec::new();

    let entries = fs::read_dir(parent_dir)
        .with_context(|| format!("failed to read directory {}", parent_dir.display()))?;

    for entry in entries {
        let entry = entry?;
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();

        if name.starts_with("ws-") && entry.path().is_dir() {
            workspaces.push(name.to_string());
        }
    }

    Ok(workspaces)
}

/// Check if a workspace should be cleaned up (bug is done)
fn should_cleanup_workspace(bug_id: &str, store: &Store) -> Result<bool> {
    let bug = store.get_bug(bug_id)?;
    Ok(matches!(bug.status(), Status::Done))
}

/// Get the jj repo root
fn get_repo_root() -> Result<String> {
    // Check if jj is available
    let jj_check = Command::new("jj").arg("--version").output();
    if jj_check.is_err() {
        return Err(anyhow!("jj is not installed or not in PATH"));
    }

    let repo_root = Command::new("jj")
        .args(["workspace", "root"])
        .output()
        .context("failed to get jj workspace root")?;

    if !repo_root.status.success() {
        return Err(anyhow!("not in a jj repository"));
    }

    Ok(String::from_utf8_lossy(&repo_root.stdout)
        .trim()
        .to_string())
}

/// Clean up a specific workspace by bug ID
fn cleanup_workspace(bug_id: &str) -> Result<()> {
    let workspace_name = format!("ws-{}", bug_id);
    let repo_root = get_repo_root()?;

    let workspace_path = format!("{}/../{}", repo_root, workspace_name);
    let workspace_dir = Path::new(&workspace_path);

    // Check if workspace directory exists
    if !workspace_dir.exists() {
        println!(
            "{} Workspace {} does not exist, nothing to clean up",
            "→".blue(),
            workspace_name.cyan()
        );
        return Ok(());
    }

    // Check for uncommitted changes in the workspace
    let status_output = Command::new("jj")
        .args(["status"])
        .current_dir(&workspace_path)
        .output()
        .context("failed to check workspace status")?;

    if status_output.status.success() {
        let status_text = String::from_utf8_lossy(&status_output.stdout);
        // jj status shows "Working copy changes:" when there are uncommitted changes
        // An empty working copy shows "The working copy is clean"
        if status_text.contains("Working copy changes:") {
            eprintln!(
                "{} Workspace {} has uncommitted changes!",
                "!".yellow(),
                workspace_name.cyan()
            );
            eprintln!("{}", status_text.dimmed());
            return Err(anyhow!(
                "refusing to clean up workspace with uncommitted changes. \
                 Commit or discard changes first."
            ));
        }
    }

    // Run jj workspace forget from the main repo
    println!(
        "{} Forgetting jj workspace {}...",
        "→".blue(),
        workspace_name.cyan()
    );

    let forget_status = Command::new("jj")
        .args(["workspace", "forget", &workspace_name])
        .current_dir(&repo_root)
        .status()
        .context("failed to forget jj workspace")?;

    if !forget_status.success() {
        // Workspace might not exist in jj (already forgotten), continue with directory removal
        println!(
            "{} Workspace {} not found in jj (may already be forgotten)",
            "!".yellow(),
            workspace_name.cyan()
        );
    }

    // Remove the workspace directory
    println!(
        "{} Removing workspace directory {}...",
        "→".blue(),
        workspace_path.cyan()
    );

    fs::remove_dir_all(&workspace_path)
        .with_context(|| format!("failed to remove workspace directory {}", workspace_path))?;

    println!(
        "{} Cleaned up workspace for bug {}",
        "✓".green(),
        bug_id.cyan()
    );

    Ok(())
}
