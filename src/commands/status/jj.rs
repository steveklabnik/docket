//! jj (Jujutsu) integration helpers for workspace detection and operations.

use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use std::path::Path;
use std::process::Command;

/// The name of the bookmark used to store docket state.
pub const STATE_BRANCH: &str = "docket-state";

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

/// Push the current change to a remote branch via jj bookmark.
/// Creates a bookmark, tracks it, and pushes to origin.
pub fn push_bookmark(bookmark_name: &str) -> Result<()> {
    println!(
        "{} Creating and pushing bookmark {}...",
        "→".blue(),
        bookmark_name.cyan()
    );

    // Create the bookmark pointing to current change
    let create_output = std::process::Command::new("jj")
        .args(["bookmark", "create", bookmark_name, "-r", "@"])
        .output()
        .context("failed to run jj bookmark create")?;

    if !create_output.status.success() {
        // Bookmark might already exist, try to set it instead
        let set_output = std::process::Command::new("jj")
            .args(["bookmark", "set", bookmark_name, "-r", "@"])
            .output()
            .context("failed to run jj bookmark set")?;

        if !set_output.status.success() {
            let stderr = String::from_utf8_lossy(&set_output.stderr);
            return Err(anyhow!("failed to create/set bookmark: {}", stderr.trim()));
        }
    }

    // Track the bookmark on origin
    let track_output = std::process::Command::new("jj")
        .args(["bookmark", "track", bookmark_name, "--remote", "origin"])
        .output()
        .context("failed to run jj bookmark track")?;

    // Track might fail if already tracked, that's okay
    if !track_output.status.success() {
        let stderr = String::from_utf8_lossy(&track_output.stderr);
        // Only warn, don't fail - might already be tracked
        if !stderr.contains("already tracked") {
            eprintln!(
                "{} Warning: bookmark track: {}",
                "!".yellow(),
                stderr.trim()
            );
        }
    }

    // Push the bookmark
    let push_output = std::process::Command::new("jj")
        .args(["git", "push", "-b", bookmark_name])
        .output()
        .context("failed to run jj git push")?;

    if !push_output.status.success() {
        let stderr = String::from_utf8_lossy(&push_output.stderr);
        return Err(anyhow!("jj git push failed: {}", stderr.trim()));
    }

    println!("{} Pushed to branch {}", "✓".green(), bookmark_name.cyan());
    Ok(())
}

/// Get the git remote URL from jj's git store.
/// Works for both colocated and non-colocated repos.
pub fn get_git_remote_url() -> Result<String> {
    // Get the workspace root
    let root_output = std::process::Command::new("jj")
        .args(["workspace", "root"])
        .output()
        .context("failed to run jj workspace root")?;

    if !root_output.status.success() {
        return Err(anyhow!("not in a jj workspace"));
    }

    let root = String::from_utf8_lossy(&root_output.stdout)
        .trim()
        .to_string();

    // Try colocated first (.git), then non-colocated (.jj/repo/store/git)
    let git_dir = if Path::new(&root).join(".git").exists() {
        format!("{}/.git", root)
    } else {
        format!("{}/.jj/repo/store/git", root)
    };

    // Get the remote URL using git
    let url_output = std::process::Command::new("git")
        .args(["--git-dir", &git_dir, "remote", "get-url", "origin"])
        .output()
        .context("failed to run git remote get-url")?;

    if !url_output.status.success() {
        return Err(anyhow!(
            "could not get git remote URL. Is 'origin' remote configured?"
        ));
    }

    Ok(String::from_utf8_lossy(&url_output.stdout)
        .trim()
        .to_string())
}

/// Parse owner/repo from a GitHub URL.
/// Handles both HTTPS and SSH formats:
/// - https://github.com/owner/repo.git
/// - git@github.com:owner/repo.git
pub fn parse_github_repo(url: &str) -> Result<String> {
    let url = url.trim();

    // Try HTTPS format: https://github.com/owner/repo.git
    if let Some(rest) = url.strip_prefix("https://github.com/") {
        let repo = rest.trim_end_matches(".git");
        return Ok(repo.to_string());
    }

    // Try SSH format: git@github.com:owner/repo.git
    if let Some(rest) = url.strip_prefix("git@github.com:") {
        let repo = rest.trim_end_matches(".git");
        return Ok(repo.to_string());
    }

    Err(anyhow!(
        "could not parse GitHub repository from URL: {}\n\
         Expected format: https://github.com/owner/repo or git@github.com:owner/repo",
        url
    ))
}

/// Get the current jj change-id.
/// Returns the full change ID of the working copy (@).
pub fn current_change_id() -> Result<String> {
    let output = Command::new("jj")
        .args(["log", "-r", "@", "--no-graph", "-T", "change_id"])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let change_id = String::from_utf8(output.stdout)
                .context("invalid UTF-8 in change_id output")?
                .trim()
                .to_string();
            if change_id.is_empty() {
                Err(anyhow!("jj returned empty change_id"))
            } else {
                Ok(change_id)
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow!("jj log failed: {}", stderr.trim()))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(anyhow!(
            "jj is not installed or not in PATH.\n\
             Install jj from https://martinvonz.github.io/jj/latest/install-and-setup/"
        )),
        Err(e) => Err(anyhow!("failed to run jj: {}", e)),
    }
}

/// Check if the docket-state bookmark exists.
/// Returns true if the bookmark exists, false otherwise.
pub fn has_state_branch() -> Result<bool> {
    let output = Command::new("jj")
        .args(["bookmark", "list", "--all"])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8(output.stdout)
                .context("invalid UTF-8 in bookmark list output")?;
            // Each line in bookmark list starts with the bookmark name
            // Format is "bookmark-name: change-id [other info]" or "bookmark-name@remote: ..."
            Ok(stdout.lines().any(|line| {
                let bookmark = line.split(':').next().unwrap_or("");
                let bookmark = bookmark.split('@').next().unwrap_or(bookmark);
                bookmark == STATE_BRANCH
            }))
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow!("jj bookmark list failed: {}", stderr.trim()))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(anyhow!(
            "jj is not installed or not in PATH.\n\
             Install jj from https://martinvonz.github.io/jj/latest/install-and-setup/"
        )),
        Err(e) => Err(anyhow!("failed to run jj: {}", e)),
    }
}

/// Read file content from the state branch.
/// Returns the content of the file at the given path in the docket-state branch.
pub fn read_state_file(path: &str) -> Result<String> {
    let output = Command::new("jj")
        .args(["file", "show", "-r", STATE_BRANCH, path])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            String::from_utf8(output.stdout).context("invalid UTF-8 in file content")
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr = stderr.trim();
            // Check for common error cases
            if stderr.contains("No such path") {
                Err(anyhow!("file not found in state branch: {}", path))
            } else if stderr.contains("Revision") && stderr.contains("doesn't exist") {
                Err(anyhow!(
                    "state branch '{}' does not exist. Run 'docket init' first.",
                    STATE_BRANCH
                ))
            } else {
                Err(anyhow!("jj file show failed: {}", stderr))
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(anyhow!(
            "jj is not installed or not in PATH.\n\
             Install jj from https://martinvonz.github.io/jj/latest/install-and-setup/"
        )),
        Err(e) => Err(anyhow!("failed to run jj: {}", e)),
    }
}

/// List files in the state branch matching a glob pattern.
/// Returns a list of file paths that match the pattern.
pub fn list_state_files(pattern: &str) -> Result<Vec<String>> {
    let output = Command::new("jj")
        .args(["file", "list", "-r", STATE_BRANCH, pattern])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let stdout =
                String::from_utf8(output.stdout).context("invalid UTF-8 in file list output")?;
            Ok(stdout.lines().map(|s| s.to_string()).collect())
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr = stderr.trim();
            // Check for state branch not existing
            if stderr.contains("Revision") && stderr.contains("doesn't exist") {
                Err(anyhow!(
                    "state branch '{}' does not exist. Run 'docket init' first.",
                    STATE_BRANCH
                ))
            } else {
                Err(anyhow!("jj file list failed: {}", stderr))
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(anyhow!(
            "jj is not installed or not in PATH.\n\
             Install jj from https://martinvonz.github.io/jj/latest/install-and-setup/"
        )),
        Err(e) => Err(anyhow!("failed to run jj: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_github_repo_https() {
        assert_eq!(
            parse_github_repo("https://github.com/owner/repo.git").unwrap(),
            "owner/repo"
        );
        assert_eq!(
            parse_github_repo("https://github.com/owner/repo").unwrap(),
            "owner/repo"
        );
    }

    #[test]
    fn test_parse_github_repo_ssh() {
        assert_eq!(
            parse_github_repo("git@github.com:owner/repo.git").unwrap(),
            "owner/repo"
        );
        assert_eq!(
            parse_github_repo("git@github.com:owner/repo").unwrap(),
            "owner/repo"
        );
    }

    #[test]
    fn test_parse_github_repo_invalid() {
        assert!(parse_github_repo("https://gitlab.com/owner/repo").is_err());
        assert!(parse_github_repo("not-a-url").is_err());
    }

    #[test]
    fn test_state_branch_constant() {
        assert_eq!(STATE_BRANCH, "docket-state");
    }
}
