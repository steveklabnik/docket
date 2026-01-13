use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use std::os::unix::process::CommandExt;
use std::process::Command;

use crate::bug::Status;
use crate::config::Config;
use crate::store::Store;

/// Start working on a bug by creating a jj workspace and launching Claude.
///
/// # Claude Flags
///
/// The following flags can be passed to `claude`:
///
/// - `--dangerously-skip-permissions`: Skips all permission prompts in Claude,
///   allowing autonomous file operations. Use with caution.
///
/// - `"/docket:implement"` (positional prompt): Automatically runs the docket:implement
///   skill on startup, so you don't have to type it manually.
///
/// These can be enabled via:
/// - CLI flags: `docket work --skip-permissions --auto <id>`
/// - Config file: `.docket/config.toml` with `[work]` section
///
/// CLI flags take precedence over config file settings.
pub fn work(id: &str, skip_permissions: bool, auto: bool) -> Result<()> {
    let store = Store::open()?;

    // Load config to merge with CLI flags
    let config = Config::load(store.root())?;
    let bug = store.get_bug(id)?;

    // Warn if not approved
    match bug.status() {
        Status::Approved | Status::InProgress => {}
        Status::Draft => {
            eprintln!(
                "{} Bug {} is still in draft status. Consider approving it first.",
                "!".yellow(),
                bug.id().cyan()
            );
        }
        Status::Done => {
            return Err(anyhow!("bug {} is already done", bug.id()));
        }
    }

    let bug_id = bug.id().to_string();
    let workspace_name = format!("ws-{}", bug_id);

    // Check if jj is available
    let jj_check = Command::new("jj").arg("--version").output();
    if jj_check.is_err() {
        return Err(anyhow!("jj is not installed or not in PATH"));
    }

    // Get repo root
    let repo_root = Command::new("jj")
        .args(["workspace", "root"])
        .output()
        .context("failed to get jj workspace root")?;

    if !repo_root.status.success() {
        return Err(anyhow!("not in a jj repository"));
    }

    let repo_root = String::from_utf8_lossy(&repo_root.stdout)
        .trim()
        .to_string();

    let workspace_path = format!("{}/../{}", repo_root, workspace_name);

    // Check if workspace already exists
    let workspace_exists = std::path::Path::new(&workspace_path).exists();

    if !workspace_exists {
        println!(
            "{} Creating jj workspace {}...",
            "→".blue(),
            workspace_name.cyan()
        );

        let status = Command::new("jj")
            .args(["workspace", "add", &workspace_path])
            .status()
            .context("failed to create jj workspace")?;

        if !status.success() {
            return Err(anyhow!("failed to create workspace"));
        }
    } else {
        println!(
            "{} Using existing workspace {}",
            "→".blue(),
            workspace_name.cyan()
        );
    }

    // Change to workspace directory
    std::env::set_current_dir(&workspace_path)
        .context("failed to change to workspace directory")?;

    // Merge CLI flags with config (CLI takes precedence)
    let use_skip_permissions = skip_permissions || config.work.skip_permissions;
    let use_auto = auto || config.work.auto_implement;

    println!(
        "{} Starting work on {} - {}",
        "✓".green(),
        bug.id().cyan(),
        bug.title()
    );
    println!("{} Workspace: {}", "→".blue(), workspace_path);

    // Build claude command with appropriate flags
    let mut claude_args: Vec<&str> = Vec::new();

    if use_skip_permissions {
        claude_args.push("--dangerously-skip-permissions");
        println!(
            "{} Skipping permission prompts (--dangerously-skip-permissions)",
            "→".blue()
        );
    }

    if use_auto {
        // Prompt is a positional argument, not a flag
        claude_args.push("/docket:implement");
        println!("{} Auto-running /docket:implement skill", "→".blue());
    }

    println!("{} Launching Claude Code...", "→".blue());
    println!();

    // Exec claude with environment variable and optional flags
    // Explicitly set current_dir to ensure claude runs in workspace
    let err = Command::new("claude")
        .current_dir(&workspace_path)
        .args(&claude_args)
        .env("DOCKET_BUG", &bug_id)
        .exec();

    // If we get here, exec failed
    Err(anyhow!("failed to exec claude: {}", err))
}
