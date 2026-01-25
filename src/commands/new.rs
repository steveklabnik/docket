use anyhow::{Context, Result};
use colored::Colorize;
use dialoguer::{Input, Select};
use std::fs;
use std::io::{self, Read};

use crate::change::{ChangelogType, Priority};
use crate::config::Config;
use crate::event::Event;
use crate::store::Store;
use crate::template::{self, TemplateContext};

/// Read body content from a file path or stdin (if path is "-")
fn read_body_from_source(source: &str) -> Result<String> {
    if source == "-" {
        let mut content = String::new();
        io::stdin()
            .read_to_string(&mut content)
            .context("failed to read body from stdin")?;
        Ok(content)
    } else {
        fs::read_to_string(source).with_context(|| format!("failed to read body from '{}'", source))
    }
}

#[allow(clippy::too_many_arguments)]
pub fn new(
    title: Option<String>,
    priority_str: &str,
    body_source: Option<&str>,
    template_name: Option<&str>,
    interactive: bool,
    changelog_type_str: Option<&str>,
    version: Option<&str>,
    tags: &[String],
    parent_id: Option<&str>,
    edit: bool,
) -> Result<()> {
    let store = Store::open()?;

    // If creating a child, verify the parent change exists
    let parent_change = if let Some(parent_ref) = parent_id {
        let parent = store.get_change(parent_ref)?;
        Some(parent)
    } else {
        None
    };

    // Get title interactively if not provided
    let title = match title {
        Some(t) => t,
        None => Input::new().with_prompt("Change title").interact_text()?,
    };

    // Parse priority - prompt interactively only if in interactive mode and using default
    let priority: Priority = if interactive && priority_str == "medium" {
        let options = vec!["low", "medium", "high"];
        let selection = Select::new()
            .with_prompt("Priority")
            .items(&options)
            .default(1)
            .interact()?;
        options[selection].parse().unwrap_or(Priority::Medium)
    } else {
        priority_str.parse().unwrap_or_else(|_| {
            eprintln!(
                "{} Invalid priority '{}', using 'medium'",
                "!".yellow(),
                priority_str
            );
            Priority::Medium
        })
    };

    // Generate ID - always use regular IDs in the unified model
    // (hierarchical IDs like abc1.1 are deprecated in favor of flat IDs with parent field)
    let id = store.generate_id()?;

    // Get body from source, template, or default
    let body = if let Some(source) = body_source {
        // Explicit body source takes precedence
        read_body_from_source(source)?
    } else {
        // Load config to get default template
        let config = Config::load(store.root())?;

        // Determine which template to use: --template flag > config default > "default"
        let effective_template = template_name
            .map(|s| s.to_string())
            .or(config.templates.default)
            .unwrap_or_else(|| "default".to_string());

        // Load and process the template
        let template_content = template::load_template(store.root(), &effective_template)?;

        // Create context for variable substitution
        let ctx = TemplateContext::new(&title, &id);

        // Substitute variables
        template::substitute_variables(&template_content, &ctx)
    };

    // Start a transaction for atomic multi-event writes
    let mut tx = store.begin_transaction(&id)?;

    // Add Created event (with parent if applicable)
    let event = if let Some(ref parent) = parent_change {
        Event::created_with_parent(
            id.clone(),
            title.clone(),
            priority,
            body,
            parent.id().to_string(),
        )
    } else {
        Event::created(id.clone(), title.clone(), priority, body)
    };
    tx.add_event(event);

    // Add ChangelogTypeSet event if changelog type was provided
    if let Some(ct_str) = changelog_type_str {
        let changelog_type: ChangelogType = ct_str.parse().with_context(|| {
            format!(
                "invalid changelog type '{}'. Valid options: feature, fix, change, deprecated, removed, security, internal",
                ct_str
            )
        })?;
        let event = Event::changelog_type_set(id.clone(), changelog_type);
        tx.add_event(event);
    }

    // Add VersionAdded event if version was provided
    if let Some(ver) = version {
        let event = Event::version_added(id.clone(), ver.to_string());
        tx.add_event(event);
    }

    // Add TagAdded events for each tag
    for tag in tags {
        let event = Event::tag_added(id.clone(), tag.clone());
        tx.add_event(event);
    }

    // Commit all events atomically
    tx.commit()?;

    if let Some(parent) = parent_change {
        println!(
            "{} Created change {} - {} (under {})",
            "✓".green(),
            id.cyan(),
            title,
            parent.id().cyan()
        );
    } else {
        println!("{} Created change {} - {}", "✓".green(), id.cyan(), title);
        if !tags.is_empty() {
            println!(
                "  Tags: {}",
                tags.iter()
                    .map(|t| t.yellow().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        if !edit {
            println!("  Edit with: {} {}", "docket edit".dimmed(), id.dimmed());
        }
    }

    // Open editor if --edit flag was passed
    if edit {
        super::edit::edit(&id)?;
    }

    Ok(())
}
