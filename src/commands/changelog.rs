use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use colored::Colorize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::bug::{Bug, ChangelogType, Status};
use crate::store::Store;

/// Generate changelog for a specific version
pub fn changelog(version: &str, preview: bool, file: Option<&str>) -> Result<()> {
    let store = Store::open()?;
    let bugs = store.list_bugs()?;

    // Filter bugs that have this version
    let version_bugs: Vec<_> = bugs
        .into_iter()
        .filter(|bug| bug.has_version(version))
        .collect();

    if version_bugs.is_empty() {
        println!(
            "{} No bugs found for version {}",
            "!".yellow(),
            version.cyan()
        );
        return Ok(());
    }

    // Check for bugs without changelog type
    let mut warnings = Vec::new();
    for bug in &version_bugs {
        if bug.changelog_type().is_none() {
            warnings.push(format!(
                "Bug {} ({}) has no changelog type - it will be skipped",
                bug.id(),
                bug.title()
            ));
        }
    }

    // Print warnings
    for warning in &warnings {
        println!("{} {}", "!".yellow(), warning);
    }

    // Generate the changelog content
    let changelog_content = generate_changelog_section(&version_bugs, version)?;

    if preview {
        println!("{}", "Preview of changelog:".bold());
        println!("{}", "-".repeat(60).dimmed());
        println!("{}", changelog_content);
        println!("{}", "-".repeat(60).dimmed());
        return Ok(());
    }

    // Write to file
    let file_path = file.unwrap_or("CHANGELOG.md");
    let path = Path::new(file_path);

    // Check if version already exists in changelog
    if path.exists() {
        let existing_content = fs::read_to_string(path)?;
        if existing_content.contains(&format!("## [{version}]"))
            || existing_content.contains(&format!("## {version}"))
        {
            return Err(anyhow!(
                "Version {} already exists in {}. Remove it first to regenerate.",
                version,
                file_path
            ));
        }
    }

    // Update or create the changelog file
    update_changelog_file(path, &changelog_content, version)?;

    println!(
        "{} Generated changelog for version {} in {}",
        "✓".green(),
        version.cyan(),
        file_path
    );

    // Show summary
    let mut type_counts: HashMap<&str, usize> = HashMap::new();
    for bug in &version_bugs {
        if let Some(ct) = bug.changelog_type() {
            if let Some(header) = ct.section_header() {
                *type_counts.entry(header).or_insert(0) += 1;
            }
        }
    }

    println!("  Included:");
    for (section, count) in type_counts {
        println!("    {} {} item(s)", section, count);
    }

    if !warnings.is_empty() {
        println!(
            "  Skipped: {} item(s) without changelog type",
            warnings.len()
        );
    }

    Ok(())
}

/// Generate the markdown content for a version section
fn generate_changelog_section(bugs: &[Bug], version: &str) -> Result<String> {
    let mut output = String::new();
    let today = Utc::now().format("%Y-%m-%d");

    output.push_str(&format!("## [{version}] - {today}\n\n"));

    // Group bugs by changelog type
    let mut sections: HashMap<ChangelogType, Vec<&Bug>> = HashMap::new();
    for bug in bugs {
        if let Some(ct) = bug.changelog_type() {
            // Skip internal changes
            if ct.section_header().is_some() {
                sections.entry(ct.clone()).or_default().push(bug);
            }
        }
    }

    // Sort sections by the standard Keep a Changelog order
    let mut sorted_types: Vec<_> = sections.keys().collect();
    sorted_types.sort_by_key(|ct| ct.sort_order());

    for changelog_type in sorted_types {
        if let Some(header) = changelog_type.section_header() {
            let bugs_in_section: &Vec<&Bug> = sections.get(changelog_type).unwrap();
            output.push_str(&format!("### {header}\n\n"));

            for bug in bugs_in_section {
                output.push_str(&format!("- {} ({})\n", bug.title(), bug.id()));
            }
            output.push('\n');
        }
    }

    Ok(output)
}

/// Update the CHANGELOG.md file with new version content
fn update_changelog_file(path: &Path, new_content: &str, _version: &str) -> Result<()> {
    if path.exists() {
        let existing = fs::read_to_string(path)?;

        // Find the insertion point: after the header, before the first version section
        // Standard Keep a Changelog format has a header followed by version sections
        let insertion_point = find_insertion_point(&existing);

        let mut result = String::new();
        result.push_str(&existing[..insertion_point]);
        if !result.ends_with('\n') {
            result.push('\n');
        }
        if !result.ends_with("\n\n") {
            result.push('\n');
        }
        result.push_str(new_content);
        result.push_str(&existing[insertion_point..]);

        fs::write(path, result).context("failed to write changelog")?;
    } else {
        // Create new changelog with standard header
        let header = r#"# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

"#;
        let content = format!("{header}{new_content}");
        fs::write(path, content).context("failed to write changelog")?;
    }

    Ok(())
}

/// Find the best insertion point for new changelog content
fn find_insertion_point(content: &str) -> usize {
    // Look for existing version sections (## [X.Y.Z] or ## X.Y.Z)
    if let Some((idx, _)) = content.match_indices("\n## [").next() {
        return idx + 1; // Insert before this line
    }
    for (idx, _) in content.match_indices("\n## ") {
        // Check if this looks like a version (starts with digit)
        let after = &content[idx + 4..];
        if after
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
        {
            return idx + 1;
        }
    }

    // No existing versions found, append at end
    content.len()
}

/// Helper to check if a bug should appear in changelog
pub fn should_include_in_changelog(bug: &Bug) -> bool {
    match bug.changelog_type() {
        Some(ct) => ct.section_header().is_some(),
        None => false,
    }
}

/// Get bugs that are done but have no version assigned
pub fn get_unreleased_done_bugs(store: &Store) -> Result<Vec<Bug>> {
    let bugs = store.list_bugs()?;
    Ok(bugs
        .into_iter()
        .filter(|bug| matches!(bug.status(), Status::Done) && bug.versions().is_empty())
        .collect())
}
