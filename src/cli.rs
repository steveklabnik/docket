use clap::{Parser, Subcommand};
use clap_complete::engine::{ArgValueCompleter, CompletionCandidate};

use crate::store::Store;

/// Custom completer for bug IDs
fn complete_bug_id(current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    let current = current.to_string_lossy();
    let store = match Store::open() {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let bugs = match store.list_bugs() {
        Ok(b) => b,
        Err(_) => return Vec::new(),
    };

    bugs.into_iter()
        .filter(|bug| bug.id().starts_with(current.as_ref()))
        .map(|bug| {
            let id = bug.id().to_string();
            let title = bug.title().to_string();
            CompletionCandidate::new(id).help(Some(title.into()))
        })
        .collect()
}

#[derive(Parser)]
#[command(name = "docket")]
#[command(about = "Task tracking for AI-assisted development")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize a new docket repository
    Init,

    /// Create a new bug
    New {
        /// Bug title (if not provided, will prompt interactively)
        #[arg(short, long)]
        title: Option<String>,

        /// Priority level
        #[arg(short, long, default_value = "medium")]
        priority: String,

        /// Read body from file (use - for stdin)
        #[arg(short, long)]
        body: Option<String>,
    },

    /// List all bugs
    List {
        /// Filter by status
        #[arg(short, long)]
        status: Option<String>,

        /// Filter by priority
        #[arg(short, long)]
        priority: Option<String>,

        /// Show all bugs including done
        #[arg(short, long)]
        all: bool,

        /// Sort by field (priority, created, status)
        #[arg(long, default_value = "priority")]
        sort: String,

        /// Reverse the sort order
        #[arg(short, long)]
        reverse: bool,
    },

    /// Show details of a bug (uses current workspace bug if no ID provided)
    Show {
        /// Bug ID (prefix match supported). If not provided, uses current workspace bug.
        #[arg(add = ArgValueCompleter::new(complete_bug_id))]
        id: Option<String>,
    },

    /// Update a bug's title, body, priority, or status (uses current workspace bug if no ID provided)
    Update {
        /// Bug ID (prefix match supported). If not provided, uses current workspace bug.
        #[arg(add = ArgValueCompleter::new(complete_bug_id))]
        id: Option<String>,

        /// New title for the bug
        #[arg(short, long)]
        title: Option<String>,

        /// Read body from file (use - for stdin)
        #[arg(short, long)]
        body: Option<String>,

        /// New priority level (low, medium, high)
        #[arg(short, long)]
        priority: Option<String>,

        /// New status (draft, approved, in-progress, done, not-planned)
        #[arg(short, long)]
        status: Option<String>,
    },

    /// Mark a bug as approved for work
    Approve {
        /// Bug ID (prefix match supported)
        #[arg(add = ArgValueCompleter::new(complete_bug_id))]
        id: String,
    },

    /// Mark a bug as done (uses current workspace bug if no ID provided)
    Done {
        /// Bug ID (prefix match supported). If not provided, uses current workspace bug.
        #[arg(add = ArgValueCompleter::new(complete_bug_id))]
        id: Option<String>,
    },

    /// Start working on a bug (creates workspace + runs Claude)
    Work {
        /// Bug ID (prefix match supported)
        #[arg(add = ArgValueCompleter::new(complete_bug_id))]
        id: String,

        /// Skip permission prompts in Claude (--dangerously-skip-permissions)
        #[arg(long)]
        skip_permissions: bool,

        /// Automatically run /docket-implement on startup
        #[arg(long)]
        auto: bool,
    },

    /// Show event history for a bug
    Log {
        /// Bug ID (prefix match supported)
        #[arg(add = ArgValueCompleter::new(complete_bug_id))]
        id: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Clean up a workspace after work is complete
    Cleanup {
        /// Bug ID (prefix match supported). If not provided, cleans up workspaces for done bugs.
        #[arg(add = ArgValueCompleter::new(complete_bug_id))]
        id: Option<String>,
    },

    /// Print the current workspace's bug ID
    Current,

    /// Edit a bug's body in your editor (uses current workspace bug if no ID provided)
    Edit {
        /// Bug ID (prefix match supported). If not provided, uses current workspace bug.
        #[arg(add = ArgValueCompleter::new(complete_bug_id))]
        id: Option<String>,
    },

    /// Sync workspaces with trunk (fetch, rebase, push)
    Sync {
        /// Only sync specific bug ID (prefix match)
        #[arg(short, long, add = ArgValueCompleter::new(complete_bug_id))]
        id: Option<String>,

        /// Skip pushing after rebase
        #[arg(long)]
        no_push: bool,

        /// Skip the initial git fetch
        #[arg(long)]
        no_fetch: bool,

        /// Skip rebasing (just fetch and/or push)
        #[arg(long)]
        no_rebase: bool,

        /// Show what would be done without doing it
        #[arg(long)]
        dry_run: bool,
    },

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for (bash, zsh, fish, powershell, elvish)
        shell: String,
    },
}
