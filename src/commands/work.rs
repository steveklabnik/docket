use anyhow::{anyhow, Context, Result};
use colored::Colorize;
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
        Status::NotPlanned => {
            return Err(anyhow!("bug {} was closed as not-planned", bug.id()));
        }
    }

    let bug_id = bug.id().to_string();
    let workspace_name = format!("ws-{}", bug_id);

    // Check if jj is available
    let jj_check = Command::new("jj").arg("--version").output();
    if jj_check.is_err() {
        return Err(anyhow!(
            "jj is not installed or not in PATH.\n\
             Install jj from https://martinvonz.github.io/jj/latest/install-and-setup/\n\
             Then run 'jj git init' to initialize a jj repository."
        ));
    }

    // Get repo root
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
            return Err(anyhow!(
                "failed to create jj workspace at '{}'.\n\
                 Check that the directory doesn't already exist and you have write permissions.",
                workspace_path
            ));
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

    // Spawn claude and wait for it to finish (instead of exec) so we can
    // set a descriptive commit message afterward
    let bug_title = bug.title().to_string();
    let status = Command::new("claude")
        .current_dir(&workspace_path)
        .args(&claude_args)
        .env("DOCKET_BUG", &bug_id)
        .status()
        .context("failed to launch claude")?;

    // After Claude exits, run cargo fmt to ensure code is formatted
    println!();
    println!("{} Claude exited, running cargo fmt...", "→".blue());

    let fmt_result = Command::new("cargo")
        .args(["fmt"])
        .status()
        .context("failed to run cargo fmt")?;

    if !fmt_result.success() {
        eprintln!("{} Warning: cargo fmt failed", "!".yellow());
    }

    // Set a descriptive commit message so the workspace is identifiable from trunk
    println!("{} Updating commit message...", "→".blue());

    let wip_message = format!("wip: {} - {}", bug_id, bug_title);
    let describe_result = Command::new("jj")
        .args(["describe", "-m", &wip_message])
        .output()
        .context("failed to run jj describe")?;

    if !describe_result.status.success() {
        eprintln!("{} Warning: failed to set commit message", "!".yellow());
    } else {
        println!("{} Set commit message: {}", "✓".green(), wip_message);
    }

    if !status.success() {
        return Err(anyhow!("claude exited with non-zero status"));
    }

    Ok(())
}
