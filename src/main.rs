use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};

use docket::commands;
use docket::workspace;

#[derive(Parser)]
#[command(name = "docket")]
#[command(about = "Task tracking for AI-assisted development")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
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
        id: Option<String>,
    },

    /// Update a bug's title, body, priority, or status (uses current workspace bug if no ID provided)
    Update {
        /// Bug ID (prefix match supported). If not provided, uses current workspace bug.
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
        id: String,
    },

    /// Mark a bug as done (uses current workspace bug if no ID provided)
    Done {
        /// Bug ID (prefix match supported). If not provided, uses current workspace bug.
        id: Option<String>,
    },

    /// Start working on a bug (creates workspace + runs Claude)
    Work {
        /// Bug ID (prefix match supported)
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
        id: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Clean up a workspace after work is complete
    Cleanup {
        /// Bug ID (prefix match supported). If not provided, cleans up workspaces for done bugs.
        id: Option<String>,
    },

    /// Print the current workspace's bug ID
    Current,

    /// Edit a bug's body in your editor (uses current workspace bug if no ID provided)
    Edit {
        /// Bug ID (prefix match supported). If not provided, uses current workspace bug.
        id: Option<String>,
    },
}

/// Resolve bug ID from either explicit argument or current workspace.
/// Returns an error if neither is available.
fn resolve_bug_id(id: Option<String>) -> Result<String> {
    match id {
        Some(id) => Ok(id),
        None => workspace::current_bug_id().ok_or_else(|| {
            anyhow!(
                "no bug ID provided and not in a workspace.\n\
                 Either provide a bug ID or run from a workspace directory (ws-{{id}})."
            )
        }),
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => commands::init(),
        Commands::New {
            title,
            priority,
            body,
        } => {
            let interactive = title.is_none();
            commands::new(title, &priority, body.as_deref(), interactive)
        }
        Commands::List {
            status,
            priority,
            all,
            sort,
            reverse,
        } => commands::list(status.as_deref(), priority.as_deref(), all, &sort, reverse),
        Commands::Show { id } => {
            let id = resolve_bug_id(id)?;
            commands::show(&id)
        }
        Commands::Update {
            id,
            title,
            body,
            priority,
            status,
        } => {
            let id = resolve_bug_id(id)?;
            commands::update(
                &id,
                title,
                body.as_deref(),
                priority.as_deref(),
                status.as_deref(),
            )
        }
        Commands::Approve { id } => commands::approve(&id),
        Commands::Done { id } => {
            let id = resolve_bug_id(id)?;
            commands::done(&id)
        }
        Commands::Work {
            id,
            skip_permissions,
            auto,
        } => commands::work(&id, skip_permissions, auto),
        Commands::Log { id, json } => commands::log(&id, json),
        Commands::Cleanup { id } => commands::cleanup(id.as_deref()),
        Commands::Current => commands::current(),
        Commands::Edit { id } => {
            let id = resolve_bug_id(id)?;
            commands::edit(&id)
        }
    }
}
