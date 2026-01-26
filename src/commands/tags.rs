use anyhow::Result;
use colored::Colorize;
use std::collections::HashMap;

use crate::store::Store;

/// List all tags in use with their bug counts
pub fn tags(by_count: bool) -> Result<()> {
    let store = Store::open()?;
    let bugs = store.list_changes()?;

    // Collect tag counts
    let mut tag_counts: HashMap<String, usize> = HashMap::new();
    for bug in &bugs {
        for tag in bug.tags() {
            *tag_counts.entry(tag.clone()).or_insert(0) += 1;
        }
    }

    if tag_counts.is_empty() {
        println!("{}", "No tags found.".dimmed());
        return Ok(());
    }

    // Sort tags
    let mut tags: Vec<_> = tag_counts.into_iter().collect();
    if by_count {
        // Sort by count descending, then alphabetically
        tags.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    } else {
        // Sort alphabetically
        tags.sort_by(|a, b| a.0.cmp(&b.0));
    }

    // Print tags
    for (tag, count) in tags {
        let bug_word = if count == 1 { "bug" } else { "bugs" };
        println!("{:20} ({} {})", tag.yellow(), count, bug_word);
    }

    Ok(())
}
