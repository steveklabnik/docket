use anyhow::Result;
use colored::Colorize;
use std::collections::{HashMap, HashSet};

use crate::change::{Change, Status};
use crate::store::Store;

/// Show dependency tree for a change, displaying what blocks it and what it blocks.
pub fn tree(id: &str, up_only: bool, down_only: bool, max_depth: Option<usize>) -> Result<()> {
    let store = Store::open()?;
    let root = store.get_change(id)?;
    let all_changes = store.list_changes()?;

    // Build lookup maps
    let change_map: HashMap<&str, &Change> = all_changes.iter().map(|c| (c.id(), c)).collect();

    // Build reverse dependency map: id -> what it blocks
    let mut blocks_map: HashMap<&str, Vec<&str>> = HashMap::new();
    for change in &all_changes {
        for blocker_id in change.blocked_by() {
            blocks_map
                .entry(blocker_id.as_str())
                .or_default()
                .push(change.id());
        }
    }

    // Print the root change
    print_change_header(&root);
    println!();

    let show_up = !down_only;
    let show_down = !up_only;

    // Track visited nodes for cycle detection
    let mut visited_up: HashSet<String> = HashSet::new();
    let mut visited_down: HashSet<String> = HashSet::new();

    // Show "Blocked by" section (ancestors)
    if show_up {
        let blocked_by: Vec<&str> = root
            .blocked_by()
            .iter()
            .filter_map(|id| change_map.get(id.as_str()).map(|_| id.as_str()))
            .collect();

        if blocked_by.is_empty() {
            println!("{}", "Blocked by: (none - ready to work)".dimmed());
        } else {
            println!("{}", "Blocked by:".bold());
            for (i, blocker_id) in blocked_by.iter().enumerate() {
                let is_last = i == blocked_by.len() - 1;
                render_ancestor(
                    blocker_id,
                    &change_map,
                    "",
                    is_last,
                    1,
                    max_depth,
                    &mut visited_up,
                );
            }
        }
        println!();
    }

    // Show "Blocks" section (descendants)
    if show_down {
        let blocks: Vec<&str> = blocks_map
            .get(root.id())
            .map(|v| v.as_slice())
            .unwrap_or(&[])
            .to_vec();

        if blocks.is_empty() {
            println!("{}", "Blocks: (nothing)".dimmed());
        } else {
            println!("{}", "Blocks:".bold());
            for (i, blocked_id) in blocks.iter().enumerate() {
                let is_last = i == blocks.len() - 1;
                render_descendant(
                    blocked_id,
                    &change_map,
                    &blocks_map,
                    "",
                    is_last,
                    1,
                    max_depth,
                    &mut visited_down,
                );
            }
        }
    }

    Ok(())
}

/// Print the header for the root change
fn print_change_header(change: &Change) {
    let status_str = format!("{}", change.status());
    let status_colored = color_status(&status_str, change.status());
    let priority_str = format!("{}", change.priority());

    println!(
        "{}: {} [{}, {}]",
        change.id().cyan().bold(),
        change.title().bold(),
        status_colored,
        priority_str
    );
}

/// Render an ancestor (blocker) and its ancestors recursively
fn render_ancestor(
    id: &str,
    change_map: &HashMap<&str, &Change>,
    prefix: &str,
    is_last: bool,
    depth: usize,
    max_depth: Option<usize>,
    visited: &mut HashSet<String>,
) {
    // Cycle detection
    if visited.contains(id) {
        let connector = if is_last { "└── " } else { "├── " };
        println!("{}{}{}", prefix, connector, format!("{} (cycle)", id).red());
        return;
    }
    visited.insert(id.to_string());

    let change = match change_map.get(id) {
        Some(c) => *c,
        None => {
            let connector = if is_last { "└── " } else { "├── " };
            println!(
                "{}{}{}",
                prefix,
                connector,
                format!("{} (not found)", id).red()
            );
            return;
        }
    };

    // Print this node
    let connector = if is_last { "└── " } else { "├── " };
    print_tree_node(change, prefix, connector);

    // Check depth limit
    if let Some(max) = max_depth {
        if depth >= max {
            return;
        }
    }

    // Get this node's blockers (its ancestors)
    let blockers: Vec<&str> = change
        .blocked_by()
        .iter()
        .filter_map(|bid| change_map.get(bid.as_str()).map(|_| bid.as_str()))
        .collect();

    if !blockers.is_empty() {
        let child_prefix = if is_last {
            format!("{}    ", prefix)
        } else {
            format!("{}│   ", prefix)
        };

        for (i, blocker_id) in blockers.iter().enumerate() {
            let is_last_child = i == blockers.len() - 1;
            render_ancestor(
                blocker_id,
                change_map,
                &child_prefix,
                is_last_child,
                depth + 1,
                max_depth,
                visited,
            );
        }
    }
}

/// Render a descendant (blocked by this) and its descendants recursively
#[allow(clippy::too_many_arguments)]
fn render_descendant(
    id: &str,
    change_map: &HashMap<&str, &Change>,
    blocks_map: &HashMap<&str, Vec<&str>>,
    prefix: &str,
    is_last: bool,
    depth: usize,
    max_depth: Option<usize>,
    visited: &mut HashSet<String>,
) {
    // Cycle detection
    if visited.contains(id) {
        let connector = if is_last { "└── " } else { "├── " };
        println!("{}{}{}", prefix, connector, format!("{} (cycle)", id).red());
        return;
    }
    visited.insert(id.to_string());

    let change = match change_map.get(id) {
        Some(c) => *c,
        None => {
            let connector = if is_last { "└── " } else { "├── " };
            println!(
                "{}{}{}",
                prefix,
                connector,
                format!("{} (not found)", id).red()
            );
            return;
        }
    };

    // Print this node
    let connector = if is_last { "└── " } else { "├── " };
    print_tree_node(change, prefix, connector);

    // Check depth limit
    if let Some(max) = max_depth {
        if depth >= max {
            return;
        }
    }

    // Get what this node blocks (its descendants)
    let descendants: Vec<&str> = blocks_map
        .get(id)
        .map(|v| v.as_slice())
        .unwrap_or(&[])
        .to_vec();

    if !descendants.is_empty() {
        let child_prefix = if is_last {
            format!("{}    ", prefix)
        } else {
            format!("{}│   ", prefix)
        };

        for (i, desc_id) in descendants.iter().enumerate() {
            let is_last_child = i == descendants.len() - 1;
            render_descendant(
                desc_id,
                change_map,
                blocks_map,
                &child_prefix,
                is_last_child,
                depth + 1,
                max_depth,
                visited,
            );
        }
    }
}

/// Print a tree node with status and title
fn print_tree_node(change: &Change, prefix: &str, connector: &str) {
    let symbol = match change.status() {
        Status::Done => "●".green(),
        Status::InProgress => "◐".yellow(),
        Status::Review => "◐".magenta(),
        Status::Approved => "○".green(),
        Status::Blocked => "○".red(),
        Status::Paused => "○".cyan(),
        Status::Draft => "○".dimmed(),
        Status::NotPlanned => "✕".red(),
    };

    let status_str = format!("{}", change.status());
    let status_colored = color_status(&status_str, change.status());

    println!(
        "{}{}{} {}: {} [{}]",
        prefix,
        connector,
        symbol,
        change.id().cyan(),
        change.title(),
        status_colored
    );
}

/// Color a status string based on status type
fn color_status(status_str: &str, status: &Status) -> colored::ColoredString {
    match status {
        Status::Draft => status_str.dimmed(),
        Status::Approved => status_str.green(),
        Status::InProgress => status_str.yellow(),
        Status::Blocked => status_str.red(),
        Status::Paused => status_str.cyan(),
        Status::Review => status_str.magenta(),
        Status::Done => status_str.blue(),
        Status::NotPlanned => status_str.red(),
    }
}
