use clap::{Args, Parser, Subcommand};
use clap_complete::engine::{ArgValueCompleter, CompletionCandidate};

use crate::store::Store;

/// Custom completer for change IDs
fn complete_change_id(current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    let current = current.to_string_lossy();
    let store = match Store::open() {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let changes = match store.list_changes() {
        Ok(b) => b,
        Err(_) => return Vec::new(),
    };

    changes
        .into_iter()
        .filter(|change| change.id().starts_with(current.as_ref()))
        .map(|change| {
            let id = change.id().to_string();
            let title = change.title().to_string();
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

/// Arguments for the 'new' command (boxed to reduce enum size)
#[derive(Args)]
pub struct NewArgs {
    /// Change title (if not provided, will prompt interactively)
    #[arg(short, long)]
    pub title: Option<String>,

    /// Priority level
    #[arg(short, long, default_value = "medium")]
    pub priority: String,

    /// Read body from file (use - for stdin)
    #[arg(short, long)]
    pub body: Option<String>,

    /// Template to use for change body (e.g., default, feature, bugfix, chore, spike)
    #[arg(long)]
    pub template: Option<String>,

    /// Changelog type (feature, fix, change, deprecated, removed, security, internal)
    #[arg(short, long)]
    pub changelog: Option<String>,

    /// Version to assign to this change (can be added multiple times for backports)
    /// DEPRECATED: use --release instead
    #[arg(short, long)]
    pub version: Option<String>,

    /// Tags to assign to this change (can be used multiple times)
    #[arg(long)]
    pub tag: Vec<String>,

    /// Create as a child (sub-change) of another change
    #[arg(long, add = ArgValueCompleter::new(complete_change_id))]
    pub parent: Option<String>,

    /// Create as a child step of an epic (legacy, alias for --parent)
    #[arg(short = 'e', long, add = ArgValueCompleter::new(complete_change_id), hide = true)]
    pub epic: Option<String>,

    /// Open editor immediately after creation to edit the body
    #[arg(long)]
    pub edit: bool,

    /// Target release version (defaults to "unscheduled")
    #[arg(short = 'r', long)]
    pub release: Option<String>,
}

/// Arguments for the 'list' command (boxed to reduce enum size)
#[derive(Args)]
pub struct ListArgs {
    /// Filter by status
    #[arg(short, long)]
    pub status: Option<String>,

    /// Filter by priority
    #[arg(short, long)]
    pub priority: Option<String>,

    /// Show all changes including done
    #[arg(short, long)]
    pub all: bool,

    /// Show only changes in review (shorthand for --status review)
    #[arg(long)]
    pub review: bool,

    /// Show only blocked changes (shorthand for --status blocked)
    #[arg(long)]
    pub blocked: bool,

    /// Show paused changes (hidden by default like done changes)
    #[arg(long)]
    pub paused: bool,

    /// Sort by field (priority, created, status)
    #[arg(long, default_value = "priority")]
    pub sort: String,

    /// Reverse the sort order
    #[arg(short, long)]
    pub reverse: bool,

    /// Interactive mode for selecting and acting on changes
    #[arg(short, long)]
    pub interactive: bool,

    /// Filter by version (DEPRECATED: use --release instead)
    #[arg(short, long)]
    pub version: Option<String>,

    /// Show only changes without a version (unreleased)
    /// DEPRECATED: use --unscheduled instead
    #[arg(long)]
    pub no_version: bool,

    /// Filter by changelog type
    #[arg(short, long)]
    pub changelog: Option<String>,

    /// Filter by tag
    #[arg(long)]
    pub tag: Option<String>,

    /// Show only changes that block other changes
    #[arg(long)]
    pub blocking: bool,

    /// Show only changes blocked by a specific change ID
    #[arg(long, value_name = "ID", add = ArgValueCompleter::new(complete_change_id))]
    pub depends_on: Option<String>,

    /// Show as flat list instead of tree structure
    #[arg(long)]
    pub flat: bool,

    /// Filter by target release version
    #[arg(long, value_name = "VERSION")]
    pub release: Option<String>,

    /// Show only unscheduled changes (not assigned to any release)
    #[arg(long)]
    pub unscheduled: bool,
}

/// Arguments for the 'record' command (boxed to reduce enum size)
#[derive(Args)]
pub struct RecordArgs {
    /// Title describing the completed work
    pub title: String,

    /// Read body/description from file (use - for stdin)
    #[arg(short, long)]
    pub body: Option<String>,

    /// Changelog type (feature, fix, change, deprecated, removed, security, internal)
    #[arg(short, long)]
    pub changelog: Option<String>,

    /// Target release version
    #[arg(short = 'r', long)]
    pub release: Option<String>,

    /// Create as a child (sub-change) of another change
    #[arg(long, add = ArgValueCompleter::new(complete_change_id))]
    pub parent: Option<String>,

    /// PR number (stored in body as reference)
    #[arg(long)]
    pub pr: Option<String>,

    /// Commit SHA (stored in body as reference)
    #[arg(long)]
    pub commit: Option<String>,

    /// Open editor to write description
    #[arg(long)]
    pub edit: bool,
}

/// Arguments for the 'update' command (boxed to reduce enum size)
#[derive(Args)]
pub struct UpdateArgs {
    /// Change ID (prefix match supported). If not provided, uses current workspace change.
    #[arg(add = ArgValueCompleter::new(complete_change_id))]
    pub id: Option<String>,

    /// New title for the change
    #[arg(short, long)]
    pub title: Option<String>,

    /// Read body from file (use - for stdin)
    #[arg(short, long)]
    pub body: Option<String>,

    /// New priority level (low, medium, high)
    #[arg(short, long)]
    pub priority: Option<String>,

    /// New status (draft, approved, in-progress, done, not-planned)
    #[arg(short, long)]
    pub status: Option<String>,

    /// Changelog type (feature, fix, change, deprecated, removed, security, internal)
    #[arg(short, long)]
    pub changelog: Option<String>,

    /// Add a version to this change (can be used multiple times for backports)
    /// DEPRECATED: use 'docket release schedule' instead
    #[arg(short, long)]
    pub version: Option<String>,

    /// Remove a version from this change
    /// DEPRECATED: use 'docket release schedule' instead
    #[arg(long)]
    pub remove_version: Option<String>,

    /// Target release version to schedule this change for
    #[arg(short = 'r', long)]
    pub release: Option<String>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize a new docket repository
    Init,

    /// Create a new change
    New(Box<NewArgs>),

    /// Record already-completed work (creates change directly in Done status)
    ///
    /// Use this to capture work that happened before docket tracking was in place,
    /// such as external contributions, ad-hoc fixes, or retrospective documentation.
    Record(Box<RecordArgs>),

    /// List all changes
    List(Box<ListArgs>),

    /// Show details of a change (uses current workspace change if no ID provided)
    Show {
        /// Change ID (prefix match supported). If not provided, uses current workspace change.
        #[arg(add = ArgValueCompleter::new(complete_change_id))]
        id: Option<String>,
    },

    /// Update a change's title, body, priority, or status (uses current workspace change if no ID provided)
    Update(Box<UpdateArgs>),

    /// Add a tag to a change
    Tag {
        /// Change ID (prefix match supported)
        #[arg(add = ArgValueCompleter::new(complete_change_id))]
        id: String,

        /// Tag to add
        tag: String,
    },

    /// Remove a tag from a change
    Untag {
        /// Change ID (prefix match supported)
        #[arg(add = ArgValueCompleter::new(complete_change_id))]
        id: String,

        /// Tag to remove
        tag: String,
    },

    /// List all tags in use with bug counts
    Tags {
        /// Sort by count (descending) instead of alphabetically
        #[arg(long)]
        by_count: bool,
    },

    /// Add a dependency: change becomes blocked by another change
    ///
    /// This creates an inter-change dependency. The change won't show up in "ready to work"
    /// views until all dependencies are marked as done.
    ///
    /// Unlike `block`, this doesn't change the change's status - it just records the
    /// relationship. Use `block` for external blockers that should change status.
    Depend {
        /// Change ID that will depend on another (prefix match supported)
        #[arg(add = ArgValueCompleter::new(complete_change_id))]
        id: String,

        /// Change ID that blocks this change
        #[arg(long, value_name = "ID", add = ArgValueCompleter::new(complete_change_id))]
        on: String,
    },

    /// Remove a dependency between changes
    ///
    /// This removes an inter-change dependency without changing the change's status.
    Undepend {
        /// Change ID to remove dependency from (prefix match supported)
        #[arg(add = ArgValueCompleter::new(complete_change_id))]
        id: String,

        /// Change ID to stop depending on
        #[arg(long, value_name = "ID", add = ArgValueCompleter::new(complete_change_id))]
        on: String,
    },

    /// Mark a change as approved for work
    Approve {
        /// Change ID (prefix match supported)
        #[arg(add = ArgValueCompleter::new(complete_change_id))]
        id: String,
    },

    /// Mark a change as blocked (on external dependency or another change)
    Block {
        /// Change ID (prefix match supported). If not provided, uses current workspace change.
        #[arg(add = ArgValueCompleter::new(complete_change_id))]
        id: Option<String>,

        /// Reason for blocking (e.g., "waiting on API access") - for external blocks
        #[arg(short, long)]
        reason: Option<String>,

        /// Change ID that blocks this change (inter-change dependency)
        #[arg(long, add = ArgValueCompleter::new(complete_change_id))]
        by: Option<String>,
    },

    /// Unblock a change (from external dependency or another change)
    Unblock {
        /// Change ID (prefix match supported). If not provided, uses current workspace change.
        #[arg(add = ArgValueCompleter::new(complete_change_id))]
        id: Option<String>,

        /// Change ID to remove as a blocker (inter-change dependency)
        #[arg(long, add = ArgValueCompleter::new(complete_change_id))]
        by: Option<String>,
    },

    /// Pause a change (intentionally set work aside)
    Pause {
        /// Change ID (prefix match supported). If not provided, uses current workspace change.
        #[arg(add = ArgValueCompleter::new(complete_change_id))]
        id: Option<String>,

        /// Reason for pausing (e.g., "switching to higher priority work")
        #[arg(short, long)]
        reason: Option<String>,
    },

    /// Resume work on a paused change
    Resume {
        /// Change ID (prefix match supported). If not provided, uses current workspace change.
        #[arg(add = ArgValueCompleter::new(complete_change_id))]
        id: Option<String>,
    },

    /// Submit a change for code review (uses current workspace change if no ID provided)
    Review {
        /// Change ID (prefix match supported). If not provided, uses current workspace change.
        #[arg(add = ArgValueCompleter::new(complete_change_id))]
        id: Option<String>,
    },

    /// Reject a change from review back to in progress
    Reject {
        /// Change ID (prefix match supported). If not provided, uses current workspace change.
        #[arg(add = ArgValueCompleter::new(complete_change_id))]
        id: Option<String>,
    },

    /// Mark a change as done (uses current workspace change if no ID provided).
    ///
    /// By default, only marks the change status as Done. Use --describe, --squash,
    /// and --submit for additional operations like generating commit messages,
    /// squashing commits, or creating PRs.
    Done {
        /// Change ID (prefix match supported). If not provided, uses current workspace change.
        #[arg(add = ArgValueCompleter::new(complete_change_id))]
        id: Option<String>,

        /// Only mark done if all acceptance criteria checkboxes are checked
        #[arg(long)]
        auto: bool,

        /// Force marking done even if unchecked criteria remain (only with --auto)
        #[arg(long)]
        force: bool,

        /// Generate commit message via Claude and update jj describe
        /// (default when workspace exists, use --no-describe to skip)
        #[arg(long)]
        describe: bool,

        /// Skip generating commit message (overrides default --describe behavior)
        #[arg(long)]
        no_describe: bool,

        /// Squash workspace commits before completing
        #[arg(long)]
        squash: bool,

        /// Create a PR using GitHub CLI (gh)
        #[arg(long)]
        submit: bool,
    },

    /// Start working on a change (creates workspace + runs Claude)
    Work {
        /// Change ID (prefix match supported)
        #[arg(add = ArgValueCompleter::new(complete_change_id))]
        id: String,

        /// Skip permission prompts in Claude (--dangerously-skip-permissions)
        #[arg(long)]
        skip_permissions: bool,

        /// Automatically run /docket-implement on startup
        #[arg(long)]
        auto: bool,
    },

    /// Show event history for a change
    Log {
        /// Change ID (prefix match supported)
        #[arg(add = ArgValueCompleter::new(complete_change_id))]
        id: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Clean up a workspace after work is complete
    Cleanup {
        /// Change ID (prefix match supported). If not provided, cleans up workspaces for done changes.
        #[arg(add = ArgValueCompleter::new(complete_change_id))]
        id: Option<String>,
    },

    /// Print the current workspace's change ID
    Current,

    /// Edit a change's body in your editor (uses current workspace change if no ID provided)
    Edit {
        /// Change ID (prefix match supported). If not provided, uses current workspace change.
        #[arg(add = ArgValueCompleter::new(complete_change_id))]
        id: Option<String>,
    },

    /// Sync docket state branch with remote (fetch + push)
    ///
    /// This command synchronizes the docket-state bookmark with the remote repository.
    /// It first fetches any remote changes, then pushes local state to the remote.
    /// Use this to share docket state across machines or with team members.
    SyncState,

    /// Sync workspaces with trunk (fetch, rebase, push)
    Sync {
        /// Only sync specific change ID (prefix match)
        #[arg(short, long, add = ArgValueCompleter::new(complete_change_id))]
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

    /// Show highest priority approved changes ready for work
    Ready {
        /// Number of changes to show
        #[arg(short = 'n', long, default_value = "1")]
        count: usize,

        /// Start work on the top change immediately
        #[arg(long)]
        work: bool,

        /// Filter to changes targeting a specific release
        #[arg(long)]
        release: Option<String>,
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
        #[arg(add = ArgValueCompleter::new(complete_change_id))]
        id_or_title: String,

        /// Priority level (only used when creating)
        #[arg(short, long, default_value = "medium")]
        priority: String,
    },

    /// Append a note to a change's scratchpad
    ///
    /// The scratchpad is a persistent, append-only notes section for each change.
    /// Use it to record working notes, decisions, or progress during implementation.
    Scratch {
        /// Change ID (prefix match supported)
        #[arg(add = ArgValueCompleter::new(complete_change_id))]
        id: String,

        /// Note content to append to the scratchpad
        content: String,
    },

    /// Move a change under a different parent
    ///
    /// Use this to reorganize your change hierarchy. Omit the parent argument
    /// to make a change top-level (remove its parent).
    Reparent {
        /// Change ID to reparent (prefix match supported)
        #[arg(add = ArgValueCompleter::new(complete_change_id))]
        id: String,

        /// New parent change ID (omit to make top-level)
        #[arg(add = ArgValueCompleter::new(complete_change_id))]
        parent: Option<String>,
    },

    /// Show dependency tree for a change
    ///
    /// Displays what blocks a change (blocked by) and what it blocks.
    /// Useful for understanding the dependency chain around a specific change.
    Tree {
        /// Change ID (prefix match supported)
        #[arg(add = ArgValueCompleter::new(complete_change_id))]
        id: String,

        /// Show only ancestors (what blocks this change)
        #[arg(long)]
        up: bool,

        /// Show only descendants (what this change blocks)
        #[arg(long)]
        down: bool,

        /// Limit tree depth
        #[arg(long)]
        depth: Option<usize>,
    },

    /// Visualize the change graph (DAG)
    ///
    /// Shows the hierarchy of changes and their dependencies as an ASCII graph.
    /// Use with a change ID to show only the subgraph rooted at that change.
    /// By default, hides completed (done/not-planned) changes.
    Graph {
        /// Change ID to show subgraph from (omit to show full graph)
        #[arg(add = ArgValueCompleter::new(complete_change_id))]
        id: Option<String>,

        /// Show all changes including done and not-planned
        #[arg(short, long)]
        all: bool,

        /// Group changes by target release instead of parent hierarchy
        #[arg(long)]
        by_release: bool,
    },

    /// Manage releases (milestones)
    #[command(subcommand)]
    Release(ReleaseCommands),

    /// Migrate from filesystem storage to state branch storage
    ///
    /// This command migrates an existing `.docket/` directory in the working tree
    /// to the state branch model where docket data is stored on an orphan branch.
    /// The migration is idempotent - running it multiple times is safe.
    ///
    /// After migration, the `.docket/` directory is removed from the working tree
    /// and all data is stored on the `docket-state` bookmark.
    Migrate,
}

#[derive(Subcommand)]
pub enum ReleaseCommands {
    /// Create a new release
    New {
        /// Semver version string (e.g., "1.0.0", "0.3.0-beta.1")
        version: String,

        /// Human-readable title (e.g., "Performance Release")
        #[arg(short, long)]
        title: Option<String>,

        /// Read description from file (use - for stdin)
        #[arg(short, long)]
        body: Option<String>,

        /// Target release date (YYYY-MM-DD)
        #[arg(long)]
        target_date: Option<String>,

        /// Open editor immediately to write description
        #[arg(long)]
        edit: bool,
    },

    /// List releases
    List {
        /// Show all releases including released and cancelled
        #[arg(short, long)]
        all: bool,
    },

    /// Show release details and progress
    Show {
        /// Release version
        version: String,
    },

    /// Edit release title and description
    Edit {
        /// Release version
        version: String,
    },

    /// Activate a release (Planning -> Active)
    Activate {
        /// Release version
        version: String,
    },

    /// Freeze a release (Active -> Frozen)
    Freeze {
        /// Release version
        version: String,
    },

    /// Ship a release (Active/Frozen -> Released)
    Ship {
        /// Release version
        version: String,
    },

    /// Cancel a release
    Cancel {
        /// Release version
        version: String,
    },

    /// Schedule a change for a release
    Schedule {
        /// Change ID (prefix match supported)
        #[arg(add = ArgValueCompleter::new(complete_change_id))]
        change_id: String,

        /// Target release version
        version: String,
    },
}
