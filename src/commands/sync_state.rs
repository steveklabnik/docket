//! Sync docket state branch with remote.

use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use std::process::Command;

use crate::store::{Store, StoreMode, STATE_BRANCH_NAME};

/// Sync docket state with remote (fetch + push)
pub fn sync_state() -> Result<()> {
    let store = Store::open()?;

    // Get the repo root based on store mode
    let repo_root = match &store.mode {
        StoreMode::StateBranch { repo_root } => repo_root.clone(),
        StoreMode::FileSystem { root } => {
            // For FileSystem mode, we need to find the repo root
            find_repo_root(root)?
        }
    };

    // Verify jj is available
    verify_jj_available()?;

    // Check if the state branch exists
    if !state_branch_exists(&repo_root)? {
        return Err(anyhow!(
            "docket-state bookmark not found.\n\
             This repository may be using file-based storage.\n\
             Sync is only needed for state branch storage mode."
        ));
    }

    // Fetch latest from remote
    println!("{} Fetching state from remote...", "→".blue());
    let fetch_result = fetch_from_remote(&repo_root);

    match &fetch_result {
        Ok(()) => println!("{} Fetched latest changes", "✓".green()),
        Err(e) => {
            let err_msg = e.to_string();
            if err_msg.contains("No git remotes are configured")
                || err_msg.contains("no remotes")
                || err_msg.contains("remote")
            {
                println!("{} No remote configured - skipping fetch", "→".blue());
            } else {
                return Err(anyhow!("fetch failed: {}", e));
            }
        }
    }

    // Push local state to remote
    println!("{} Pushing state to remote...", "→".blue());
    let push_result = push_state_branch(&repo_root);

    match push_result {
        Ok(()) => {
            println!("{} State synchronized", "✓".green());
            Ok(())
        }
        Err(e) => {
            let err_msg = e.to_string();
            if err_msg.contains("No git remotes are configured")
                || err_msg.contains("no remotes")
                || err_msg.contains("remote")
            {
                println!("{} No remote configured - skipping push", "→".blue());
                println!("{} State sync complete (local only)", "✓".green());
                Ok(())
            } else {
                Err(anyhow!("push failed: {}", e))
            }
        }
    }
}

/// Find the repository root from a .docket directory path
fn find_repo_root(docket_path: &std::path::Path) -> Result<std::path::PathBuf> {
    // .docket is typically at repo_root/.docket, so parent is repo root
    let parent = docket_path
        .parent()
        .ok_or_else(|| anyhow!("could not determine repository root from .docket path"))?;

    // Verify this is actually a repo
    if parent.join(".jj").is_dir() || parent.join(".git").exists() {
        Ok(parent.to_path_buf())
    } else {
        // Try using jj to find the workspace root
        let output = Command::new("jj")
            .args(["workspace", "root"])
            .output()
            .context("failed to get jj workspace root")?;

        if output.status.success() {
            let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Ok(std::path::PathBuf::from(root))
        } else {
            Err(anyhow!(
                "not in a jj/git repository.\n\
                 State sync requires a version control repository."
            ))
        }
    }
}

/// Verify jj is available
fn verify_jj_available() -> Result<()> {
    let jj_check = Command::new("jj").arg("--version").output();
    if jj_check.is_err() {
        return Err(anyhow!(
            "jj is not installed or not in PATH.\n\
             Install jj from https://martinvonz.github.io/jj/latest/install-and-setup/"
        ));
    }
    Ok(())
}

/// Check if the docket-state bookmark exists
fn state_branch_exists(repo_root: &std::path::Path) -> Result<bool> {
    let output = Command::new("jj")
        .args([
            "bookmark",
            "list",
            "--repository",
            repo_root.to_str().unwrap_or("."),
        ])
        .output()
        .context("failed to list jj bookmarks")?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout
            .lines()
            .any(|line| line.split(':').next().map(|name| name.trim()) == Some(STATE_BRANCH_NAME)))
    } else {
        Ok(false)
    }
}

/// Fetch from git remote
fn fetch_from_remote(repo_root: &std::path::Path) -> Result<()> {
    let output = Command::new("jj")
        .args(["git", "fetch"])
        .current_dir(repo_root)
        .output()
        .context("failed to run jj git fetch")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("{}", stderr.trim()));
    }

    Ok(())
}

/// Push the state branch to remote
fn push_state_branch(repo_root: &std::path::Path) -> Result<()> {
    let output = Command::new("jj")
        .args(["git", "push", "--bookmark", STATE_BRANCH_NAME])
        .current_dir(repo_root)
        .output()
        .context("failed to run jj git push")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("{}", stderr.trim()));
    }

    Ok(())
}
