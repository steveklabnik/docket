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

    /// Mark a bug as approved for work
    Approve {
        /// Bug ID (prefix match supported)
        id: String,
    },

    /// Mark a bug as in-progress
    Start {
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

    /// Link a jj change to a bug
    Link {
        /// Bug ID (prefix match supported)
        bug_id: String,

        /// jj change ID to link
        change_id: String,
    },

    /// Auto-close bugs whose linked changes have been merged to trunk
    Sweep,

    /// Clean up a workspace after work is complete
    Cleanup {
        /// Bug ID (prefix match supported). If not provided, cleans up all merged workspaces.
        id: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => commands::init(),
        Commands::New { title, priority } => {
            let interactive = title.is_none();
            commands::new(title, &priority, interactive)
        }
        Commands::List {
            status,
            priority,
            all,
        } => commands::list(status.as_deref(), priority.as_deref(), all),
        Commands::Show { id } => commands::show(&id),
        Commands::Approve { id } => commands::approve(&id),
        Commands::Start { id } => commands::start(&id),
        Commands::Done { id } => commands::done(&id),
        Commands::Work {
            id,
            skip_permissions,
            auto,
        } => commands::work(&id, skip_permissions, auto),
        Commands::Link { bug_id, change_id } => commands::link(&bug_id, &change_id),
        Commands::Sweep => commands::sweep(),
        Commands::Cleanup { id } => commands::cleanup(id.as_deref()),
    }
}
