//! The `current` command for printing the current workspace's bug ID.

use anyhow::{anyhow, Result};

use crate::workspace;

/// Print the current workspace's bug ID.
pub fn current() -> Result<()> {
    match workspace::current_bug_id() {
        Some(id) => {
            println!("{}", id);
            Ok(())
        }
        None => Err(anyhow!(
            "not in a workspace directory.\n\
             Workspace directories are named ws-{{id}} where {{id}} is a bug ID.\n\
             Use 'docket work <id>' to create a workspace and start working."
        )),
    }
}
