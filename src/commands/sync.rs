//! Sync workspaces with trunk by rebasing onto latest main branch.

use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::bug::Status;
use crate::store::Store;

/// Result of syncing a single workspace
#[derive(Debug)]
enum SyncResult {
    /// Successfully rebased
    Synced,
    /// Already up to date (no rebase needed)
    UpToDate,
    /// Rebase produced conflicts
    Conflicts(String),
    /// Push succeeded (only if --push was used)
    Pushed,
    /// Error during sync
    Error(String),
}

/// Info about a workspace to sync
struct WorkspaceInfo {
    name: String,
    path: PathBuf,
    #[allow(dead_code)]
    bug_id: String,
    bug_title: String,
}

/// Main entry point for sync command
pub fn sync(
    id: Option<&str>,
    no_push: bool,
    no_fetch: bool,
    no_rebase: bool,
    dry_run: bool,
) -> Result<()> {
    // 1. Verify jj is available
    verify_jj_available()?;

    // 2. Get repo root and parent directory
    let repo_root = get_repo_root()?;
    let parent_dir = Path::new(&repo_root)
        .parent()
        .ok_or_else(|| anyhow!("could not determine parent directory of repo"))?;

    // 3. Fetch from remote (unless --no-fetch)
    if !no_fetch {
        fetch_from_remote(&repo_root, dry_run)?;
    }

    // 4. Find workspaces to sync
    let workspaces = find_workspaces_to_sync(parent_dir, id)?;

    if workspaces.is_empty() {
        println!("{} No workspaces to sync", "→".blue());
        return Ok(());
    }

    // 5. Sync each workspace (rebase and push by default)
    let push = !no_push;
    let rebase = !no_rebase;
    let results = sync_workspaces(&workspaces, rebase, push, dry_run)?;

    // 6. Print summary
    print_summary(&results);

    Ok(())
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

/// Get the jj repo root
fn get_repo_root() -> Result<String> {
    let repo_root = Command::new("jj")
        .args(["workspace", "root"])
        .output()
        .context("failed to get jj workspace root")?;

    if !repo_root.status.success() {
        return Err(anyhow!(
            "not in a jj repository.\n\
             Run 'jj git init' to initialize jj in an existing git repository,\n\
             or 'jj git clone <url>' to clone a repository with jj."
        ));
    }

    Ok(String::from_utf8_lossy(&repo_root.stdout)
        .trim()
        .to_string())
}

/// Fetch from git remote
fn fetch_from_remote(repo_root: &str, dry_run: bool) -> Result<()> {
    println!("{} Fetching from remote...", "→".blue());

    if dry_run {
        println!("  {} Would run: jj git fetch", "(dry-run)".dimmed());
        return Ok(());
    }

    let output = Command::new("jj")
        .args(["git", "fetch"])
        .current_dir(repo_root)
        .output()
        .context("failed to run jj git fetch")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("jj git fetch failed: {}", stderr.trim()));
    }

    println!("{} Fetched latest changes", "✓".green());
    Ok(())
}

/// Find all workspaces that should be synced
fn find_workspaces_to_sync(
    parent_dir: &Path,
    filter_id: Option<&str>,
) -> Result<Vec<WorkspaceInfo>> {
    let store = Store::open()?;
    let mut workspaces = Vec::new();

    // Find all ws-* directories
    let entries = fs::read_dir(parent_dir)
        .with_context(|| format!("failed to read directory {}", parent_dir.display()))?;

    for entry in entries {
        let entry = entry?;
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy().to_string();

        if !name.starts_with("ws-") || !entry.path().is_dir() {
            continue;
        }

        let bug_id = name.strip_prefix("ws-").unwrap();

        // Apply ID filter if specified
        if let Some(filter) = filter_id {
            if !bug_id.starts_with(filter) {
                continue;
            }
        }

        // Check if bug exists and is in syncable state
        // Include Draft bugs since they may have active PRs
        // Include Blocked bugs since they still have active workspaces
        match store.get_bug(bug_id) {
            Ok(bug) => match bug.status() {
                Status::InProgress
                | Status::Approved
                | Status::Draft
                | Status::Review
                | Status::Blocked => {
                    workspaces.push(WorkspaceInfo {
                        name: name.clone(),
                        path: entry.path(),
                        bug_id: bug_id.to_string(),
                        bug_title: bug.title().to_string(),
                    });
                }
                Status::Done | Status::NotPlanned => {
                    // Skip - should be cleaned up
                }
            },
            Err(_) => {
                // Workspace exists but bug not found - orphan workspace
                println!(
                    "{} Skipping {} - bug not found (orphan workspace?)",
                    "!".yellow(),
                    name.cyan()
                );
            }
        }
    }

    Ok(workspaces)
}

/// Sync all workspaces
fn sync_workspaces(
    workspaces: &[WorkspaceInfo],
    rebase: bool,
    push: bool,
    dry_run: bool,
) -> Result<Vec<(String, SyncResult)>> {
    let mut results = Vec::new();

    println!();
    println!(
        "{} Syncing {} workspace{}...",
        "→".blue(),
        workspaces.len(),
        if workspaces.len() == 1 { "" } else { "s" }
    );
    println!();

    for ws in workspaces {
        println!("  {} ({})", ws.name.cyan(), ws.bug_title);

        let result = sync_single_workspace(ws, rebase, push, dry_run);

        match &result {
            SyncResult::Synced => {
                println!("    {} Rebased successfully", "✓".green());
            }
            SyncResult::UpToDate => {
                if push {
                    println!("    {} Already up to date", "→".blue());
                } else {
                    println!("    {} Already up to date", "✓".green());
                }
            }
            SyncResult::Conflicts(msg) => {
                println!("    {} Conflicts detected - not pushed", "!".yellow());
                println!("      {}", msg.dimmed());
            }
            SyncResult::Pushed => {
                println!("    {} Synced and pushed", "✓".green());
            }
            SyncResult::Error(msg) => {
                println!("    {} Error: {}", "✗".red(), msg);
            }
        }

        results.push((ws.name.clone(), result));
        println!();
    }

    Ok(results)
}

/// Sync a single workspace
fn sync_single_workspace(
    ws: &WorkspaceInfo,
    rebase: bool,
    push: bool,
    dry_run: bool,
) -> SyncResult {
    // First, update stale workspace if needed
    if let Err(e) = update_stale_workspace(&ws.path, dry_run) {
        return SyncResult::Error(e.to_string());
    }

    // Check for existing conflicts before doing anything
    if has_conflicts(&ws.path) {
        // Try to resolve with Claude
        match resolve_conflicts_with_claude(&ws.path, dry_run) {
            Ok(true) => {
                // Conflicts resolved, continue with sync
            }
            Ok(false) => {
                return SyncResult::Conflicts(
                    "unresolved conflicts - manual resolution needed".to_string(),
                );
            }
            Err(e) => {
                return SyncResult::Error(format!("conflict resolution failed: {}", e));
            }
        }
    }

    // Check if rebase is needed
    let needs_rebase = if rebase {
        match check_needs_rebase(&ws.path) {
            Ok(needs) => needs,
            Err(e) => return SyncResult::Error(e.to_string()),
        }
    } else {
        false
    };

    if dry_run {
        if needs_rebase {
            println!(
                "    {} Would run: jj rebase -d trunk()",
                "(dry-run)".dimmed()
            );
        }
        if push {
            println!("    {} Would run: jj git push -c @", "(dry-run)".dimmed());
        }
        return if needs_rebase {
            SyncResult::Synced
        } else {
            SyncResult::UpToDate
        };
    }

    // Perform rebase if needed
    if needs_rebase {
        println!("    {} Rebasing onto trunk()...", "→".blue());

        let rebase_output = Command::new("jj")
            .args(["rebase", "-d", "trunk()"])
            .current_dir(&ws.path)
            .output();

        match rebase_output {
            Ok(output) if output.status.success() => {
                // Check for conflicts after rebase
                if has_conflicts(&ws.path) {
                    // Try to resolve with Claude
                    match resolve_conflicts_with_claude(&ws.path, dry_run) {
                        Ok(true) => {
                            // Conflicts resolved, continue to push
                        }
                        Ok(false) => {
                            return SyncResult::Conflicts(
                                "resolve conflicts, then run 'docket sync' again".to_string(),
                            );
                        }
                        Err(e) => {
                            return SyncResult::Error(format!("conflict resolution failed: {}", e));
                        }
                    }
                }
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if stderr.contains("conflict") {
                    // Rebase introduced conflicts, try to resolve
                    match resolve_conflicts_with_claude(&ws.path, dry_run) {
                        Ok(true) => {
                            // Conflicts resolved, continue to push
                        }
                        Ok(false) => {
                            return SyncResult::Conflicts(
                                "resolve conflicts, then run 'docket sync' again".to_string(),
                            );
                        }
                        Err(e) => {
                            return SyncResult::Error(format!("conflict resolution failed: {}", e));
                        }
                    }
                } else {
                    return SyncResult::Error(stderr.trim().to_string());
                }
            }
            Err(e) => return SyncResult::Error(e.to_string()),
        }
    }

    // Push if requested
    if push {
        match push_change(&ws.path) {
            Ok(()) => SyncResult::Pushed,
            Err(e) => SyncResult::Error(format!("push failed: {}", e)),
        }
    } else if needs_rebase {
        SyncResult::Synced
    } else {
        SyncResult::UpToDate
    }
}

/// Update a stale workspace if needed
fn update_stale_workspace(workspace_path: &Path, dry_run: bool) -> Result<()> {
    // Check if workspace is stale by trying a simple jj command
    let check = Command::new("jj")
        .args(["status"])
        .current_dir(workspace_path)
        .output()
        .context("failed to check workspace status")?;

    let stderr = String::from_utf8_lossy(&check.stderr);
    if stderr.contains("stale") {
        if dry_run {
            println!(
                "    {} Would run: jj workspace update-stale",
                "(dry-run)".dimmed()
            );
            return Ok(());
        }

        println!("    {} Updating stale workspace...", "→".blue());
        let update = Command::new("jj")
            .args(["workspace", "update-stale"])
            .current_dir(workspace_path)
            .output()
            .context("failed to update stale workspace")?;

        if !update.status.success() {
            let err = String::from_utf8_lossy(&update.stderr);
            return Err(anyhow!("failed to update stale workspace: {}", err.trim()));
        }
    }

    Ok(())
}

const RESOLVE_PROMPT: &str = r#"Resolve jj merge conflicts in this workspace.

1. Run `jj status` to see which files have conflicts
2. For each conflicted file, read it and resolve by editing

jj conflict format:
```
<<<<<<< conflict N of M
+++++++ [destination info]
content from destination (trunk)
%%%%%%% diff from base to our branch
+lines we added
>>>>>>> conflict N of M ends
```

To resolve: COMBINE both sides. Keep destination content AND apply our additions.
Example: if destination added `pub mod edit;` and we added `pub mod completions;`,
the result should have BOTH modules, with all conflict markers removed.

After editing all files, run `jj status` to verify no conflicts remain.
"#;

/// Attempt to resolve conflicts using Claude
fn resolve_conflicts_with_claude(workspace_path: &Path, dry_run: bool) -> Result<bool> {
    if dry_run {
        println!(
            "    {} Would invoke Claude to resolve conflicts",
            "(dry-run)".dimmed()
        );
        return Ok(false); // In dry-run, pretend conflicts remain
    }

    println!("    {} Invoking Claude to resolve conflicts...", "→".blue());

    let status = Command::new("claude")
        .args([
            "-p",
            RESOLVE_PROMPT,
            "--model",
            "sonnet",
            "--allowedTools",
            "Bash,Read,Edit,Write",
        ])
        .current_dir(workspace_path)
        .status();

    match status {
        Ok(status) if status.success() => {
            // Check if conflicts are actually resolved
            if has_conflicts(workspace_path) {
                println!("    {} Claude finished but conflicts remain", "!".yellow());
                Ok(false)
            } else {
                println!("    {} Conflicts resolved by Claude", "✓".green());
                Ok(true)
            }
        }
        Ok(_status) => {
            println!("    {} Claude failed to resolve conflicts", "!".yellow(),);
            Ok(false)
        }
        Err(e) => {
            println!("    {} Failed to run Claude ({})", "!".yellow(), e);
            Ok(false)
        }
    }
}

/// Check if rebase is needed (current parent != trunk)
fn check_needs_rebase(workspace_path: &Path) -> Result<bool> {
    // Get trunk change ID
    let trunk_output = Command::new("jj")
        .args(["log", "-r", "trunk()", "--no-graph", "-T", "change_id"])
        .current_dir(workspace_path)
        .output()
        .context("failed to get trunk change id")?;

    if !trunk_output.status.success() {
        let stderr = String::from_utf8_lossy(&trunk_output.stderr);
        return Err(anyhow!("failed to get trunk: {}", stderr.trim()));
    }

    // Get parent change ID
    let parent_output = Command::new("jj")
        .args(["log", "-r", "@-", "--no-graph", "-T", "change_id"])
        .current_dir(workspace_path)
        .output()
        .context("failed to get parent change id")?;

    if !parent_output.status.success() {
        let stderr = String::from_utf8_lossy(&parent_output.stderr);
        return Err(anyhow!("failed to get parent: {}", stderr.trim()));
    }

    let trunk_id = String::from_utf8_lossy(&trunk_output.stdout)
        .trim()
        .to_string();
    let parent_id = String::from_utf8_lossy(&parent_output.stdout)
        .trim()
        .to_string();

    Ok(trunk_id != parent_id)
}

/// Check if workspace has conflicts
fn has_conflicts(workspace_path: &Path) -> bool {
    let output = Command::new("jj")
        .args(["status"])
        .current_dir(workspace_path)
        .output();

    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout.contains("conflict")
        }
        Err(_) => false,
    }
}

/// Push the current change
fn push_change(workspace_path: &Path) -> Result<()> {
    println!("    {} Pushing to remote...", "→".blue());

    let push_output = Command::new("jj")
        .args(["git", "push", "-c", "@"])
        .current_dir(workspace_path)
        .output()
        .context("failed to push")?;

    if !push_output.status.success() {
        let stderr = String::from_utf8_lossy(&push_output.stderr);
        return Err(anyhow!("{}", stderr.trim()));
    }

    Ok(())
}

/// Print summary of results
fn print_summary(results: &[(String, SyncResult)]) {
    let synced = results
        .iter()
        .filter(|(_, r)| matches!(r, SyncResult::Synced))
        .count();
    let pushed = results
        .iter()
        .filter(|(_, r)| matches!(r, SyncResult::Pushed))
        .count();
    let up_to_date = results
        .iter()
        .filter(|(_, r)| matches!(r, SyncResult::UpToDate))
        .count();
    let conflicts = results
        .iter()
        .filter(|(_, r)| matches!(r, SyncResult::Conflicts(_)))
        .count();
    let errors = results
        .iter()
        .filter(|(_, r)| matches!(r, SyncResult::Error(_)))
        .count();

    println!("{}", "─".repeat(40).dimmed());
    println!("Summary:");

    if synced > 0 {
        println!("  {} {} synced", "✓".green(), synced);
    }
    if pushed > 0 {
        println!("  {} {} pushed", "✓".green(), pushed);
    }
    if up_to_date > 0 {
        println!("  {} {} already up to date", "→".blue(), up_to_date);
    }
    if conflicts > 0 {
        println!(
            "  {} {} with conflicts (manual resolution needed)",
            "!".yellow(),
            conflicts
        );
    }
    if errors > 0 {
        println!("  {} {} failed", "✗".red(), errors);
    }
}
