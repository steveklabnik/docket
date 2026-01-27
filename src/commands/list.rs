use anyhow::{Context, Result};
use colored::Colorize;
use dialoguer::console::{Key, Term};
use std::cmp::Ordering;

use crate::change::{Change, ChangelogType, Priority, SortBy, Status};
use crate::commands::{approve, show, work};
use crate::release::UNSCHEDULED_RELEASE;
use crate::store::Store;

/// Format a release version for display (12 chars max)
fn format_release(release: &str) -> String {
    if release == UNSCHEDULED_RELEASE {
        "-".to_string()
    } else if release.len() > 12 {
        format!("{}...", &release[..9])
    } else {
        release.to_string()
    }
}

const ID_COLUMN_WIDTH: usize = 6;
const STATUS_COLUMN_WIDTH: usize = 14;
const PRIORITY_COLUMN_WIDTH: usize = 8;
const RELEASE_COLUMN_WIDTH: usize = 12;
const TAGS_COLUMN_WIDTH: usize = 10;
const TITLE_COLUMN_WIDTH: usize = 20;

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
    flat: bool,
    release_filter: Option<&str>,
    unscheduled_only: bool,
) -> Result<()> {
    // Parse sort field early to catch invalid input
    let sort_by: SortBy = sort_by.parse().context("invalid sort field")?;

    let store = Store::open()?;
    let bugs = store.list_changes()?;

    if bugs.is_empty() {
        println!("{}", "No changes found.".dimmed());
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
            // In tree mode, hide child bugs from the top level (they're shown under their parent)
            // In flat mode, show all bugs at the same level
            if !flat && bug.is_child() {
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

            // Apply --release filter: show only bugs targeting a specific release
            if let Some(release) = release_filter {
                if bug.target_release() != release {
                    return false;
                }
            }

            // Apply --unscheduled filter: show only bugs not assigned to a release
            if unscheduled_only && bug.target_release() != UNSCHEDULED_RELEASE {
                return false;
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
        println!("{}", "No changes match the filters.".dimmed());
        return Ok(());
    }

    // Get all bugs for dependency resolution
    let all_bugs = store.list_changes()?;

    if interactive {
        run_interactive_mode(&store, &filtered, &all_bugs)
    } else if flat {
        print_bug_list(&store, &filtered, &all_bugs);
        Ok(())
    } else {
        print_bug_tree(&store, &filtered, &all_bugs);
        Ok(())
    }
}

fn print_bug_list(store: &Store, bugs: &[Change], all_bugs: &[Change]) {
    // Print header
    println!(
        "  {:ID_COLUMN_WIDTH$} {:STATUS_COLUMN_WIDTH$} {:PRIORITY_COLUMN_WIDTH$} {:RELEASE_COLUMN_WIDTH$} {:TAGS_COLUMN_WIDTH$} {:TITLE_COLUMN_WIDTH$} {}",
        "ID".bold(),
        "STATUS".bold(),
        "PRIORITY".bold(),
        "RELEASE".bold(),
        "WORKSPACE".bold(),
        "TAGS".bold(),
        "TITLE".bold()
    );
    println!("{}", "-".repeat(106).dimmed());

    // Print bugs
    for bug in bugs {
        print_bug_row(store, bug, false, all_bugs, 0);
    }
}

fn print_bug_tree(store: &Store, bugs: &[Change], all_bugs: &[Change]) {
    // Print header
    println!(
        "  {:ID_COLUMN_WIDTH$} {:STATUS_COLUMN_WIDTH$} {:RELEASE_COLUMN_WIDTH$} {:RELEASE_COLUMN_WIDTH$} {:TAGS_COLUMN_WIDTH$} {:TITLE_COLUMN_WIDTH$} {}",
        "ID".bold(),
        "STATUS".bold(), // 'approved [dep]' is 14 char
        "PRIORITY".bold(),
        "RELEASE".bold(),
        "WORKSPACE".bold(),
        "TAGS".bold(),
        "TITLE".bold()
    );
    println!("{}", "-".repeat(106).dimmed());

    // Print top-level bugs and their children recursively
    for bug in bugs {
        print_bug_row(store, bug, false, all_bugs, 0);
        print_children(store, bug.id(), all_bugs, 1);
    }
}

fn print_children(store: &Store, parent_id: &str, all_bugs: &[Change], depth: usize) {
    // Find children of this parent
    let children: Vec<_> = all_bugs
        .iter()
        .filter(|b| b.parent() == Some(parent_id))
        .collect();

    for child in children {
        print_bug_row(store, child, false, all_bugs, depth);
        // Recursively print grandchildren
        print_children(store, child.id(), all_bugs, depth + 1);
    }
}

fn print_bug_row(store: &Store, bug: &Change, selected: bool, all_bugs: &[Change], depth: usize) {
    // Check if bug has unresolved dependencies
    let has_unresolved_deps = has_unresolved_dependencies(bug, all_bugs);

    // Check if this bug has children (making it a parent change)
    let has_children = all_bugs.iter().any(|b| b.parent() == Some(bug.id()));

    // For changes with children, show progress instead of status
    let status_str = if has_children {
        let children: Vec<_> = all_bugs
            .iter()
            .filter(|b| b.parent() == Some(bug.id()))
            .collect();
        let completed = children
            .iter()
            .filter(|b| matches!(b.status(), Status::Done))
            .count();
        let total = children.len();
        if total > 0 {
            format!("[{}/{}]", completed, total)
        } else {
            format!("{}", bug.status())
        }
    } else if has_unresolved_deps {
        // Show dependency indicator along with status
        format!("{} [dep]", bug.status())
    } else {
        format!("{}", bug.status())
    };

    let status_colored = if has_children {
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
    } else if has_unresolved_deps {
        // Highlight bugs with unresolved dependencies in orange/yellow
        status_str.yellow()
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

    // Format release version
    let release_str = format_release(bug.target_release());
    let release_colored = if bug.target_release() == UNSCHEDULED_RELEASE {
        release_str.dimmed()
    } else {
        release_str.cyan()
    };

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

    // Create indentation for tree display
    let indent = "  ".repeat(depth);
    let tree_prefix = if depth > 0 { "└─ " } else { "" };
    let selector = if selected { ">" } else { " " };

    // Adjust title display to account for indentation
    let title_display = format!("{}{}{}", indent, tree_prefix, bug.title());

    if selected {
        println!(
            "{} {:ID_COLUMN_WIDTH$} {:STATUS_COLUMN_WIDTH$} {:PRIORITY_COLUMN_WIDTH$} {:RELEASE_COLUMN_WIDTH$} {:TAGS_COLUMN_WIDTH$} {:TITLE_COLUMN_WIDTH$} {}",
            selector.green().bold(),
            bug.id().cyan().bold(),
            status_colored.bold(),
            priority_colored.bold(),
            release_colored.bold(),
            workspace_str.bold(),
            tags_str.yellow().bold(),
            title_display.bold()
        );
    } else {
        println!(
            "{} {:ID_COLUMN_WIDTH$} {:STATUS_COLUMN_WIDTH$} {:PRIORITY_COLUMN_WIDTH$} {:RELEASE_COLUMN_WIDTH$} {:TAGS_COLUMN_WIDTH$} {:TITLE_COLUMN_WIDTH$} {}",
            selector,
            bug.id().cyan(),
            status_colored,
            priority_colored,
            release_colored,
            workspace_str,
            tags_str.yellow(),
            title_display
        );
    }
}

/// Check if a bug has unresolved dependencies (dependencies that are not Done)
fn has_unresolved_dependencies(bug: &Change, all_bugs: &[Change]) -> bool {
    if !bug.has_dependencies() {
        return false;
    }

    bug.blocked_by().iter().any(|blocker_id| {
        all_bugs
            .iter()
            .find(|b| b.id() == blocker_id)
            .map(|b| !matches!(b.status(), Status::Done))
            .unwrap_or(true) // If blocker not found, consider it unresolved
    })
}

/// Interactive mode action result
enum InteractiveAction {
    Show(String),
    Work(String),
    Approve(String),
    Quit,
    Refresh,
}

fn run_interactive_mode(store: &Store, bugs: &[Change], all_bugs: &[Change]) -> Result<()> {
    let term = Term::stdout();
    let mut selected: usize = 0;
    let bug_count = bugs.len();

    // Initial render
    render_interactive_list(&term, store, bugs, selected, all_bugs)?;

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
                render_interactive_list(&term, store, bugs, selected, all_bugs)?;
            }
            InteractiveAction::Show(id) => {
                // Clear the list and show the bug
                term.clear_screen()?;
                show(&id)?;
                println!();
                println!("{}", "Press any key to return to the list...".dimmed());
                term.read_key()?;
                render_interactive_list(&term, store, bugs, selected, all_bugs)?;
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
                render_interactive_list(&term, store, bugs, selected, all_bugs)?;
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
    bugs: &[Change],
    selected: usize,
    all_bugs: &[Change],
) -> Result<()> {
    term.clear_screen()?;

    // Print header
    println!(
        "  {:ID_COLUMN_WIDTH$} {:STATUS_COLUMN_WIDTH$} {:PRIORITY_COLUMN_WIDTH$} {:RELEASE_COLUMN_WIDTH$} {:TAGS_COLUMN_WIDTH$} {:TITLE_COLUMN_WIDTH$} {}",
        "ID".bold(),
        "STATUS".bold(),
        "PRIORITY".bold(),
        "RELEASE".bold(),
        "WORKSPACE".bold(),
        "TAGS".bold(),
        "TITLE".bold()
    );
    println!("{}", "-".repeat(108).dimmed());

    // Print bugs with selection indicator
    for (i, bug) in bugs.iter().enumerate() {
        print_bug_row(store, bug, i == selected, all_bugs, 0);
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
fn compare_bugs(a: &Change, b: &Change, sort_by: SortBy) -> Ordering {
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
