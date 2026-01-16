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

        /// Template to use for bug body (e.g., default, feature, bugfix, chore, spike)
        #[arg(long)]
        template: Option<String>,

        /// Changelog type (feature, fix, change, deprecated, removed, security, internal)
        #[arg(short, long)]
        changelog: Option<String>,

        /// Version to assign to this bug (can be added multiple times for backports)
        #[arg(short, long)]
        version: Option<String>,

        /// Tags to assign to this bug (can be used multiple times)
        #[arg(long)]
        tag: Vec<String>,

        /// Create as a child step of an epic
        #[arg(short, long, add = ArgValueCompleter::new(complete_bug_id))]
        epic: Option<String>,
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

        /// Show only bugs in review (shorthand for --status review)
        #[arg(long)]
        review: bool,

        /// Show only blocked bugs (shorthand for --status blocked)
        #[arg(long)]
        blocked: bool,

        /// Show paused bugs (hidden by default like done bugs)
        #[arg(long)]
        paused: bool,

        /// Sort by field (priority, created, status)
        #[arg(long, default_value = "priority")]
        sort: String,

        /// Reverse the sort order
        #[arg(short, long)]
        reverse: bool,

        /// Interactive mode for selecting and acting on bugs
        #[arg(short, long)]
        interactive: bool,

        /// Filter by version
        #[arg(short, long)]
        version: Option<String>,

        /// Show only bugs without a version (unreleased)
        #[arg(long)]
        no_version: bool,

        /// Filter by changelog type
        #[arg(short, long)]
        changelog: Option<String>,

        /// Filter by tag
        #[arg(long)]
        tag: Option<String>,

        /// Show only bugs that block other bugs
        #[arg(long)]
        blocking: bool,

        /// Show only bugs blocked by a specific bug ID
        #[arg(long, value_name = "ID", add = ArgValueCompleter::new(complete_bug_id))]
        depends_on: Option<String>,
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

        /// Changelog type (feature, fix, change, deprecated, removed, security, internal)
        #[arg(short, long)]
        changelog: Option<String>,

        /// Add a version to this bug (can be used multiple times for backports)
        #[arg(short, long)]
        version: Option<String>,

        /// Remove a version from this bug
        #[arg(long)]
        remove_version: Option<String>,
    },

    /// Add a tag to a bug
    Tag {
        /// Bug ID (prefix match supported)
        #[arg(add = ArgValueCompleter::new(complete_bug_id))]
        id: String,

        /// Tag to add
        tag: String,
    },

    /// Remove a tag from a bug
    Untag {
        /// Bug ID (prefix match supported)
        #[arg(add = ArgValueCompleter::new(complete_bug_id))]
        id: String,

        /// Tag to remove
        tag: String,
    },

    /// Mark a bug as approved for work
    Approve {
        /// Bug ID (prefix match supported)
        #[arg(add = ArgValueCompleter::new(complete_bug_id))]
        id: String,
    },

    /// Mark a bug as blocked (on external dependency or another bug)
    Block {
        /// Bug ID (prefix match supported). If not provided, uses current workspace bug.
        #[arg(add = ArgValueCompleter::new(complete_bug_id))]
        id: Option<String>,

        /// Reason for blocking (e.g., "waiting on API access") - for external blocks
        #[arg(short, long)]
        reason: Option<String>,

        /// Bug ID that blocks this bug (inter-bug dependency)
        #[arg(long, add = ArgValueCompleter::new(complete_bug_id))]
        by: Option<String>,
    },

    /// Unblock a bug (from external dependency or another bug)
    Unblock {
        /// Bug ID (prefix match supported). If not provided, uses current workspace bug.
        #[arg(add = ArgValueCompleter::new(complete_bug_id))]
        id: Option<String>,

        /// Bug ID to remove as a blocker (inter-bug dependency)
        #[arg(long, add = ArgValueCompleter::new(complete_bug_id))]
        by: Option<String>,
    },

    /// Pause a bug (intentionally set work aside)
    Pause {
        /// Bug ID (prefix match supported). If not provided, uses current workspace bug.
        #[arg(add = ArgValueCompleter::new(complete_bug_id))]
        id: Option<String>,

        /// Reason for pausing (e.g., "switching to higher priority work")
        #[arg(short, long)]
        reason: Option<String>,
    },

    /// Resume work on a paused bug
    Resume {
        /// Bug ID (prefix match supported). If not provided, uses current workspace bug.
        #[arg(add = ArgValueCompleter::new(complete_bug_id))]
        id: Option<String>,
    },

    /// Submit a bug for code review (uses current workspace bug if no ID provided)
    Review {
        /// Bug ID (prefix match supported). If not provided, uses current workspace bug.
        #[arg(add = ArgValueCompleter::new(complete_bug_id))]
        id: Option<String>,
    },

    /// Reject a bug from review back to in progress
    Reject {
        /// Bug ID (prefix match supported). If not provided, uses current workspace bug.
        #[arg(add = ArgValueCompleter::new(complete_bug_id))]
        id: Option<String>,
    },

    /// Mark a bug as done (uses current workspace bug if no ID provided).
    ///
    /// By default, only marks the bug status as Done. Use --describe, --squash,
    /// and --submit for additional operations like generating commit messages,
    /// squashing commits, or creating PRs.
    Done {
        /// Bug ID (prefix match supported). If not provided, uses current workspace bug.
        #[arg(add = ArgValueCompleter::new(complete_bug_id))]
        id: Option<String>,

        /// Only mark done if all acceptance criteria checkboxes are checked
        #[arg(long)]
        auto: bool,

        /// Force marking done even if unchecked criteria remain (only with --auto)
        #[arg(long)]
        force: bool,

        /// Generate commit message via Claude and update jj describe
        #[arg(long)]
        describe: bool,

        /// Squash workspace commits before completing
        #[arg(long)]
        squash: bool,

        /// Create a PR using GitHub CLI (gh)
        #[arg(long)]
        submit: bool,
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

    /// Show highest priority approved bugs ready for work
    Ready {
        /// Number of bugs to show
        #[arg(short = 'n', long, default_value = "1")]
        count: usize,

        /// Start work on the top bug immediately
        #[arg(long)]
        work: bool,
    },

    /// Generate changelog for a version
    Changelog {
        /// Version to generate changelog for (e.g., "1.2.0")
        version: String,

        /// Preview the changelog without writing to file
        #[arg(long)]
        preview: bool,

        /// Custom file to write to (default: CHANGELOG.md)
        #[arg(short, long)]
        file: Option<String>,
    },

    /// Create an epic or show epic details
    ///
    /// If the argument matches an existing epic ID, shows the epic detail with step progress.
    /// Otherwise, creates a new epic with the argument as the title.
    Epic {
        /// Epic ID to show, or title for new epic
        #[arg(add = ArgValueCompleter::new(complete_bug_id))]
        id_or_title: String,

        /// Priority level (only used when creating)
        #[arg(short, long, default_value = "medium")]
        priority: String,
    },
}
