use anyhow::{Context, Result};
use colored::Colorize;
use dialoguer::console::{Key, Term};
use std::cmp::Ordering;

use crate::bug::{Bug, ChangelogType, Priority, SortBy, Status};
use crate::commands::{approve, show, work};
use crate::store::Store;

#[allow(clippy::too_many_arguments)]
pub fn list(
    status_filter: Option<&str>,
    priority_filter: Option<&str>,
    show_all: bool,
    sort_by: &str,
    reverse: bool,
    interactive: bool,
    version_filter: Option<&str>,
    no_version: bool,
    changelog_filter: Option<&str>,
) -> Result<()> {
    // Parse sort field early to catch invalid input
    let sort_by: SortBy = sort_by.parse().context("invalid sort field")?;

    let store = Store::open()?;
    let bugs = store.list_bugs()?;

    if bugs.is_empty() {
        println!("{}", "No bugs found.".dimmed());
        return Ok(());
    }

    // Parse filters
    let status_filter: Option<Status> = status_filter.and_then(|s| s.parse().ok());
    let priority_filter: Option<Priority> = priority_filter.and_then(|p| p.parse().ok());
    let changelog_filter: Option<ChangelogType> = changelog_filter.and_then(|c| c.parse().ok());

    // Filter bugs
    let mut filtered: Vec<_> = bugs
        .into_iter()
        .filter(|bug| {
            // By default, hide terminal states (done, not-planned) unless --all is specified
            if !show_all && matches!(bug.status(), Status::Done | Status::NotPlanned) {
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
        "{:6} {:12} {:8} {:10} {}",
        "ID".bold(),
        "STATUS".bold(),
        "PRIORITY".bold(),
        "WORKSPACE".bold(),
        "TITLE".bold()
    );
    println!("{}", "-".repeat(72).dimmed());

    // Print bugs
    for bug in bugs {
        print_bug_row(store, bug, false);
    }
}

fn print_bug_row(store: &Store, bug: &Bug, selected: bool) {
    let status_str = format!("{}", bug.status());
    let status_colored = match bug.status() {
        Status::Draft => status_str.dimmed(),
        Status::Approved => status_str.green(),
        Status::InProgress => status_str.yellow(),
        Status::Done => status_str.blue(),
        Status::NotPlanned => status_str.red(),
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

    let selector = if selected { ">" } else { " " };

    if selected {
        println!(
            "{} {:6} {:12} {:8} {:10} {}",
            selector.green().bold(),
            bug.id().cyan().bold(),
            status_colored.bold(),
            priority_colored.bold(),
            workspace_str.bold(),
            bug.title().bold()
        );
    } else {
        println!(
            "{} {:6} {:12} {:8} {:10} {}",
            selector,
            bug.id().cyan(),
            status_colored,
            priority_colored,
            workspace_str,
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
        "  {:6} {:12} {:8} {:10} {}",
        "ID".bold(),
        "STATUS".bold(),
        "PRIORITY".bold(),
        "WORKSPACE".bold(),
        "TITLE".bold()
    );
    println!("{}", "-".repeat(74).dimmed());

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
/// Secondary sort is always by created date (oldest first) within the same primary field.
fn compare_bugs(a: &Bug, b: &Bug, sort_by: SortBy) -> Ordering {
    let primary = match sort_by {
        SortBy::Priority => a.priority().cmp(b.priority()),
        SortBy::Created => a.created().cmp(&b.created()),
        SortBy::Status => a.status().cmp(b.status()),
    };

    // Secondary sort: oldest first (FIFO within same primary)
    if primary == Ordering::Equal {
        a.created().cmp(&b.created())
    } else {
        primary
    }
}
