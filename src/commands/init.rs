use anyhow::Result;
use colored::Colorize;
use std::fs;

use crate::commands::status::jj;
use crate::store::Store;
use crate::template;

const IMPLEMENT_TEMPLATE: &str = include_str!("../../templates/docket-implement.md");
const DESCRIBE_TEMPLATE: &str = include_str!("../../templates/docket-describe.md");

pub fn init() -> Result<()> {
    let store = Store::init()?;

    // Create .claude/commands/ with skill templates
    let repo_root = store.root().parent().expect("docket root has parent");
    let commands_dir = repo_root.join(".claude").join("commands");
    fs::create_dir_all(&commands_dir)?;

    let implement_path = commands_dir.join("docket-implement.md");
    if !implement_path.exists() {
        fs::write(&implement_path, IMPLEMENT_TEMPLATE)?;
        println!(
            "  {} .claude/commands/docket-implement.md",
            "Created".green()
        );
    }

    let describe_path = commands_dir.join("docket-describe.md");
    if !describe_path.exists() {
        fs::write(&describe_path, DESCRIBE_TEMPLATE)?;
        println!(
            "  {} .claude/commands/docket-describe.md",
            "Created".green()
        );
    }

    // Create .docket/templates/ with default template
    template::create_templates_dir(store.root())?;
    println!("  {} .docket/templates/default.md", "Created".green());

    // Try to initialize the state branch for jj repositories
    match jj::init_state_branch() {
        Ok(true) => {
            println!(
                "  {} {} bookmark on orphan state branch",
                "Created".green(),
                jj::STATE_BRANCH.cyan()
            );
        }
        Ok(false) => {
            // Not a jj repo or jj not installed - that's fine, we just skip state branch creation
        }
        Err(e) => {
            // Log a warning but don't fail init
            eprintln!("{} Failed to create state branch: {}", "!".yellow(), e);
        }
    }

    println!(
        "{} Initialized docket repository at {}",
        "✓".green(),
        store.root().display()
    );
    Ok(())
}
