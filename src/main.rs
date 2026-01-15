use anyhow::{anyhow, Result};
use clap::Parser;

use docket::cli::{Cli, Commands};
use docket::commands;
use docket::workspace;

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
            changelog,
            version,
            tag,
        } => {
            let interactive = title.is_none();
            commands::new(
                title,
                &priority,
                body.as_deref(),
                interactive,
                changelog.as_deref(),
                version.as_deref(),
                &tag,
            )
        }
        Commands::List {
            status,
            priority,
            all,
            review,
            sort,
            reverse,
            interactive,
            version,
            no_version,
            changelog,
            tag,
        } => {
            // --review flag is shorthand for --status review
            let status_filter = if review {
                Some("review".to_string())
            } else {
                status
            };
            commands::list(
                status_filter.as_deref(),
                priority.as_deref(),
                all,
                &sort,
                reverse,
                interactive,
                version.as_deref(),
                no_version,
                changelog.as_deref(),
                tag.as_deref(),
            )
        }
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
            changelog,
            version,
            remove_version,
        } => {
            let id = resolve_bug_id(id)?;
            commands::update(
                &id,
                title,
                body.as_deref(),
                priority.as_deref(),
                status.as_deref(),
                changelog.as_deref(),
                version.as_deref(),
                remove_version.as_deref(),
            )
        }
        Commands::Tag { id, tag } => commands::tag(&id, &tag),
        Commands::Untag { id, tag } => commands::untag(&id, &tag),
        Commands::Approve { id } => commands::approve(&id),
        Commands::Review { id } => {
            let id = resolve_bug_id(id)?;
            commands::review(&id)
        }
        Commands::Reject { id } => {
            let id = resolve_bug_id(id)?;
            commands::reject(&id)
        }
        Commands::Done { id, auto, force } => {
            let id = resolve_bug_id(id)?;
            commands::done(&id, auto, force)
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
        Commands::Sync {
            id,
            no_push,
            no_fetch,
            no_rebase,
            dry_run,
        } => commands::sync(id.as_deref(), no_push, no_fetch, no_rebase, dry_run),
        Commands::Completions { shell } => commands::completions(&shell),
        Commands::Ready { count, work } => commands::ready(count, work),
        Commands::Changelog {
            version,
            preview,
            file,
        } => commands::changelog(&version, preview, file.as_deref()),
    }
}
