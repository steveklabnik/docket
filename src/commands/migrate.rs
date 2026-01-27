//! Migrate from filesystem storage to state branch storage.
//!
//! This command migrates an existing `.docket/` directory in the working tree
//! to the new state branch model where docket data is stored on an orphan branch.

use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::commands::status::jj;
use crate::event;
use crate::store::{Store, StoreMode};

/// Migrate existing .docket to state branch storage.
///
/// This is idempotent - if already on state branch mode, it returns early.
pub fn migrate() -> Result<()> {
    let store = Store::open()?;

    // Check if already using state branch mode
    match &store.mode {
        StoreMode::StateBranch { .. } => {
            println!(
                "{} Already using state branch mode - nothing to migrate",
                "!".yellow()
            );
            Ok(())
        }
        StoreMode::FileSystem { root } => {
            // Continue with migration
            migrate_from_filesystem(root.clone())
        }
    }
}

/// Perform the actual migration from filesystem to state branch.
fn migrate_from_filesystem(docket_root: PathBuf) -> Result<()> {
    println!("{} Migrating to state branch storage...", "→".blue());

    // 1. Check if we're in a jj repository
    let workspace_root = get_jj_workspace_root()?;

    // 2. Create state branch if not exists
    if !jj::has_state_branch()? {
        println!("{} Creating state branch...", "→".blue());
        jj::init_state_branch()?;
    }

    // 3. Collect all files to migrate
    let changes_dir = docket_root.join("changes");
    let releases_dir = docket_root.join("releases");

    let mut files_to_migrate: Vec<(PathBuf, String, String)> = Vec::new(); // (source_path, dest_path, content)

    if changes_dir.exists() {
        collect_change_files(&changes_dir, &mut files_to_migrate)?;
    }

    if releases_dir.exists() {
        collect_release_files(&releases_dir, &mut files_to_migrate)?;
    }

    let changes_count = files_to_migrate
        .iter()
        .filter(|(_, dest, _)| dest.contains("/changes/"))
        .count();
    let releases_count = files_to_migrate
        .iter()
        .filter(|(_, dest, _)| dest.contains("/releases/"))
        .count();

    // 4. Write all files to state branch using working copy approach
    if !files_to_migrate.is_empty() {
        write_files_to_state_branch(&workspace_root, &files_to_migrate)?;
    }

    // 5. Remove .docket from the working copy
    println!("{} Removing .docket from working copy...", "→".blue());
    remove_docket_from_working_copy(&workspace_root, &docket_root)?;

    println!(
        "{} Migration complete: {} changes, {} releases migrated to state branch",
        "✓".green(),
        changes_count,
        releases_count
    );

    Ok(())
}

/// Get the jj workspace root, or return an error if not in a jj repo.
fn get_jj_workspace_root() -> Result<String> {
    let output = match Command::new("jj").args(["workspace", "root"]).output() {
        Ok(output) => output,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(anyhow!(
                "docket migrate requires a jj repository.\n\
                 jj is not installed or not in PATH.\n\
                 Install jj from https://martinvonz.github.io/jj/latest/install-and-setup/"
            ));
        }
        Err(e) => {
            return Err(anyhow!("failed to run jj workspace root: {}", e));
        }
    };

    if !output.status.success() {
        return Err(anyhow!(
            "docket migrate requires a jj repository.\n\
             The current directory is not in a jj workspace."
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Collect all change files to migrate.
fn collect_change_files(
    changes_dir: &Path,
    files: &mut Vec<(PathBuf, String, String)>,
) -> Result<()> {
    // Pattern for sharded structure: changes/*/*.jsonl
    let sharded_pattern = changes_dir.join("*").join("*.jsonl");
    if let Some(pattern_str) = sharded_pattern.to_str() {
        for entry in glob::glob(pattern_str)? {
            let path = entry?;
            if let Some((dest_path, content)) = prepare_change_file(&path)? {
                files.push((path, dest_path, content));
            }
        }
    }

    // Pattern for flat structure (backward compat): changes/*.jsonl
    let flat_pattern = changes_dir.join("*.jsonl");
    if let Some(pattern_str) = flat_pattern.to_str() {
        for entry in glob::glob(pattern_str)? {
            let path = entry?;
            // Skip if this is inside a shard directory
            if let Some(parent) = path.parent() {
                if parent != changes_dir {
                    continue; // Already handled in sharded pattern
                }
            }
            if let Some((dest_path, content)) = prepare_change_file(&path)? {
                files.push((path, dest_path, content));
            }
        }
    }

    Ok(())
}

/// Prepare a change file for migration.
/// Returns (destination_path, content) if valid, None if should skip.
fn prepare_change_file(path: &Path) -> Result<Option<(String, String)>> {
    // Extract change ID from filename
    let change_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("invalid change file name: {}", path.display()))?;

    // Read the file content
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;

    // Validate the content by parsing events
    let events = event::parse_jsonl_content(&content)
        .with_context(|| format!("failed to parse events from {}", path.display()))?;

    if events.is_empty() {
        eprintln!(
            "{} Skipping empty change file: {}",
            "!".yellow(),
            path.display()
        );
        return Ok(None);
    }

    // Build the state branch path: .docket/changes/{first_char}/{id}.jsonl
    let shard = change_id
        .chars()
        .next()
        .ok_or_else(|| anyhow!("empty change ID"))?;
    let dest_path = format!(".docket/changes/{}/{}.jsonl", shard, change_id);

    Ok(Some((dest_path, content)))
}

/// Collect all release files to migrate.
fn collect_release_files(
    releases_dir: &Path,
    files: &mut Vec<(PathBuf, String, String)>,
) -> Result<()> {
    let pattern = releases_dir.join("*.jsonl");
    if let Some(pattern_str) = pattern.to_str() {
        for entry in glob::glob(pattern_str)? {
            let path = entry?;
            if let Some((dest_path, content)) = prepare_release_file(&path)? {
                files.push((path, dest_path, content));
            }
        }
    }

    Ok(())
}

/// Prepare a release file for migration.
/// Returns (destination_path, content) if valid, None if should skip.
fn prepare_release_file(path: &Path) -> Result<Option<(String, String)>> {
    // Extract release version from filename
    let version = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("invalid release file name: {}", path.display()))?;

    // Read the file content
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;

    if content.trim().is_empty() {
        eprintln!(
            "{} Skipping empty release file: {}",
            "!".yellow(),
            path.display()
        );
        return Ok(None);
    }

    // Build the state branch path: .docket/releases/{version}.jsonl
    let dest_path = format!(".docket/releases/{}.jsonl", version);

    Ok(Some((dest_path, content)))
}

/// Write all files to the state branch using the working copy approach.
///
/// This function:
/// 1. Saves the current change ID
/// 2. Edits the state branch
/// 3. Writes all files to the working copy
/// 4. Describes the commit
/// 5. Returns to the original change
fn write_files_to_state_branch(
    workspace_root: &str,
    files: &[(PathBuf, String, String)],
) -> Result<()> {
    // Save the current change ID so we can return to it
    let original_change = jj::current_change_id()?;

    // Helper to restore original change on error
    let restore = || {
        let _ = Command::new("jj").args(["edit", &original_change]).output();
    };

    // Edit the state branch
    let edit_output = Command::new("jj")
        .args(["edit", jj::STATE_BRANCH])
        .output()
        .context("failed to run jj edit")?;

    if !edit_output.status.success() {
        let stderr = String::from_utf8_lossy(&edit_output.stderr);
        return Err(anyhow!("failed to edit state branch: {}", stderr.trim()));
    }

    // Clean up the working directory: delete everything except .docket/, .jj/, .git/
    // This is necessary because the state branch doesn't have a .gitignore,
    // so files like target/ that were gitignored on the original branch
    // would be seen as new untracked files by jj
    if let Err(e) = cleanup_working_directory(workspace_root) {
        restore();
        return Err(e);
    }

    // Write all files to the working copy
    let workspace_path = Path::new(workspace_root);
    for (_source, dest_path, content) in files {
        let full_path = workspace_path.join(dest_path);

        // Create parent directories
        if let Some(parent) = full_path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                restore();
                return Err(anyhow!(
                    "failed to create directory {}: {}",
                    parent.display(),
                    e
                ));
            }
        }

        // Write the file
        if let Err(e) = fs::write(&full_path, content) {
            restore();
            return Err(anyhow!("failed to write {}: {}", full_path.display(), e));
        }
    }

    // Describe the commit
    let file_count = files.len();
    let describe_output = Command::new("jj")
        .args([
            "describe",
            "-m",
            &format!("docket: migrate {} files from filesystem", file_count),
        ])
        .output()
        .context("failed to run jj describe")?;

    if !describe_output.status.success() {
        let stderr = String::from_utf8_lossy(&describe_output.stderr);
        restore();
        return Err(anyhow!("failed to describe commit: {}", stderr.trim()));
    }

    // Return to the original change
    let return_output = Command::new("jj")
        .args(["edit", &original_change])
        .output()
        .context("failed to run jj edit")?;

    if !return_output.status.success() {
        let stderr = String::from_utf8_lossy(&return_output.stderr);
        return Err(anyhow!(
            "failed to return to original change: {}",
            stderr.trim()
        ));
    }

    println!(
        "  {} Migrated {} files to state branch",
        "→".blue(),
        file_count
    );

    Ok(())
}

/// Remove the .docket directory from the working copy.
///
/// This uses jj to properly track the removal and commit it.
fn remove_docket_from_working_copy(workspace_root: &str, docket_root: &Path) -> Result<()> {
    // Get the relative path of .docket from workspace root
    let workspace_path = Path::new(workspace_root);
    let relative_docket = docket_root
        .strip_prefix(workspace_path)
        .unwrap_or(Path::new(".docket"));

    // First, check if the path exists in the working copy
    if !docket_root.exists() {
        // Already removed
        return Ok(());
    }

    // Remove the directory from the filesystem
    // jj will automatically detect this change and track it
    fs::remove_dir_all(docket_root)
        .with_context(|| format!("failed to remove {}", docket_root.display()))?;

    // Describe the commit to indicate the migration
    let describe_output = Command::new("jj")
        .args([
            "describe",
            "-m",
            &format!(
                "docket: migrate {} to state branch",
                relative_docket.display()
            ),
        ])
        .output()
        .context("failed to run jj describe")?;

    if !describe_output.status.success() {
        let stderr = String::from_utf8_lossy(&describe_output.stderr);
        // Don't fail if describe fails - the migration itself succeeded
        eprintln!(
            "{} Warning: could not set commit message: {}",
            "!".yellow(),
            stderr.trim()
        );
    }

    Ok(())
}

/// Clean up the working directory when on the state branch.
///
/// The state branch doesn't have a .gitignore, so files that were gitignored
/// on the original branch (like target/) would appear as new untracked files.
/// This function deletes everything in the working directory except:
/// - .docket/ (the state we're managing)
/// - .jj/ (jj's internal state)
/// - .git/ (git's internal state for colocated repos)
fn cleanup_working_directory(workspace_root: &str) -> Result<()> {
    let workspace_path = Path::new(workspace_root);

    let entries = fs::read_dir(workspace_path)
        .with_context(|| format!("failed to read directory {}", workspace_root))?;

    for entry in entries {
        let entry = entry.with_context(|| format!("failed to read entry in {}", workspace_root))?;
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();

        // Keep .docket/, .jj/, and .git/
        if name == ".docket" || name == ".jj" || name == ".git" {
            continue;
        }

        let path = entry.path();
        if path.is_dir() {
            fs::remove_dir_all(&path)
                .with_context(|| format!("failed to remove directory {}", path.display()))?;
        } else {
            fs::remove_file(&path)
                .with_context(|| format!("failed to remove file {}", path.display()))?;
        }
    }

    Ok(())
}
