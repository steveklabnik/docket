use anyhow::{Context, Result};
use colored::Colorize;
use dialoguer::console::{Key, Term};
use std::cmp::Ordering;

use crate::bug::{Bug, ChangelogType, Priority, SortBy, Status};
use crate::commands::{approve, epic, show, work};
use crate::store::Store;

#[allow(clippy::too_many_arguments)]
pub fn list(
    status_filter: Option<&str>,
    priority_filter: Option<&str>,
    show_all: bool,
    show_paused: bool,
    sort_by: &str,
    reverse: bool,
    interactive: bool,
    version_filter: Option<&str>,
    no_version: bool,
    changelog_filter: Option<&str>,
    tag_filter: Option<&str>,
    blocking_filter: bool,
    depends_on_filter: Option<&str>,
) -> Result<()> {
    // Parse sort field early to catch invalid input
    let sort_by: SortBy = sort_by.parse().context("invalid sort field")?;

    let store = Store::open()?;
    let bugs = store.list_bugs()?;

    if bugs.is_empty() {
        println!("{}", "No bugs found.".dimmed());
        return Ok(());
    }

    // Resolve depends_on filter ID if provided
    let depends_on_full_id: Option<String> = depends_on_filter
        .map(|id| store.resolve_id(id))
        .transpose()?;

    // Build a set of bug IDs that block other bugs (for --blocking filter)
    let blocking_bug_ids: std::collections::HashSet<String> = if blocking_filter {
        bugs.iter()
            .flat_map(|bug| bug.blocked_by().iter().cloned())
            .collect()
    } else {
        std::collections::HashSet::new()
    };

    // Parse filters
    let status_filter: Option<Status> = status_filter.and_then(|s| s.parse().ok());
    let priority_filter: Option<Priority> = priority_filter.and_then(|p| p.parse().ok());
    let changelog_filter: Option<ChangelogType> = changelog_filter.and_then(|c| c.parse().ok());

    // Filter bugs
    let mut filtered: Vec<_> = bugs
        .into_iter()
        .filter(|bug| {
            // Hide child bugs from the main list (they're shown under their epic)
            if bug.is_child() {
                return false;
            }

            // By default, hide terminal states (done, not-planned) unless --all is specified
            if !show_all && matches!(bug.status(), Status::Done | Status::NotPlanned) {
                return false;
            }

            // By default, hide paused bugs unless --paused or --all is specified
            if !show_all && !show_paused && matches!(bug.status(), Status::Paused) {
                return false;
            }

            // Apply status filter
            if let Some(ref filter) = status_filter {
                if std::mem::discriminant(bug.status()) != std::mem::discriminant(filter) {
                    return false;
                }
            }

            // Apply priority filter
            if let Some(ref filter) = priority_filter {
                if std::mem::discriminant(bug.priority()) != std::mem::discriminant(filter) {
                    return false;
                }
            }

            // Apply version filter
            if let Some(ver) = version_filter {
                if !bug.has_version(ver) {
                    return false;
                }
            }

            // Apply no-version filter (show only bugs without any versions)
            if no_version && !bug.versions().is_empty() {
                return false;
            }

            // Apply changelog type filter
            if let Some(ref filter) = changelog_filter {
                match bug.changelog_type() {
                    Some(ct) => {
                        if std::mem::discriminant(ct) != std::mem::discriminant(filter) {
                            return false;
                        }
                    }
                    None => return false,
                }
            }

            // Apply tag filter
            if let Some(tag) = tag_filter {
                if !bug.has_tag(tag) {
                    return false;
                }
            }

            // Apply --blocking filter: show only bugs that block other bugs
            if blocking_filter && !blocking_bug_ids.contains(bug.id()) {
                return false;
            }

            // Apply --depends-on filter: show only bugs blocked by a specific bug
            if let Some(ref blocker_id) = depends_on_full_id {
                if !bug.is_blocked_by(blocker_id) {
                    return false;
                }
            }

            true
        })
        .collect();

    // Sort bugs
    filtered.sort_by(|a, b| {
        let ordering = compare_bugs(a, b, sort_by);
        if reverse {
            ordering.reverse()
        } else {
            ordering
        }
    });

    if filtered.is_empty() {
        println!("{}", "No bugs match the filters.".dimmed());
        return Ok(());
    }

    if interactive {
        run_interactive_mode(&store, &filtered)
    } else {
        print_bug_list(&store, &filtered);
        Ok(())
    }
}

fn print_bug_list(store: &Store, bugs: &[Bug]) {
    // Print header
    println!(
        "{:6} {:12} {:8} {:10} {:20} {}",
        "ID".bold(),
        "STATUS".bold(),
        "PRIORITY".bold(),
        "WORKSPACE".bold(),
        "TAGS".bold(),
        "TITLE".bold()
    );
    println!("{}", "-".repeat(92).dimmed());

    // Print bugs
    for bug in bugs {
        print_bug_row(store, bug, false);
    }
}

fn print_bug_row(store: &Store, bug: &Bug, selected: bool) {
    // For epics, show progress instead of status
    let status_str = if bug.is_epic() {
        match epic::epic_progress(store, bug.id()) {
            Ok((completed, total)) if total > 0 => format!("[{}/{}]", completed, total),
            _ => format!("{}", bug.status()),
        }
    } else {
        format!("{}", bug.status())
    };

    let status_colored = if bug.is_epic() {
        // Progress indicator styling
        if status_str.starts_with('[') {
            status_str.magenta()
        } else {
            match bug.status() {
                Status::Draft => status_str.dimmed(),
                Status::Approved => status_str.green(),
                Status::InProgress => status_str.yellow(),
                Status::Blocked => status_str.red().bold(),
                Status::Paused => status_str.cyan(),
                Status::Review => status_str.magenta(),
                Status::Done => status_str.blue(),
                Status::NotPlanned => status_str.red(),
            }
        }
    } else {
        match bug.status() {
            Status::Draft => status_str.dimmed(),
            Status::Approved => status_str.green(),
            Status::InProgress => status_str.yellow(),
            Status::Blocked => status_str.red().bold(),
            Status::Paused => status_str.cyan(),
            Status::Review => status_str.magenta(),
            Status::Done => status_str.blue(),
            Status::NotPlanned => status_str.red(),
        }
    };

    let priority_str = format!("{}", bug.priority());
    let priority_colored = match bug.priority() {
        Priority::Low => priority_str.dimmed(),
        Priority::Medium => priority_str.normal(),
        Priority::High => priority_str.red(),
    };

    let workspace_str = store
        .workspace_name(bug.id())
        .unwrap_or_else(|| "-".to_string());

    // Format tags compactly (sorted, comma-separated, truncated if too long)
    let tags_str = {
        let mut tags: Vec<_> = bug.tags().iter().map(|s| s.as_str()).collect();
        tags.sort();
        let joined = tags.join(",");
        if joined.len() > 18 {
            format!("{}...", &joined[..15])
        } else if joined.is_empty() {
            "-".to_string()
        } else {
            joined
        }
    };

    let selector = if selected { ">" } else { " " };

    if selected {
        println!(
            "{} {:6} {:12} {:8} {:10} {:20} {}",
            selector.green().bold(),
            bug.id().cyan().bold(),
            status_colored.bold(),
            priority_colored.bold(),
            workspace_str.bold(),
            tags_str.yellow().bold(),
            bug.title().bold()
        );
    } else {
        println!(
            "{} {:6} {:12} {:8} {:10} {:20} {}",
            selector,
            bug.id().cyan(),
            status_colored,
            priority_colored,
            workspace_str,
            tags_str.yellow(),
            bug.title()
        );
    }
}

/// Interactive mode action result
enum InteractiveAction {
    Show(String),
    Work(String),
    Approve(String),
    Quit,
    Refresh,
}

fn run_interactive_mode(store: &Store, bugs: &[Bug]) -> Result<()> {
    let term = Term::stdout();
    let mut selected: usize = 0;
    let bug_count = bugs.len();

    // Initial render
    render_interactive_list(&term, store, bugs, selected)?;

    loop {
        let key = term.read_key().context("failed to read key")?;
        let action = match key {
            Key::ArrowUp | Key::Char('k') => {
                selected = selected.saturating_sub(1);
                InteractiveAction::Refresh
            }
            Key::ArrowDown | Key::Char('j') => {
                if selected < bug_count - 1 {
                    selected += 1;
                }
                InteractiveAction::Refresh
            }
            Key::Enter => InteractiveAction::Show(bugs[selected].id().to_string()),
            Key::Char('w') => InteractiveAction::Work(bugs[selected].id().to_string()),
            Key::Char('a') => InteractiveAction::Approve(bugs[selected].id().to_string()),
            Key::Escape | Key::Char('q') => InteractiveAction::Quit,
            _ => InteractiveAction::Refresh,
        };

        match action {
            InteractiveAction::Refresh => {
                render_interactive_list(&term, store, bugs, selected)?;
            }
            InteractiveAction::Show(id) => {
                // Clear the list and show the bug
                term.clear_screen()?;
                show(&id)?;
                println!();
                println!("{}", "Press any key to return to the list...".dimmed());
                term.read_key()?;
                render_interactive_list(&term, store, bugs, selected)?;
            }
            InteractiveAction::Work(id) => {
                // Exit interactive mode and run work command
                term.clear_screen()?;
                return work(&id, false, false);
            }
            InteractiveAction::Approve(id) => {
                // Approve and refresh the display
                term.clear_screen()?;
                if let Err(e) = approve(&id) {
                    println!("{} {}", "!".red(), e);
                    println!();
                    println!("{}", "Press any key to continue...".dimmed());
                    term.read_key()?;
                }
                render_interactive_list(&term, store, bugs, selected)?;
            }
            InteractiveAction::Quit => {
                term.clear_screen()?;
                return Ok(());
            }
        }
    }
}

fn render_interactive_list(
    term: &Term,
    store: &Store,
    bugs: &[Bug],
    selected: usize,
) -> Result<()> {
    term.clear_screen()?;

    // Print header
    println!(
        "  {:6} {:12} {:8} {:10} {:20} {}",
        "ID".bold(),
        "STATUS".bold(),
        "PRIORITY".bold(),
        "WORKSPACE".bold(),
        "TAGS".bold(),
        "TITLE".bold()
    );
    println!("{}", "-".repeat(94).dimmed());

    // Print bugs with selection indicator
    for (i, bug) in bugs.iter().enumerate() {
        print_bug_row(store, bug, i == selected);
    }

    // Print help footer
    println!();
    println!(
        "{}",
        "↑/↓: navigate  Enter: show  w: work  a: approve  q: quit".dimmed()
    );

    Ok(())
}

/// Compare two bugs for sorting.
/// Primary sort groups approved bugs (non-Draft) before unapproved (Draft).
/// Secondary sort is by the specified field.
/// Tertiary sort is by created date (oldest first) within the same group.
fn compare_bugs(a: &Bug, b: &Bug, sort_by: SortBy) -> Ordering {
    // Primary: approved (non-Draft) bugs come before Draft bugs
    let a_approved = !matches!(a.status(), Status::Draft);
    let b_approved = !matches!(b.status(), Status::Draft);

    match (a_approved, b_approved) {
        (true, false) => Ordering::Less, // a is approved, b is draft -> a first
        (false, true) => Ordering::Greater, // a is draft, b is approved -> b first
        _ => {
            // Both in same approval group, use secondary sort
            let secondary = match sort_by {
                SortBy::Priority => a.priority().cmp(b.priority()),
                SortBy::Created => a.created().cmp(&b.created()),
                SortBy::Status => a.status().cmp(b.status()),
            };

            // Tertiary sort: oldest first (FIFO within same secondary)
            if secondary == Ordering::Equal {
                a.created().cmp(&b.created())
            } else {
                secondary
            }
        }
    }
}
