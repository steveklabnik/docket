use anyhow::{anyhow, Result};
use clap::Parser;

use docket::cli::{Cli, Commands, ListArgs, NewArgs, RecordArgs, ReleaseCommands, UpdateArgs};
use docket::commands;
use docket::workspace;

/// Resolve change ID from either explicit argument or current workspace.
/// Returns an error if neither is available.
fn resolve_change_id(id: Option<String>) -> Result<String> {
    match id {
        Some(id) => Ok(id),
        None => workspace::current_change_id().ok_or_else(|| {
            anyhow!(
                "no change ID provided and not in a workspace.\n\
                 Either provide a change ID or run from a workspace directory (ws-{{id}})."
            )
        }),
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => commands::init(),
        Commands::New(args) => {
            let NewArgs {
                title,
                priority,
                body,
                template,
                changelog,
                version,
                tag,
                parent,
                epic,
                edit,
                release,
            } = *args;
            let interactive = title.is_none();
            // parent takes precedence over epic (epic is legacy alias)
            let parent_id = parent.or(epic);
            commands::new(
                title,
                &priority,
                body.as_deref(),
                template.as_deref(),
                interactive,
                changelog.as_deref(),
                version.as_deref(),
                &tag,
                parent_id.as_deref(),
                edit,
                release.as_deref(),
            )
        }
        Commands::Record(args) => {
            let RecordArgs {
                title,
                body,
                changelog,
                release,
                parent,
                pr,
                commit,
                edit,
            } = *args;
            commands::record(
                &title,
                body.as_deref(),
                changelog.as_deref(),
                release.as_deref(),
                parent.as_deref(),
                pr.as_deref(),
                commit.as_deref(),
                edit,
            )
        }
        Commands::List(args) => {
            let ListArgs {
                status,
                priority,
                all,
                review,
                blocked,
                paused,
                sort,
                reverse,
                interactive,
                version,
                no_version,
                changelog,
                tag,
                blocking,
                depends_on,
                flat,
                release,
                unscheduled,
            } = *args;
            // --review and --blocked flags are shorthand for --status
            let status_filter = if review {
                Some("review".to_string())
            } else if blocked {
                Some("blocked".to_string())
            } else {
                status
            };
            commands::list(
                status_filter.as_deref(),
                priority.as_deref(),
                all,
                paused,
                &sort,
                reverse,
                interactive,
                version.as_deref(),
                no_version,
                changelog.as_deref(),
                tag.as_deref(),
                blocking,
                depends_on.as_deref(),
                flat,
                release.as_deref(),
                unscheduled,
            )
        }
        Commands::Show { id } => {
            let id = resolve_change_id(id)?;
            commands::show(&id)
        }
        Commands::Update(args) => {
            let UpdateArgs {
                id,
                title,
                body,
                priority,
                status,
                changelog,
                version,
                remove_version,
                release,
            } = *args;
            let id = resolve_change_id(id)?;
            commands::update(
                &id,
                title,
                body.as_deref(),
                priority.as_deref(),
                status.as_deref(),
                changelog.as_deref(),
                version.as_deref(),
                remove_version.as_deref(),
                release.as_deref(),
            )
        }
        Commands::Tag { id, tag } => commands::tag(&id, &tag),
        Commands::Untag { id, tag } => commands::untag(&id, &tag),
        Commands::Tags { by_count } => commands::tags(by_count),
        Commands::Depend { id, on } => commands::depend(&id, &on),
        Commands::Undepend { id, on } => commands::undepend(&id, &on),
        Commands::Approve { id } => commands::approve(&id),
        Commands::Block { id, reason, by } => {
            let id = resolve_change_id(id)?;
            commands::block(&id, reason, by)
        }
        Commands::Unblock { id, by } => {
            let id = resolve_change_id(id)?;
            commands::unblock(&id, by)
        }
        Commands::Pause { id, reason } => {
            let id = resolve_change_id(id)?;
            commands::pause(&id, reason)
        }
        Commands::Resume { id } => {
            let id = resolve_change_id(id)?;
            commands::resume(&id)
        }
        Commands::Review { id } => {
            let id = resolve_change_id(id)?;
            commands::review(&id)
        }
        Commands::Reject { id } => {
            let id = resolve_change_id(id)?;
            commands::reject(&id)
        }
        Commands::Done {
            id,
            auto,
            force,
            describe,
            no_describe,
            squash,
            submit,
        } => {
            let id = resolve_change_id(id)?;
            commands::done(&id, auto, force, describe, no_describe, squash, submit)
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
            let id = resolve_change_id(id)?;
            commands::edit(&id)
        }
        Commands::SyncState => commands::sync_state(),
        Commands::Sync {
            id,
            no_push,
            no_fetch,
            no_rebase,
            dry_run,
        } => commands::sync(id.as_deref(), no_push, no_fetch, no_rebase, dry_run),
        Commands::Completions { shell } => commands::completions(&shell),
        Commands::Ready {
            count,
            work,
            release,
        } => commands::ready(count, work, release.as_deref()),
        Commands::Changelog {
            version,
            preview,
            file,
        } => commands::changelog(&version, preview, file.as_deref()),
        Commands::Epic {
            id_or_title,
            priority,
        } => {
            // Try to find an existing epic with this ID
            let store = docket::store::Store::open()?;
            match store.get_change(&id_or_title) {
                Ok(bug) if bug.is_epic() => {
                    // It's an existing epic, show details
                    commands::epic_show(&store, &bug)
                }
                Ok(_bug) => {
                    // Found a bug but it's not an epic
                    Err(anyhow!(
                        "'{}' is not an epic. Use 'docket show {}' to view it.",
                        id_or_title,
                        id_or_title
                    ))
                }
                Err(_) => {
                    // Not found, treat as title for new epic
                    commands::epic_create(&id_or_title, &priority)
                }
            }
        }
        Commands::Scratch { id, content } => commands::scratch(&id, &content),
        Commands::Reparent { id, parent } => commands::reparent(&id, parent.as_deref()),
        Commands::Tree {
            id,
            up,
            down,
            depth,
        } => commands::tree(&id, up, down, depth),
        Commands::Graph {
            id,
            all,
            by_release,
        } => commands::graph(id.as_deref(), all, by_release),
        Commands::Migrate => commands::migrate(),
        Commands::Release(release_cmd) => match release_cmd {
            ReleaseCommands::New {
                version,
                title,
                body,
                target_date,
                edit,
            } => commands::release::new(
                &version,
                title.as_deref(),
                body.as_deref(),
                target_date.as_deref(),
                edit,
            ),
            ReleaseCommands::List { all } => commands::release::list(all),
            ReleaseCommands::Show { version } => commands::release::show(&version),
            ReleaseCommands::Edit { version } => commands::release::edit(&version),
            ReleaseCommands::Activate { version } => commands::release::activate(&version),
            ReleaseCommands::Freeze { version } => commands::release::freeze(&version),
            ReleaseCommands::Ship { version } => commands::release::ship(&version),
            ReleaseCommands::Cancel { version } => commands::release::cancel(&version),
            ReleaseCommands::Schedule { change_id, version } => {
                commands::release::schedule(&change_id, &version)
            }
        },
    }
}
