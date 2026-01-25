use anyhow::Result;
use colored::Colorize;
use std::collections::{HashMap, HashSet};

use crate::change::{Change, Status};
use crate::store::Store;

/// A forest represents a connected component of changes in the graph.
struct Forest {
    ids: HashSet<String>,
    name: String,
    root_id: String,
    /// Priority for sorting: lower = more actionable (in-progress first, then approved, then draft)
    priority: u8,
}

/// Find forests based on parent/child hierarchy only.
/// Each top-level change (no parent) and its descendants form a forest.
/// Dependencies (blocked_by) are cross-cutting and don't affect forest membership.
fn identify_forests(bugs: &[Change], included: &HashSet<String>) -> Vec<(String, HashSet<String>)> {
    let bug_map: HashMap<&str, &Change> = bugs.iter().map(|b| (b.id(), b)).collect();

    // Find the root ancestor of a change by following parent links
    fn find_hierarchy_root<'a>(
        id: &'a str,
        bug_map: &'a HashMap<&str, &Change>,
        included: &HashSet<String>,
    ) -> &'a str {
        if let Some(bug) = bug_map.get(id) {
            if let Some(parent_id) = bug.parent() {
                if included.contains(parent_id) {
                    return find_hierarchy_root(parent_id, bug_map, included);
                }
            }
        }
        id
    }

    // Group nodes by their hierarchy root
    let mut forests_map: HashMap<String, HashSet<String>> = HashMap::new();
    for id in included {
        let root = find_hierarchy_root(id, &bug_map, included);
        forests_map
            .entry(root.to_string())
            .or_default()
            .insert(id.clone());
    }

    forests_map.into_iter().collect()
}

/// Build forests with names and priorities from connected components.
fn build_forests(bugs: &[Change], included: &HashSet<String>) -> Vec<Forest> {
    let components = identify_forests(bugs, included);
    let bug_map: HashMap<&str, &Change> = bugs.iter().map(|b| (b.id(), b)).collect();

    let mut forests: Vec<Forest> = components
        .into_iter()
        .map(|(root_id, ids)| {
            // Derive name from root title
            let name = bug_map
                .get(root_id.as_str())
                .map(|b| b.title().to_string())
                .unwrap_or_else(|| "Changes".to_string());

            // Calculate priority based on most actionable status in the forest
            let priority = calculate_forest_priority(&ids, &bug_map);

            Forest {
                ids,
                name,
                root_id,
                priority,
            }
        })
        .collect();

    // Sort forests by priority (most actionable first), then by name for stability
    forests.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.name.cmp(&b.name))
    });

    forests
}

/// Calculate priority for forest ordering (lower = more actionable).
fn calculate_forest_priority(ids: &HashSet<String>, bug_map: &HashMap<&str, &Change>) -> u8 {
    let mut has_in_progress = false;
    let mut has_approved = false;

    for id in ids {
        if let Some(bug) = bug_map.get(id.as_str()) {
            match bug.status() {
                Status::InProgress | Status::Review => has_in_progress = true,
                Status::Approved => has_approved = true,
                _ => {}
            }
        }
    }

    if has_in_progress {
        0 // Highest priority
    } else if has_approved {
        1
    } else {
        2 // All draft/blocked/etc
    }
}

/// Display the DAG of changes showing hierarchy and dependencies.
/// By default, hides completed (done/not-planned) changes unless `show_all` is true.
pub fn graph(id: Option<&str>, show_all: bool) -> Result<()> {
    let store = Store::open()?;
    let all_bugs = store.list_changes()?;

    if all_bugs.is_empty() {
        println!("{}", "No changes found.".dimmed());
        return Ok(());
    }

    // If an ID is provided, show the subgraph centered on that change
    let mut included_ids: HashSet<String> = if let Some(ref_id) = id {
        let bug = store.get_change(ref_id)?;
        let mut ids = HashSet::new();
        collect_related(bug.id(), &all_bugs, &mut ids);
        ids
    } else {
        all_bugs.iter().map(|b| b.id().to_string()).collect()
    };

    // By default, hide terminal states (done, not-planned) unless --all is specified
    if !show_all {
        included_ids.retain(|id| {
            all_bugs
                .iter()
                .find(|b| b.id() == id)
                .map(|b| !matches!(b.status(), Status::Done | Status::NotPlanned))
                .unwrap_or(true)
        });
    }

    if included_ids.is_empty() {
        println!(
            "{}",
            "No open changes found. Use --all to show completed changes.".dimmed()
        );
        return Ok(());
    }

    // Build bug lookup map
    let bug_map: HashMap<&str, &Change> = all_bugs.iter().map(|b| (b.id(), b)).collect();

    // Build children map for tree traversal
    let mut children_of: HashMap<&str, Vec<&str>> = HashMap::new();
    for bug in &all_bugs {
        if !included_ids.contains(bug.id()) {
            continue;
        }
        if let Some(parent_id) = bug.parent() {
            if included_ids.contains(parent_id) {
                children_of.entry(parent_id).or_default().push(bug.id());
            }
        }
    }

    // Calculate subtree depths for sorting (longer chains first)
    let depths = calculate_subtree_depths(&bug_map, &children_of, &included_ids);

    // Build forests (connected components)
    let forests = build_forests(&all_bugs, &included_ids);

    for (i, forest) in forests.iter().enumerate() {
        // Print forest header
        println!("{}", format!("--- {} ---", forest.name).bold());

        // Render the tree starting from the root
        render_tree(
            &forest.root_id,
            &bug_map,
            &children_of,
            &depths,
            &forest.ids,
            "",
            true, // is_root
            true, // is_last (doesn't matter for root)
        );

        if i < forests.len() - 1 {
            println!(); // Blank line between forests
        }
    }

    Ok(())
}

/// Calculate the maximum depth of subtree rooted at each node.
/// Used for sorting siblings so longer chains appear first.
fn calculate_subtree_depths(
    _bug_map: &HashMap<&str, &Change>,
    children_of: &HashMap<&str, Vec<&str>>,
    included: &HashSet<String>,
) -> HashMap<String, usize> {
    let mut depths: HashMap<String, usize> = HashMap::new();

    fn calc_depth(
        id: &str,
        children_of: &HashMap<&str, Vec<&str>>,
        depths: &mut HashMap<String, usize>,
    ) -> usize {
        if let Some(&cached) = depths.get(id) {
            return cached;
        }

        let max_child_depth = children_of
            .get(id)
            .map(|children| {
                children
                    .iter()
                    .map(|c| calc_depth(c, children_of, depths))
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or(0);

        let depth = 1 + max_child_depth;
        depths.insert(id.to_string(), depth);
        depth
    }

    for id in included {
        calc_depth(id, children_of, &mut depths);
    }

    depths
}

/// Render a tree node and its children recursively.
#[allow(clippy::too_many_arguments)]
fn render_tree(
    id: &str,
    bug_map: &HashMap<&str, &Change>,
    children_of: &HashMap<&str, Vec<&str>>,
    depths: &HashMap<String, usize>,
    forest_ids: &HashSet<String>,
    prefix: &str,
    is_root: bool,
    is_last: bool,
) {
    let bug = match bug_map.get(id) {
        Some(b) => *b,
        None => return,
    };

    // Choose node symbol based on status
    let node_symbol = match bug.status() {
        Status::Done => "●",
        Status::InProgress => "◐",
        Status::Approved => "○",
        Status::NotPlanned => "✕",
        _ => "○",
    };

    // Format status
    let status_str = format!("{}", bug.status());
    let status_colored = match bug.status() {
        Status::Draft => status_str.dimmed(),
        Status::Approved => status_str.green(),
        Status::InProgress => status_str.yellow(),
        Status::Blocked => status_str.red(),
        Status::Paused => status_str.cyan(),
        Status::Review => status_str.magenta(),
        Status::Done => status_str.blue(),
        Status::NotPlanned => status_str.red(),
    };

    // Format tags
    let tags = bug.tags();
    let tags_str = if tags.is_empty() {
        String::new()
    } else {
        format!(
            " {}",
            tags.iter()
                .map(|t| t.magenta().to_string())
                .collect::<Vec<_>>()
                .join(" ")
        )
    };

    // Collect blocked_by dependencies (shown as text annotation)
    let blocked_by: Vec<&str> = bug
        .blocked_by()
        .iter()
        .filter(|dep_id| forest_ids.contains(*dep_id))
        .map(|s| s.as_str())
        .collect();

    let is_blocked = !blocked_by.is_empty();

    // Build the connector for non-root nodes
    let connector = if is_root {
        "".to_string()
    } else if is_last {
        "└─".to_string()
    } else {
        "├─".to_string()
    };

    // Use different visuals for blocked vs ready items
    // Blocked items are dimmed, ready items are bright
    let (displayed_symbol, id_colored) = if is_blocked {
        (node_symbol.dimmed().to_string(), id.dimmed().to_string())
    } else {
        (node_symbol.to_string(), id.cyan().to_string())
    };

    // Print the node
    println!(
        "{}{}{}  {} [{}]{}",
        prefix, connector, displayed_symbol, id_colored, status_colored, tags_str
    );

    // Calculate prefix for continuation lines (title, blocked_by)
    let cont_prefix = if is_root {
        "   ".to_string()
    } else if is_last {
        format!("{}   ", prefix)
    } else {
        format!("{}│  ", prefix)
    };

    // Print the title (dimmed if blocked)
    if is_blocked {
        println!("{}{}", cont_prefix, bug.title().dimmed());
        println!(
            "{}{}",
            cont_prefix,
            format!("(blocked by: {})", blocked_by.join(", ")).dimmed()
        );
    } else {
        println!("{}{}", cont_prefix, bug.title());
    }

    // Get and sort children by subtree depth (deepest first for main trunk on left)
    let mut children: Vec<&str> = children_of
        .get(id)
        .map(|c| c.as_slice())
        .unwrap_or(&[])
        .to_vec();

    children.sort_by(|a, b| {
        let depth_a = depths.get(*a).unwrap_or(&0);
        let depth_b = depths.get(*b).unwrap_or(&0);
        depth_b.cmp(depth_a).then_with(|| a.cmp(b)) // Deeper first, then alphabetical
    });

    // Calculate prefix for children
    let child_prefix = if is_root {
        "".to_string()
    } else if is_last {
        format!("{}   ", prefix)
    } else {
        format!("{}│  ", prefix)
    };

    for (i, child_id) in children.iter().enumerate() {
        let is_last_child = i == children.len() - 1;
        render_tree(
            child_id,
            bug_map,
            children_of,
            depths,
            forest_ids,
            &child_prefix,
            false, // not root
            is_last_child,
        );
    }
}

/// Collect a node and all related nodes (ancestors and descendants)
fn collect_related(id: &str, all_bugs: &[Change], collected: &mut HashSet<String>) {
    if collected.contains(id) {
        return;
    }
    collected.insert(id.to_string());

    let bug = match all_bugs.iter().find(|b| b.id() == id) {
        Some(b) => b,
        None => return,
    };

    // Collect ancestors
    if let Some(parent_id) = bug.parent() {
        collect_related(parent_id, all_bugs, collected);
    }
    for blocker_id in bug.blocked_by() {
        collect_related(blocker_id, all_bugs, collected);
    }

    // Collect descendants
    for other in all_bugs {
        if other.parent() == Some(id) {
            collect_related(other.id(), all_bugs, collected);
        }
        if other.is_blocked_by(id) {
            collect_related(other.id(), all_bugs, collected);
        }
    }
}
