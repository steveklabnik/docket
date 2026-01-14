use anyhow::Result;
use clap::{Parser, Subcommand};

use docket::commands;

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
    },

    /// Show details of a bug
    Show {
        /// Bug ID (prefix match supported)
        id: String,
    },

    /// Update a bug's title, body, priority, or status
    Update {
        /// Bug ID (prefix match supported)
        id: String,

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

    /// Mark a bug as done
    Done {
        /// Bug ID (prefix match supported)
        id: String,
    },

    /// Start working on a bug (creates workspace + runs Claude)
    Work {
        /// Bug ID (prefix match supported)
        id: String,

        /// Skip permission prompts in Claude (--dangerously-skip-permissions)
        #[arg(long)]
        skip_permissions: bool,

        /// Automatically run /docket:implement on startup
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

    /// Migrate markdown bugs to JSONL format (one-time migration)
    Migrate,

    /// Clean up a workspace after work is complete
    Cleanup {
        /// Bug ID (prefix match supported). If not provided, cleans up workspaces for done bugs.
        id: Option<String>,
    },
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
        } => commands::list(status.as_deref(), priority.as_deref(), all),
        Commands::Show { id } => commands::show(&id),
        Commands::Update {
            id,
            title,
            body,
            priority,
            status,
        } => commands::update(
            &id,
            title,
            body.as_deref(),
            priority.as_deref(),
            status.as_deref(),
        ),
        Commands::Approve { id } => commands::approve(&id),
        Commands::Done { id } => commands::done(&id),
        Commands::Work {
            id,
            skip_permissions,
            auto,
        } => commands::work(&id, skip_permissions, auto),
        Commands::Log { id, json } => commands::log(&id, json),
        Commands::Migrate => commands::migrate(),
        Commands::Cleanup { id } => commands::cleanup(id.as_deref()),
    }
}
