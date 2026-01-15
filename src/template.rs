use anyhow::{anyhow, Context, Result};
use chrono::Local;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

const TEMPLATES_DIR: &str = "templates";

/// Built-in templates that are created during `docket init`
pub const BUILTIN_TEMPLATES: &[(&str, &str)] = &[
    (
        "default",
        r#"## Goal

<!-- One-sentence description of success -->

## Acceptance Criteria

- [ ] First criterion

## Context

<!-- Background information, constraints, relevant details -->

## Log

<!-- Notes added during implementation -->"#,
    ),
    (
        "feature",
        r#"## Goal

<!-- What should users be able to do after this is implemented? -->

## User Story

<!-- As a [type of user], I want [an action] so that [a benefit/a value] -->

## Acceptance Criteria

- [ ] First criterion

## Design Notes

<!-- Technical approach, architecture decisions, UI/UX considerations -->

## Log

<!-- Notes added during implementation -->"#,
    ),
    (
        "bugfix",
        r#"## Problem

<!-- What's broken? What's the impact? -->

## Steps to Reproduce

1. First step
2. Second step

## Expected vs Actual

**Expected:** <!-- What should happen -->

**Actual:** <!-- What actually happens -->

## Root Cause

<!-- Technical explanation of why this happens (fill in during investigation) -->

## Fix

- [ ] Implement fix
- [ ] Add regression test

## Log

<!-- Notes added during implementation -->"#,
    ),
    (
        "chore",
        r#"## Task

<!-- What needs to be done? -->

## Motivation

<!-- Why is this necessary? What's the benefit? -->

## Checklist

- [ ] First task

## Notes

<!-- Any relevant details or considerations -->"#,
    ),
    (
        "spike",
        r#"## Question

<!-- What are we trying to learn or decide? -->

## Time Box

<!-- How much time should be spent on this investigation? -->

## Approach

<!-- How will we investigate this? -->

## Findings

<!-- What did we learn? (fill in during investigation) -->

## Recommendation

<!-- What should we do based on the findings? (fill in after investigation) -->"#,
    ),
];

/// Context for template variable substitution
pub struct TemplateContext {
    pub title: String,
    pub id: String,
    pub date: String,
}

impl TemplateContext {
    pub fn new(title: &str, id: &str) -> Self {
        Self {
            title: title.to_string(),
            id: id.to_string(),
            date: Local::now().format("%Y-%m-%d").to_string(),
        }
    }
}

/// Load a template from the templates directory or use a built-in template
pub fn load_template(docket_root: &Path, name: &str) -> Result<String> {
    let templates_dir = docket_root.join(TEMPLATES_DIR);
    let template_path = templates_dir.join(format!("{}.md", name));

    // First, try to load from the templates directory
    if template_path.exists() {
        return fs::read_to_string(&template_path)
            .with_context(|| format!("failed to read template '{}'", name));
    }

    // Fall back to built-in templates
    for (builtin_name, content) in BUILTIN_TEMPLATES {
        if *builtin_name == name {
            return Ok(content.to_string());
        }
    }

    Err(anyhow!(
        "template '{}' not found.\n\
         Available templates: {}",
        name,
        list_available_templates(docket_root)
            .unwrap_or_default()
            .join(", ")
    ))
}

/// List all available templates (built-in + custom)
pub fn list_available_templates(docket_root: &Path) -> Result<Vec<String>> {
    let mut templates: HashMap<String, ()> = HashMap::new();

    // Add built-in templates
    for (name, _) in BUILTIN_TEMPLATES {
        templates.insert(name.to_string(), ());
    }

    // Add custom templates from the templates directory
    let templates_dir = docket_root.join(TEMPLATES_DIR);
    if templates_dir.exists() {
        for entry in fs::read_dir(&templates_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    templates.insert(stem.to_string(), ());
                }
            }
        }
    }

    let mut result: Vec<_> = templates.into_keys().collect();
    result.sort();
    Ok(result)
}

/// Substitute template variables with actual values
pub fn substitute_variables(template: &str, ctx: &TemplateContext) -> String {
    template
        .replace("{{title}}", &ctx.title)
        .replace("{{date}}", &ctx.date)
        .replace("{{id}}", &ctx.id)
}

/// Create the templates directory and populate with built-in templates
pub fn create_templates_dir(docket_root: &Path) -> Result<()> {
    let templates_dir = docket_root.join(TEMPLATES_DIR);
    fs::create_dir_all(&templates_dir)
        .with_context(|| format!("failed to create {}", templates_dir.display()))?;

    // Create the default template file
    let default_template_path = templates_dir.join("default.md");
    if !default_template_path.exists() {
        let (_, content) = BUILTIN_TEMPLATES
            .iter()
            .find(|(name, _)| *name == "default")
            .expect("default template should exist");
        fs::write(&default_template_path, content)
            .with_context(|| format!("failed to write {}", default_template_path.display()))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_substitute_variables() {
        let template = "Title: {{title}}\nID: {{id}}\nDate: {{date}}";
        let ctx = TemplateContext {
            title: "Test Bug".to_string(),
            id: "abc1".to_string(),
            date: "2024-01-15".to_string(),
        };
        let result = substitute_variables(template, &ctx);
        assert_eq!(result, "Title: Test Bug\nID: abc1\nDate: 2024-01-15");
    }

    #[test]
    fn test_substitute_multiple_occurrences() {
        let template = "{{title}} - {{title}}";
        let ctx = TemplateContext {
            title: "Test".to_string(),
            id: "x".to_string(),
            date: "d".to_string(),
        };
        let result = substitute_variables(template, &ctx);
        assert_eq!(result, "Test - Test");
    }
}
