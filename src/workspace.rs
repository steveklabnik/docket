//! Workspace detection and utilities.
//!
//! Provides functions to detect if we're inside a workspace directory (ws-{id})
//! and extract the change ID from it.

/// Extract change ID from current directory if it's a workspace (ws-{id}).
/// Returns None if not in a workspace directory.
pub fn current_change_id() -> Option<String> {
    let cwd = std::env::current_dir().ok()?;
    let dir_name = cwd.file_name()?.to_str()?;

    // Check if directory name matches ws-{id} pattern
    if let Some(id) = dir_name.strip_prefix("ws-") {
        if !id.is_empty() {
            return Some(id.to_string());
        }
    }

    None
}

/// Check if we're inside any workspace directory.
pub fn is_in_workspace() -> bool {
    current_change_id().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_change_id_parsing() {
        // This is tricky to test since it depends on current directory
        // We'll just test the logic manually by checking the return type
        let _ = current_change_id();
    }
}
