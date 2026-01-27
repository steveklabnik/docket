use anyhow::{anyhow, Context, Result};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use rand::Rng;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use crate::change::Change;
use crate::commands::status::jj;
use crate::event::{self, Event};
use crate::release::{
    self, append_release_event, derive_release, read_release_events, Release, ReleaseEvent,
    UNSCHEDULED_RELEASE,
};

/// Storage mode for docket data.
///
/// Docket supports two storage modes:
/// - `FileSystem`: Legacy mode where `.docket/` directory is in the working tree
/// - `StateBranch`: New mode where `.docket/` is stored on an orphan state branch
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreMode {
    /// Legacy: .docket in working tree
    FileSystem {
        /// Path to the .docket directory
        root: PathBuf,
    },
    /// New: .docket on orphan state branch
    StateBranch {
        /// Path to the repository root (containing .jj or .git)
        repo_root: PathBuf,
    },
}

const DOCKET_DIR: &str = ".docket";
const CHANGES_DIR: &str = "changes";
const RELEASES_DIR: &str = "releases";
const LEGACY_BUGS_DIR: &str = "bugs";
const ID_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
const ID_LENGTH: usize = 4;

/// Name of the bookmark/branch used for state branch storage
pub const STATE_BRANCH_NAME: &str = "docket-state";

#[derive(Debug)]
pub struct Store {
    /// Storage mode (FileSystem or StateBranch)
    pub mode: StoreMode,
    /// Path to the .docket directory (for FileSystem mode, this is the actual path;
    /// for StateBranch mode, this will be a temporary directory in the future)
    root: PathBuf,
}

impl Store {
    /// Find .docket/ directory by walking up from current directory
    pub fn open() -> Result<Self> {
        let current = std::env::current_dir()?;
        Self::open_from(&current)
    }

    /// Find .docket/ directory by walking up from specified directory
    ///
    /// Detection order:
    /// 1. Find repo root (look for .jj or .git)
    /// 2. Check if docket-state bookmark exists → StateBranch mode (takes priority)
    /// 3. Check if .docket directory exists → FileSystem mode (fallback)
    /// 4. Neither → error
    pub fn open_from(start: &Path) -> Result<Self> {
        let mut current = start.to_path_buf();

        // Track if we find a repo root (for potential state branch mode)
        let mut repo_root: Option<PathBuf> = None;
        // Track if we find a .docket directory (for potential filesystem mode)
        let mut docket_dir: Option<PathBuf> = None;

        loop {
            // Check for repo root (.jj or .git)
            if repo_root.is_none()
                && (current.join(".jj").is_dir() || current.join(".git").exists())
            {
                repo_root = Some(current.clone());
            }

            // Check for .docket directory (potential FileSystem mode)
            if docket_dir.is_none() {
                let docket_path = current.join(DOCKET_DIR);
                if docket_path.is_dir() {
                    docket_dir = Some(docket_path);
                }
            }

            if !current.pop() {
                break;
            }
        }

        // Priority 1: If we found a repo root, check for state branch first
        // StateBranch mode takes priority over FileSystem mode when both exist
        if let Some(ref repo_root) = repo_root {
            if Self::has_state_branch(repo_root)? {
                // StateBranch mode - data is stored on the docket-state branch
                // The root path is virtual (used for consistency but not for actual file access)
                let virtual_root = repo_root.join(DOCKET_DIR);
                let mode = StoreMode::StateBranch {
                    repo_root: repo_root.clone(),
                };
                return Ok(Store {
                    mode,
                    root: virtual_root,
                });
            }
        }

        // Priority 2: Fall back to FileSystem mode if .docket directory exists
        if let Some(docket_path) = docket_dir {
            let mode = StoreMode::FileSystem {
                root: docket_path.clone(),
            };
            let store = Store {
                mode,
                root: docket_path,
            };
            // Migrate from legacy bugs/ directory if needed
            store.migrate_bugs_to_changes()?;
            // Ensure releases directory exists (for repos created before releases feature)
            store.ensure_releases_dir()?;
            store.ensure_unscheduled_release()?;
            return Ok(store);
        }

        Err(anyhow!(
            "not a docket repository (or any parent): .docket directory not found.\n\
             Run 'docket init' to initialize a new docket repository."
        ))
    }

    /// Check if the docket-state branch/bookmark exists in a repository
    fn has_state_branch(repo_root: &Path) -> Result<bool> {
        // Check for jj first (preferred)
        if repo_root.join(".jj").is_dir() {
            return Self::has_jj_bookmark(repo_root, STATE_BRANCH_NAME);
        }

        // Fall back to git
        if repo_root.join(".git").exists() {
            return Self::has_git_branch(repo_root, STATE_BRANCH_NAME);
        }

        Ok(false)
    }

    /// Check if a jj bookmark exists
    fn has_jj_bookmark(repo_root: &Path, bookmark_name: &str) -> Result<bool> {
        use std::process::Command;

        let output = Command::new("jj")
            .args([
                "bookmark",
                "list",
                "--repository",
                repo_root.to_str().unwrap_or("."),
            ])
            .output();

        match output {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                // jj bookmark list output format: "bookmark_name: commit_id"
                Ok(stdout.lines().any(|line| {
                    line.split(':').next().map(|name| name.trim()) == Some(bookmark_name)
                }))
            }
            _ => Ok(false),
        }
    }

    /// Check if a git branch exists
    fn has_git_branch(repo_root: &Path, branch_name: &str) -> Result<bool> {
        use std::process::Command;

        let output = Command::new("git")
            .args([
                "-C",
                repo_root.to_str().unwrap_or("."),
                "branch",
                "--list",
                branch_name,
            ])
            .output();

        match output {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                Ok(!stdout.trim().is_empty())
            }
            _ => Ok(false),
        }
    }

    /// Initialize a new .docket/ directory in the current location
    pub fn init() -> Result<Self> {
        let current = std::env::current_dir()?;
        Self::init_at(&current)
    }

    /// Initialize a new .docket/ directory at specified location (FileSystem mode)
    pub fn init_at(path: &Path) -> Result<Self> {
        let root = path.join(DOCKET_DIR);

        if root.exists() {
            return Err(anyhow!("docket already initialized at {}", root.display()));
        }

        let changes_dir = root.join(CHANGES_DIR);
        fs::create_dir_all(&changes_dir)
            .with_context(|| format!("failed to create {}", changes_dir.display()))?;

        let releases_dir = root.join(RELEASES_DIR);
        fs::create_dir_all(&releases_dir)
            .with_context(|| format!("failed to create {}", releases_dir.display()))?;

        // Create .gitignore for cache directory
        let gitignore_path = root.join(".gitignore");
        fs::write(&gitignore_path, ".cache/\n")
            .with_context(|| format!("failed to create {}", gitignore_path.display()))?;

        let mode = StoreMode::FileSystem { root: root.clone() };
        let store = Store { mode, root };

        // Create the special "unscheduled" release
        store.ensure_unscheduled_release()?;

        Ok(store)
    }

    /// Migrate from legacy .docket/bugs/ directory to .docket/changes/
    fn migrate_bugs_to_changes(&self) -> Result<()> {
        let legacy_dir = self.root.join(LEGACY_BUGS_DIR);
        let new_dir = self.root.join(CHANGES_DIR);

        // If legacy directory doesn't exist, nothing to migrate
        if !legacy_dir.exists() {
            return Ok(());
        }

        // If new directory already exists with content, don't migrate
        if new_dir.exists() && new_dir.read_dir()?.next().is_some() {
            return Ok(());
        }

        // Create new directory if needed
        fs::create_dir_all(&new_dir)
            .with_context(|| format!("failed to create {}", new_dir.display()))?;

        // Move all contents from legacy to new directory
        for entry in fs::read_dir(&legacy_dir)? {
            let entry = entry?;
            let old_path = entry.path();
            let file_name = entry.file_name();
            let new_path = new_dir.join(&file_name);

            fs::rename(&old_path, &new_path).with_context(|| {
                format!(
                    "failed to migrate {} to {}",
                    old_path.display(),
                    new_path.display()
                )
            })?;
        }

        // Remove the now-empty legacy directory
        fs::remove_dir(&legacy_dir).with_context(|| {
            format!("failed to remove legacy directory {}", legacy_dir.display())
        })?;

        eprintln!("Migrated change data from .docket/bugs/ to .docket/changes/");

        Ok(())
    }

    /// Path to the changes directory
    fn changes_dir(&self) -> PathBuf {
        self.root.join(CHANGES_DIR)
    }

    /// Get the shard key (first character) for a change ID
    fn shard_key(id: &str) -> Option<char> {
        id.chars().next()
    }

    /// Path to a specific change's event log file (sharded structure)
    /// Returns the sharded path: .docket/changes/{first_char}/{id}.jsonl
    fn change_path(&self, id: &str) -> PathBuf {
        if let Some(shard) = Self::shard_key(id) {
            self.changes_dir()
                .join(shard.to_string())
                .join(format!("{}.jsonl", id))
        } else {
            // Fallback for empty ID (shouldn't happen in practice)
            self.changes_dir().join(format!("{}.jsonl", id))
        }
    }

    /// Path to the legacy flat location for a change
    fn change_path_flat(&self, id: &str) -> PathBuf {
        self.changes_dir().join(format!("{}.jsonl", id))
    }

    /// Find the actual path where a change file exists
    /// Checks sharded path first, then falls back to flat path for backward compatibility
    fn find_change_path(&self, id: &str) -> Option<PathBuf> {
        let sharded = self.change_path(id);
        if sharded.exists() {
            return Some(sharded);
        }
        let flat = self.change_path_flat(id);
        if flat.exists() {
            return Some(flat);
        }
        None
    }

    /// Check if a change exists (works in both FileSystem and StateBranch modes)
    fn change_exists(&self, id: &str) -> Result<bool> {
        if self.is_state_branch_mode() {
            let all_ids = self.list_all_change_ids_state_branch()?;
            Ok(all_ids.contains(&id.to_string()))
        } else {
            Ok(self.find_change_path(id).is_some())
        }
    }

    /// Migrate a change file from flat to sharded structure if needed
    fn migrate_to_sharded(&self, id: &str) -> Result<()> {
        let flat_path = self.change_path_flat(id);
        if !flat_path.exists() {
            return Ok(()); // Nothing to migrate
        }

        let sharded_path = self.change_path(id);
        if sharded_path.exists() {
            return Ok(()); // Already migrated
        }

        // Create shard directory if needed
        if let Some(shard_dir) = sharded_path.parent() {
            fs::create_dir_all(shard_dir).with_context(|| {
                format!("failed to create shard directory {}", shard_dir.display())
            })?;
        }

        // Move the file
        fs::rename(&flat_path, &sharded_path).with_context(|| {
            format!(
                "failed to migrate {} to {}",
                flat_path.display(),
                sharded_path.display()
            )
        })?;

        Ok(())
    }

    /// List all changes (searches both sharded and flat structures)
    pub fn list_changes(&self) -> Result<Vec<Change>> {
        // Dispatch to correct implementation based on mode
        if self.is_state_branch_mode() {
            return self.list_from_state_branch();
        }

        // FileSystem mode
        let changes_dir = self.changes_dir();

        // Pattern for sharded structure: .docket/changes/*/*.jsonl
        let sharded_pattern = changes_dir.join("*").join("*.jsonl");
        let sharded_pattern_str = sharded_pattern
            .to_str()
            .ok_or_else(|| anyhow!("invalid path encoding"))?;

        // Pattern for flat structure (backward compat): .docket/changes/*.jsonl
        let flat_pattern = changes_dir.join("*.jsonl");
        let flat_pattern_str = flat_pattern
            .to_str()
            .ok_or_else(|| anyhow!("invalid path encoding"))?;

        let mut changes = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();

        // Search sharded structure first
        for entry in glob::glob(sharded_pattern_str)? {
            let path = entry?;
            let events = event::read_events(&path)?;

            match event::derive_change(&events) {
                Ok(change) => {
                    seen_ids.insert(change.id().to_string());
                    changes.push(change);
                }
                Err(e) => {
                    eprintln!(
                        "warning: failed to derive change from {}: {}",
                        path.display(),
                        e
                    );
                }
            }
        }

        // Search flat structure for any changes not yet migrated
        for entry in glob::glob(flat_pattern_str)? {
            let path = entry?;
            let events = event::read_events(&path)?;

            match event::derive_change(&events) {
                Ok(change) => {
                    // Only add if not already found in sharded structure
                    if !seen_ids.contains(change.id()) {
                        changes.push(change);
                    }
                }
                Err(e) => {
                    eprintln!(
                        "warning: failed to derive change from {}: {}",
                        path.display(),
                        e
                    );
                }
            }
        }

        // Sort by created date, newest first
        changes.sort_by(|a, b| b.metadata.created.cmp(&a.metadata.created));

        Ok(changes)
    }

    /// Get a specific change by ID (supports prefix and fuzzy matching)
    pub fn get_change(&self, id: &str) -> Result<Change> {
        // Dispatch to correct implementation based on mode
        if self.is_state_branch_mode() {
            return self.get_change_state_branch(id);
        }

        // FileSystem mode: First try exact match (checks sharded then flat)
        if let Some(path) = self.find_change_path(id) {
            let events = event::read_events(&path)?;
            return event::derive_change(&events);
        }

        // Try prefix match in both structures
        let prefix_matches = self.find_matching_changes(id)?;

        match prefix_matches.len() {
            0 => {
                // No prefix matches, try fuzzy matching
                let fuzzy_matches = self.find_fuzzy_matches(id)?;
                match fuzzy_matches.len() {
                    0 => Err(anyhow!(
                        "change not found: '{}'\n\
                         Run 'docket list' to see all changes, or 'docket new' to create one.",
                        id
                    )),
                    1 => {
                        let path = self
                            .find_change_path(&fuzzy_matches[0])
                            .ok_or_else(|| anyhow!("internal error: fuzzy match path not found"))?;
                        let events = event::read_events(&path)?;
                        event::derive_change(&events)
                    }
                    _ => {
                        // Check if top matches have the same score (truly ambiguous)
                        // For now, just report ambiguity with all fuzzy matches
                        Err(anyhow!(
                            "ambiguous change ID '{}', fuzzy matches: {}",
                            id,
                            fuzzy_matches.join(", ")
                        ))
                    }
                }
            }
            1 => {
                let events = event::read_events(&prefix_matches[0])?;
                event::derive_change(&events)
            }
            _ => {
                let ids: Vec<_> = prefix_matches
                    .iter()
                    .filter_map(|p| p.file_stem())
                    .filter_map(|s| s.to_str())
                    .collect();
                Err(anyhow!(
                    "ambiguous change ID '{}', matches: {}",
                    id,
                    ids.join(", ")
                ))
            }
        }
    }

    /// Find all change files matching a prefix (searches both sharded and flat structures)
    fn find_matching_changes(&self, prefix: &str) -> Result<Vec<PathBuf>> {
        let changes_dir = self.changes_dir();
        let mut matches = Vec::new();
        let mut seen_stems = std::collections::HashSet::new();

        // Search sharded structure: .docket/changes/*/{prefix}*.jsonl
        let sharded_pattern = changes_dir.join("*").join(format!("{}*.jsonl", prefix));
        if let Some(pattern_str) = sharded_pattern.to_str() {
            for entry in glob::glob(pattern_str)? {
                let path = entry?;
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    seen_stems.insert(stem.to_string());
                    matches.push(path);
                }
            }
        }

        // Search flat structure: .docket/changes/{prefix}*.jsonl
        let flat_pattern = changes_dir.join(format!("{}*.jsonl", prefix));
        if let Some(pattern_str) = flat_pattern.to_str() {
            for entry in glob::glob(pattern_str)? {
                let path = entry?;
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    // Only add if not found in sharded structure
                    if !seen_stems.contains(stem) {
                        matches.push(path);
                    }
                }
            }
        }

        Ok(matches)
    }

    /// Find all change IDs in the repository (for fuzzy matching)
    fn list_all_change_ids(&self) -> Result<Vec<String>> {
        // Dispatch to correct implementation based on mode
        if self.is_state_branch_mode() {
            return self.list_all_change_ids_state_branch();
        }

        // FileSystem mode
        let changes_dir = self.changes_dir();
        let mut ids = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // Search sharded structure: .docket/changes/*/*.jsonl
        let sharded_pattern = changes_dir.join("*").join("*.jsonl");
        if let Some(pattern_str) = sharded_pattern.to_str() {
            for entry in glob::glob(pattern_str)? {
                let path = entry?;
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if seen.insert(stem.to_string()) {
                        ids.push(stem.to_string());
                    }
                }
            }
        }

        // Search flat structure: .docket/changes/*.jsonl
        let flat_pattern = changes_dir.join("*.jsonl");
        if let Some(pattern_str) = flat_pattern.to_str() {
            for entry in glob::glob(pattern_str)? {
                let path = entry?;
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if seen.insert(stem.to_string()) {
                        ids.push(stem.to_string());
                    }
                }
            }
        }

        Ok(ids)
    }

    /// List all change IDs from the state branch
    fn list_all_change_ids_state_branch(&self) -> Result<Vec<String>> {
        let files = jj::list_state_files(".docket/changes/*/*.jsonl")?;
        let ids: Vec<String> = files
            .iter()
            .filter_map(|path| {
                // Extract ID from path like ".docket/changes/a/abc1.jsonl"
                let path = std::path::Path::new(path);
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
            })
            .collect();
        Ok(ids)
    }

    /// Find changes using fuzzy matching
    /// Returns change IDs sorted by match score (best match first)
    fn find_fuzzy_matches(&self, query: &str) -> Result<Vec<String>> {
        let all_ids = self.list_all_change_ids()?;
        let matcher = SkimMatcherV2::default();

        let mut scored: Vec<(String, i64)> = all_ids
            .into_iter()
            .filter_map(|id| matcher.fuzzy_match(&id, query).map(|score| (id, score)))
            .collect();

        // Sort by score descending (best matches first)
        scored.sort_by(|a, b| b.1.cmp(&a.1));

        Ok(scored.into_iter().map(|(id, _)| id).collect())
    }

    /// Append an event to a change's event log
    /// If the change exists in flat structure, migrates it to sharded first
    pub fn append_event(&self, event: &Event) -> Result<()> {
        match &self.mode {
            StoreMode::FileSystem { .. } => self.append_event_filesystem(event),
            StoreMode::StateBranch { .. } => self.append_to_state_branch(event),
        }
    }

    /// Append an event using filesystem storage (legacy mode)
    fn append_event_filesystem(&self, event: &Event) -> Result<()> {
        // Migrate from flat to sharded if needed
        self.migrate_to_sharded(&event.change_id)?;

        // Get the sharded path and ensure directory exists
        let path = self.change_path(&event.change_id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }

        event::append_event(&path, event)
    }

    /// Append an event to the state branch (new mode)
    ///
    /// This atomically writes the event to the state branch by:
    /// 1. Reading the current content (if any)
    /// 2. Appending the new event JSON
    /// 3. Writing atomically via jj
    fn append_to_state_branch(&self, event: &Event) -> Result<()> {
        let change_id = &event.change_id;

        // Build the path: .docket/changes/{first_char}/{id}.jsonl
        let shard = change_id
            .chars()
            .next()
            .ok_or_else(|| anyhow!("empty change ID"))?;
        let path = format!(".docket/changes/{}/{}.jsonl", shard, change_id);

        // Read current content (may not exist)
        let current = jj::read_state_file(&path).unwrap_or_default();

        // Serialize the new event
        let event_json = serde_json::to_string(event).context("failed to serialize event")?;

        // Append new event to content
        let new_content = if current.is_empty() {
            format!("{}\n", event_json)
        } else if current.ends_with('\n') {
            format!("{}{}\n", current, event_json)
        } else {
            format!("{}\n{}\n", current, event_json)
        };

        // Write atomically via jj
        let message = format!("docket: update {}", change_id);
        jj::write_to_state_branch(&path, &new_content, &message)?;

        Ok(())
    }

    /// Get all events for a change
    pub fn get_events(&self, id: &str) -> Result<Vec<Event>> {
        // Dispatch to correct implementation based on mode
        if self.is_state_branch_mode() {
            return self.read_from_state_branch(id);
        }

        // FileSystem mode
        let path = self.find_change_path(id).ok_or_else(|| {
            anyhow!(
                "change not found: '{}'\n\
                 Run 'docket list' to see all changes.",
                id
            )
        })?;
        event::read_events(&path)
    }

    /// Resolve a change ID prefix to the full ID (supports prefix and fuzzy matching)
    pub fn resolve_id(&self, id: &str) -> Result<String> {
        // Dispatch to correct implementation based on mode
        if self.is_state_branch_mode() {
            return self.resolve_id_state_branch(id);
        }

        // FileSystem mode: First try exact match (checks sharded then flat)
        if self.find_change_path(id).is_some() {
            return Ok(id.to_string());
        }

        // Try prefix match in both structures
        let prefix_matches = self.find_matching_changes(id)?;

        match prefix_matches.len() {
            0 => {
                // No prefix matches, try fuzzy matching
                let fuzzy_matches = self.find_fuzzy_matches(id)?;
                match fuzzy_matches.len() {
                    0 => Err(anyhow!(
                        "change not found: '{}'\n\
                         Run 'docket list' to see all changes.",
                        id
                    )),
                    1 => Ok(fuzzy_matches[0].clone()),
                    _ => Err(anyhow!(
                        "ambiguous change ID '{}', fuzzy matches: {}",
                        id,
                        fuzzy_matches.join(", ")
                    )),
                }
            }
            1 => {
                let full_id = prefix_matches[0]
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| anyhow!("invalid file name"))?;
                Ok(full_id.to_string())
            }
            _ => {
                let ids: Vec<_> = prefix_matches
                    .iter()
                    .filter_map(|p| p.file_stem())
                    .filter_map(|s| s.to_str())
                    .collect();
                Err(anyhow!(
                    "ambiguous change ID '{}', matches: {}",
                    id,
                    ids.join(", ")
                ))
            }
        }
    }

    /// Resolve a change ID in StateBranch mode
    fn resolve_id_state_branch(&self, id: &str) -> Result<String> {
        let all_ids = self.list_all_change_ids_state_branch()?;

        // First try exact match
        if all_ids.contains(&id.to_string()) {
            return Ok(id.to_string());
        }

        // Try prefix match
        let prefix_matches: Vec<_> = all_ids
            .iter()
            .filter(|change_id| change_id.starts_with(id))
            .cloned()
            .collect();

        match prefix_matches.len() {
            0 => {
                // No prefix matches, try fuzzy matching
                let fuzzy_matches = self.find_fuzzy_matches(id)?;
                match fuzzy_matches.len() {
                    0 => Err(anyhow!(
                        "change not found: '{}'\n\
                         Run 'docket list' to see all changes.",
                        id
                    )),
                    1 => Ok(fuzzy_matches[0].clone()),
                    _ => Err(anyhow!(
                        "ambiguous change ID '{}', fuzzy matches: {}",
                        id,
                        fuzzy_matches.join(", ")
                    )),
                }
            }
            1 => Ok(prefix_matches[0].clone()),
            _ => Err(anyhow!(
                "ambiguous change ID '{}', matches: {}",
                id,
                prefix_matches.join(", ")
            )),
        }
    }

    /// Generate a unique change ID
    pub fn generate_id(&self) -> Result<String> {
        let mut rng = rand::thread_rng();

        // Try up to 100 times to generate a unique ID
        for _ in 0..100 {
            let id: String = (0..ID_LENGTH)
                .map(|_| {
                    let idx = rng.gen_range(0..ID_CHARS.len());
                    ID_CHARS[idx] as char
                })
                .collect();

            // Check uniqueness across all modes
            if !self.change_exists(&id)? {
                return Ok(id);
            }
        }

        Err(anyhow!("failed to generate unique ID after 100 attempts"))
    }

    /// Generate the next child ID for an epic (e.g., abc1.1, abc1.2, abc1.3)
    pub fn generate_child_id(&self, parent_id: &str) -> Result<String> {
        let changes_dir = self.changes_dir();

        // Find existing children in both sharded and flat structures
        let mut max_num: u32 = 0;

        // Helper to extract child number from path
        let extract_num = |path: &Path| -> Option<u32> {
            let stem = path.file_stem()?.to_str()?;
            let num_str = stem.strip_prefix(&format!("{}.", parent_id))?;
            num_str.parse::<u32>().ok()
        };

        // Search sharded structure: .docket/changes/*/{parent_id}.*.jsonl
        let sharded_pattern = changes_dir.join("*").join(format!("{}.*.jsonl", parent_id));
        if let Some(pattern_str) = sharded_pattern.to_str() {
            for path in glob::glob(pattern_str)?.flatten() {
                if let Some(num) = extract_num(&path) {
                    max_num = max_num.max(num);
                }
            }
        }

        // Search flat structure: .docket/changes/{parent_id}.*.jsonl
        let flat_pattern = changes_dir.join(format!("{}.*.jsonl", parent_id));
        if let Some(pattern_str) = flat_pattern.to_str() {
            for path in glob::glob(pattern_str)?.flatten() {
                if let Some(num) = extract_num(&path) {
                    max_num = max_num.max(num);
                }
            }
        }

        Ok(format!("{}.{}", parent_id, max_num + 1))
    }

    /// Get the root .docket directory path
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Check if the store is in FileSystem mode
    pub fn is_filesystem_mode(&self) -> bool {
        matches!(self.mode, StoreMode::FileSystem { .. })
    }

    /// Check if the store is in StateBranch mode
    pub fn is_state_branch_mode(&self) -> bool {
        matches!(self.mode, StoreMode::StateBranch { .. })
    }

    // ========================================================================
    // State Branch Reading Methods
    // ========================================================================

    /// Read change events from the state branch.
    /// Used when in StateBranch mode.
    fn read_from_state_branch(&self, change_id: &str) -> Result<Vec<Event>> {
        // Build the path: .docket/changes/{first_char}/{id}.jsonl
        let shard = change_id
            .chars()
            .next()
            .ok_or_else(|| anyhow!("empty change ID"))?;
        let path = format!(".docket/changes/{}/{}.jsonl", shard, change_id);

        let content = jj::read_state_file(&path)?;
        event::parse_jsonl_content(&content)
    }

    /// List all changes from the state branch.
    /// Used when in StateBranch mode.
    fn list_from_state_branch(&self) -> Result<Vec<Change>> {
        // Get all change files from state branch
        // Pattern: .docket/changes/*/*.jsonl
        let files = jj::list_state_files(".docket/changes/*/*.jsonl")?;

        let mut changes = Vec::new();

        for file_path in files {
            // Read and parse each change file
            match jj::read_state_file(&file_path) {
                Ok(content) => match event::parse_jsonl_content(&content) {
                    Ok(events) => match event::derive_change(&events) {
                        Ok(change) => changes.push(change),
                        Err(e) => {
                            eprintln!("warning: failed to derive change from {}: {}", file_path, e);
                        }
                    },
                    Err(e) => {
                        eprintln!("warning: failed to parse events from {}: {}", file_path, e);
                    }
                },
                Err(e) => {
                    eprintln!("warning: failed to read {}: {}", file_path, e);
                }
            }
        }

        // Sort by created date, newest first
        changes.sort_by(|a, b| b.metadata.created.cmp(&a.metadata.created));

        Ok(changes)
    }

    /// Get a specific change by ID from the state branch.
    /// Supports prefix matching for convenience.
    fn get_change_state_branch(&self, id: &str) -> Result<Change> {
        // Try exact match first
        if let Ok(events) = self.read_from_state_branch(id) {
            if !events.is_empty() {
                return event::derive_change(&events);
            }
        }

        // Try prefix match by listing all changes and filtering
        let files = jj::list_state_files(".docket/changes/*/*.jsonl")?;

        let mut prefix_matches: Vec<String> = Vec::new();

        for file_path in &files {
            // Extract change ID from path: .docket/changes/a/abcd.jsonl -> abcd
            if let Some(file_name) = file_path.rsplit('/').next() {
                if let Some(change_id) = file_name.strip_suffix(".jsonl") {
                    if change_id.starts_with(id) {
                        prefix_matches.push(change_id.to_string());
                    }
                }
            }
        }

        match prefix_matches.len() {
            0 => {
                // No prefix matches, try fuzzy matching
                let fuzzy_matches = self.find_fuzzy_matches(id)?;
                match fuzzy_matches.len() {
                    0 => Err(anyhow!(
                        "change not found: '{}'\n\
                         Run 'docket list' to see all changes, or 'docket new' to create one.",
                        id
                    )),
                    1 => {
                        let events = self.read_from_state_branch(&fuzzy_matches[0])?;
                        event::derive_change(&events)
                    }
                    _ => Err(anyhow!(
                        "ambiguous change ID '{}', fuzzy matches: {}",
                        id,
                        fuzzy_matches.join(", ")
                    )),
                }
            }
            1 => {
                let events = self.read_from_state_branch(&prefix_matches[0])?;
                event::derive_change(&events)
            }
            _ => Err(anyhow!(
                "ambiguous change ID '{}', matches: {}",
                id,
                prefix_matches.join(", ")
            )),
        }
    }

    // ========================================================================

    /// Get the directory where workspace directories (ws-*) are located.
    /// This is the parent of .docket, unless we're inside a workspace directory,
    /// in which case we need to go up one more level.
    pub fn workspaces_dir(&self) -> Option<PathBuf> {
        let parent = self.root.parent()?;

        // Check if we're inside a workspace directory (ws-*)
        if let Some(dir_name) = parent.file_name().and_then(|n| n.to_str()) {
            if dir_name.starts_with("ws-") {
                // We're inside a workspace, go up one more level
                return parent.parent().map(|p| p.to_path_buf());
            }
        }

        Some(parent.to_path_buf())
    }

    /// Check if a workspace directory exists for a given change ID
    pub fn has_workspace(&self, change_id: &str) -> bool {
        if let Some(workspaces_dir) = self.workspaces_dir() {
            let ws_path = workspaces_dir.join(format!("ws-{}", change_id));
            ws_path.is_dir()
        } else {
            false
        }
    }

    /// Get the workspace name for a change ID if it exists
    pub fn workspace_name(&self, change_id: &str) -> Option<String> {
        if self.has_workspace(change_id) {
            Some(format!("ws-{}", change_id))
        } else {
            None
        }
    }

    /// Begin a transaction for atomic multi-event writes
    pub fn begin_transaction(&self, change_id: &str) -> Result<Transaction> {
        match &self.mode {
            StoreMode::FileSystem { .. } => {
                // Migrate from flat to sharded if needed
                self.migrate_to_sharded(change_id)?;

                // Get the sharded path and ensure directory exists
                let path = self.change_path(change_id);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).with_context(|| {
                        format!("failed to create directory {}", parent.display())
                    })?;
                }

                Transaction::new_filesystem(path)
            }
            StoreMode::StateBranch { .. } => Transaction::new_state_branch(change_id.to_string()),
        }
    }

    /// Recover a potentially corrupted change file
    /// Returns the number of events that were recovered (excluding corrupted lines)
    pub fn recover_change(&self, id: &str) -> Result<RecoveryResult> {
        let path = self.find_change_path(id).ok_or_else(|| {
            anyhow!(
                "change not found: '{}'\n\
                 Run 'docket list' to see all changes.",
                id
            )
        })?;

        recover_file(&path)
    }

    // ========================================================================
    // Release Methods
    // ========================================================================

    /// Path to the releases directory
    fn releases_dir(&self) -> PathBuf {
        self.root.join(RELEASES_DIR)
    }

    /// Ensure the releases directory exists
    fn ensure_releases_dir(&self) -> Result<()> {
        let dir = self.releases_dir();
        if !dir.exists() {
            fs::create_dir_all(&dir)
                .with_context(|| format!("failed to create {}", dir.display()))?;
        }
        Ok(())
    }

    /// Path to a specific release's event log file
    fn release_path(&self, version: &str) -> PathBuf {
        self.releases_dir().join(format!("{}.jsonl", version))
    }

    /// Ensure the special "unscheduled" release exists
    pub fn ensure_unscheduled_release(&self) -> Result<()> {
        self.ensure_releases_dir()?;

        let path = self.release_path(UNSCHEDULED_RELEASE);
        if path.exists() {
            return Ok(());
        }

        let event = ReleaseEvent::created(
            UNSCHEDULED_RELEASE.to_string(),
            Some("Unscheduled".to_string()),
            "Backlog items not yet assigned to a release.".to_string(),
            None,
        );

        append_release_event(&path, &event)?;

        Ok(())
    }

    /// List all releases
    pub fn list_releases(&self) -> Result<Vec<Release>> {
        self.ensure_releases_dir()?;
        self.ensure_unscheduled_release()?;

        let releases_dir = self.releases_dir();
        let pattern = releases_dir.join("*.jsonl");
        let pattern_str = pattern
            .to_str()
            .ok_or_else(|| anyhow!("invalid path encoding"))?;

        let mut releases = Vec::new();

        for entry in glob::glob(pattern_str)? {
            let path = entry?;
            let events = read_release_events(&path)?;

            match derive_release(&events) {
                Ok(release) => releases.push(release),
                Err(e) => {
                    eprintln!(
                        "warning: failed to derive release from {}: {}",
                        path.display(),
                        e
                    );
                }
            }
        }

        // Sort by status (active first), then by version
        releases.sort_by(|a, b| {
            let status_cmp = a.status().cmp(b.status());
            if status_cmp == std::cmp::Ordering::Equal {
                // Compare by semver if both are valid, otherwise alphabetically
                let a_ver = release::parse_version(a.version());
                let b_ver = release::parse_version(b.version());
                match (a_ver, b_ver) {
                    (Some(av), Some(bv)) => bv.cmp(&av), // Newer versions first
                    (Some(_), None) => std::cmp::Ordering::Less, // Semver before non-semver
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => a.version().cmp(b.version()),
                }
            } else {
                status_cmp
            }
        });

        Ok(releases)
    }

    /// Get a specific release by version
    pub fn get_release(&self, version: &str) -> Result<Release> {
        self.ensure_releases_dir()?;

        // Handle the unscheduled release specially
        if version == UNSCHEDULED_RELEASE {
            self.ensure_unscheduled_release()?;
        }

        let path = self.release_path(version);

        if !path.exists() {
            return Err(anyhow!(
                "release not found: '{}'\n\
                 Run 'docket release list' to see all releases.",
                version
            ));
        }

        let events = read_release_events(&path)?;
        derive_release(&events)
    }

    /// Append an event to a release's event log
    pub fn append_release_event(&self, event: &ReleaseEvent) -> Result<()> {
        self.ensure_releases_dir()?;

        let path = self.release_path(&event.release_version);
        append_release_event(&path, event)
    }

    /// Check if a release exists
    pub fn release_exists(&self, version: &str) -> bool {
        let path = self.release_path(version);
        path.exists()
    }

    /// Get all events for a release
    pub fn get_release_events(&self, version: &str) -> Result<Vec<ReleaseEvent>> {
        let path = self.release_path(version);
        if !path.exists() {
            return Err(anyhow!(
                "release not found: '{}'\n\
                 Run 'docket release list' to see all releases.",
                version
            ));
        }
        read_release_events(&path)
    }

    /// Get changes for a specific release
    pub fn get_changes_for_release(&self, version: &str) -> Result<Vec<Change>> {
        let changes = self.list_changes()?;
        Ok(changes
            .into_iter()
            .filter(|c| c.target_release() == version)
            .collect())
    }

    /// Count changes by status for a release (returns (completed, total))
    pub fn release_progress(&self, version: &str) -> Result<(usize, usize)> {
        let changes = self.get_changes_for_release(version)?;
        let completed = changes
            .iter()
            .filter(|c| matches!(c.status(), crate::change::Status::Done))
            .count();
        Ok((completed, changes.len()))
    }
}

/// Result of a file recovery operation
#[derive(Debug)]
pub struct RecoveryResult {
    /// Number of valid events recovered
    pub valid_events: usize,
    /// Number of corrupted lines that were removed
    pub corrupted_lines: usize,
    /// Whether the file was modified
    pub file_modified: bool,
}

/// Transaction mode for different storage backends
#[derive(Debug, Clone)]
enum TransactionMode {
    /// Filesystem mode: use temp file + atomic rename
    FileSystem { path: PathBuf },
    /// State branch mode: write to jj state branch
    StateBranch { change_id: String },
}

/// A transaction for atomic multi-event writes
///
/// Collects events and writes them atomically. The commit strategy depends on the mode:
/// - FileSystem: uses temp file + rename pattern
/// - StateBranch: reads current content, appends events, writes atomically via jj
///
/// This ensures that either all events are written or none are, preventing
/// inconsistent state from crashes between event writes.
pub struct Transaction {
    /// Transaction mode (determines commit strategy)
    mode: TransactionMode,
    /// Events to append
    events: Vec<Event>,
    /// Whether to call fsync for durability (filesystem mode only)
    fsync: bool,
}

impl Transaction {
    fn new_filesystem(path: PathBuf) -> Result<Self> {
        Ok(Transaction {
            mode: TransactionMode::FileSystem { path },
            events: Vec::new(),
            fsync: false,
        })
    }

    fn new_state_branch(change_id: String) -> Result<Self> {
        Ok(Transaction {
            mode: TransactionMode::StateBranch { change_id },
            events: Vec::new(),
            fsync: false,
        })
    }

    /// Add an event to the transaction
    pub fn add_event(&mut self, event: Event) {
        self.events.push(event);
    }

    /// Enable fsync for durability guarantees
    /// When enabled, the transaction will call fsync on the file before renaming
    pub fn with_fsync(mut self, fsync: bool) -> Self {
        self.fsync = fsync;
        self
    }

    /// Check if the transaction has any events
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Get the number of events in the transaction
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Commit the transaction atomically
    ///
    /// For FileSystem mode: writes all events to a temp file, optionally calls fsync,
    /// then atomically renames the temp file over the original.
    /// For StateBranch mode: reads current content, appends events, writes atomically via jj.
    ///
    /// If the original file exists, its contents are preserved and new events appended.
    pub fn commit(self) -> Result<()> {
        if self.events.is_empty() {
            return Ok(());
        }

        match self.mode {
            TransactionMode::FileSystem { path } => {
                Self::commit_filesystem(path, self.events, self.fsync)
            }
            TransactionMode::StateBranch { change_id } => {
                Self::commit_state_branch(change_id, self.events)
            }
        }
    }

    /// Commit using filesystem (temp file + atomic rename)
    fn commit_filesystem(path: PathBuf, events: Vec<Event>, fsync: bool) -> Result<()> {
        // Read existing events if file exists
        let mut existing_content = String::new();
        if path.exists() {
            existing_content = fs::read_to_string(&path).with_context(|| {
                format!("failed to read existing events from {}", path.display())
            })?;
        }

        // Create temp file in same directory (required for atomic rename)
        let parent = path
            .parent()
            .ok_or_else(|| anyhow!("change path has no parent directory: {}", path.display()))?;

        let temp_path = parent.join(format!(
            ".{}.tmp.{}",
            path.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("events"),
            std::process::id()
        ));

        // Write existing content + new events to temp file
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temp_path)
            .with_context(|| format!("failed to create temp file {}", temp_path.display()))?;

        // Write existing content
        if !existing_content.is_empty() {
            write!(file, "{}", existing_content).with_context(|| {
                format!("failed to write existing events to {}", temp_path.display())
            })?;
            // Ensure existing content ends with newline
            if !existing_content.ends_with('\n') {
                writeln!(file)?;
            }
        }

        // Write new events
        for event in &events {
            let json = serde_json::to_string(event)?;
            writeln!(file, "{}", json)
                .with_context(|| format!("failed to write event to {}", temp_path.display()))?;
        }

        // Optionally fsync for durability
        if fsync {
            file.sync_all()
                .with_context(|| format!("failed to fsync {}", temp_path.display()))?;
        }

        // Close file handle before rename
        drop(file);

        // Atomic rename
        fs::rename(&temp_path, &path).with_context(|| {
            format!(
                "failed to rename {} to {}",
                temp_path.display(),
                path.display()
            )
        })?;

        // Optionally fsync the directory for extra durability
        if fsync {
            if let Ok(dir) = File::open(parent) {
                let _ = dir.sync_all();
            }
        }

        Ok(())
    }

    /// Commit using state branch (jj atomic write)
    fn commit_state_branch(change_id: String, events: Vec<Event>) -> Result<()> {
        // Build the path: .docket/changes/{first_char}/{id}.jsonl
        let shard = change_id
            .chars()
            .next()
            .ok_or_else(|| anyhow!("empty change ID"))?;
        let path = format!(".docket/changes/{}/{}.jsonl", shard, change_id);

        // Read current content (may not exist)
        let current = jj::read_state_file(&path).unwrap_or_default();

        // Build new content by appending all events
        let mut new_content = current.clone();

        // Ensure we start from a newline if there's existing content
        if !new_content.is_empty() && !new_content.ends_with('\n') {
            new_content.push('\n');
        }

        // Append all events
        for event in &events {
            let event_json = serde_json::to_string(event).context("failed to serialize event")?;
            new_content.push_str(&event_json);
            new_content.push('\n');
        }

        // Write atomically via jj
        let message = format!("docket: update {}", change_id);
        jj::write_to_state_branch(&path, &new_content, &message)?;

        Ok(())
    }
}

/// Recover a potentially corrupted JSONL file
///
/// Reads through the file line by line, keeping only valid JSON event lines.
/// Returns information about what was recovered.
fn recover_file(path: &Path) -> Result<RecoveryResult> {
    if !path.exists() {
        return Ok(RecoveryResult {
            valid_events: 0,
            corrupted_lines: 0,
            file_modified: false,
        });
    }

    let file = File::open(path)
        .with_context(|| format!("failed to open {} for recovery", path.display()))?;
    let reader = BufReader::new(file);

    let mut valid_events: Vec<Event> = Vec::new();
    let mut corrupted_lines = 0;

    for (line_num, line_result) in reader.lines().enumerate() {
        match line_result {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                match serde_json::from_str::<Event>(trimmed) {
                    Ok(event) => valid_events.push(event),
                    Err(_) => {
                        eprintln!(
                            "warning: corrupted event on line {} in {}: {}",
                            line_num + 1,
                            path.display(),
                            if trimmed.len() > 50 {
                                format!("{}...", &trimmed[..50])
                            } else {
                                trimmed.to_string()
                            }
                        );
                        corrupted_lines += 1;
                    }
                }
            }
            Err(e) => {
                eprintln!(
                    "warning: failed to read line {} in {}: {}",
                    line_num + 1,
                    path.display(),
                    e
                );
                corrupted_lines += 1;
            }
        }
    }

    // Only rewrite the file if there were corrupted lines
    let file_modified = corrupted_lines > 0;

    if file_modified {
        // Use the same atomic write pattern
        let parent = path
            .parent()
            .ok_or_else(|| anyhow!("change path has no parent directory: {}", path.display()))?;

        let temp_path = parent.join(format!(
            ".{}.recovery.{}",
            path.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("events"),
            std::process::id()
        ));

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temp_path)
            .with_context(|| {
                format!(
                    "failed to create recovery temp file {}",
                    temp_path.display()
                )
            })?;

        for event in &valid_events {
            let json = serde_json::to_string(event)?;
            writeln!(file, "{}", json)?;
        }

        file.sync_all()?;
        drop(file);

        fs::rename(&temp_path, path)?;
    }

    Ok(RecoveryResult {
        valid_events: valid_events.len(),
        corrupted_lines,
        file_modified,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::change::Priority;
    use crate::event::Event;

    #[test]
    fn transaction_atomic_write_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");

        let mut tx = Transaction::new_filesystem(path.clone()).unwrap();

        let event1 = Event::created(
            "abc1".to_string(),
            "Test Change".to_string(),
            Priority::Medium,
            "Body".to_string(),
        );
        let event2 = Event::priority_changed("abc1".to_string(), Priority::Medium, Priority::High);

        tx.add_event(event1);
        tx.add_event(event2);

        assert_eq!(tx.len(), 2);
        assert!(!tx.is_empty());

        tx.commit().unwrap();

        // Verify the file was created with both events
        let events = event::read_events(&path).unwrap();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn transaction_atomic_append_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");

        // Write initial event directly
        let initial_event = Event::created(
            "abc1".to_string(),
            "Test Change".to_string(),
            Priority::Medium,
            "Body".to_string(),
        );
        event::append_event(&path, &initial_event).unwrap();

        // Use transaction to append more events
        let mut tx = Transaction::new_filesystem(path.clone()).unwrap();

        let event1 = Event::priority_changed("abc1".to_string(), Priority::Medium, Priority::High);
        let event2 = Event::updated("abc1".to_string(), Some("New Title".to_string()), None);

        tx.add_event(event1);
        tx.add_event(event2);

        tx.commit().unwrap();

        // Verify all 3 events are present
        let events = event::read_events(&path).unwrap();
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn transaction_empty_commit_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");

        // Write initial event
        let initial_event = Event::created(
            "abc1".to_string(),
            "Test Change".to_string(),
            Priority::Medium,
            "Body".to_string(),
        );
        event::append_event(&path, &initial_event).unwrap();

        // Get initial file content
        let initial_content = fs::read_to_string(&path).unwrap();

        // Empty transaction commit
        let tx = Transaction::new_filesystem(path.clone()).unwrap();
        assert!(tx.is_empty());
        tx.commit().unwrap();

        // File should be unchanged
        let final_content = fs::read_to_string(&path).unwrap();
        assert_eq!(initial_content, final_content);
    }

    #[test]
    fn transaction_with_fsync() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");

        let mut tx = Transaction::new_filesystem(path.clone())
            .unwrap()
            .with_fsync(true);

        let event = Event::created(
            "abc1".to_string(),
            "Test Change".to_string(),
            Priority::Medium,
            "Body".to_string(),
        );

        tx.add_event(event);
        tx.commit().unwrap();

        // Verify file exists and has the event
        let events = event::read_events(&path).unwrap();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn recover_file_no_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");

        // Write valid events
        let event1 = Event::created(
            "abc1".to_string(),
            "Test Change".to_string(),
            Priority::Medium,
            "Body".to_string(),
        );
        let event2 = Event::priority_changed("abc1".to_string(), Priority::Medium, Priority::High);
        event::append_event(&path, &event1).unwrap();
        event::append_event(&path, &event2).unwrap();

        let result = recover_file(&path).unwrap();

        assert_eq!(result.valid_events, 2);
        assert_eq!(result.corrupted_lines, 0);
        assert!(!result.file_modified);
    }

    #[test]
    fn recover_file_with_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");

        // Write a valid event
        let event = Event::created(
            "abc1".to_string(),
            "Test Change".to_string(),
            Priority::Medium,
            "Body".to_string(),
        );
        event::append_event(&path, &event).unwrap();

        // Append corrupted data
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(file, "{{invalid json").unwrap();
        writeln!(file, "truncated").unwrap();
        drop(file);

        let result = recover_file(&path).unwrap();

        assert_eq!(result.valid_events, 1);
        assert_eq!(result.corrupted_lines, 2);
        assert!(result.file_modified);

        // Verify file now only has the valid event
        let events = event::read_events(&path).unwrap();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn recover_file_nonexistent() {
        let path = PathBuf::from("/nonexistent/path/test.jsonl");

        let result = recover_file(&path).unwrap();

        assert_eq!(result.valid_events, 0);
        assert_eq!(result.corrupted_lines, 0);
        assert!(!result.file_modified);
    }

    #[test]
    fn transaction_preserves_trailing_newline() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");

        // Write initial event
        let initial_event = Event::created(
            "abc1".to_string(),
            "Test Change".to_string(),
            Priority::Medium,
            "Body".to_string(),
        );
        event::append_event(&path, &initial_event).unwrap();

        // Append via transaction
        let mut tx = Transaction::new_filesystem(path.clone()).unwrap();
        tx.add_event(Event::updated(
            "abc1".to_string(),
            Some("New Title".to_string()),
            None,
        ));
        tx.commit().unwrap();

        // Read the raw file and check for proper newlines
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.ends_with('\n'));
        // Each line should be valid JSON
        for line in content.lines() {
            if !line.trim().is_empty() {
                serde_json::from_str::<Event>(line).unwrap();
            }
        }
    }

    // ========================================================================
    // StoreMode Detection Tests
    // ========================================================================

    #[test]
    fn store_mode_filesystem_on_init() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::init_at(dir.path()).unwrap();

        assert!(store.is_filesystem_mode());
        assert!(!store.is_state_branch_mode());

        match &store.mode {
            StoreMode::FileSystem { root } => {
                assert_eq!(root, &dir.path().join(DOCKET_DIR));
            }
            StoreMode::StateBranch { .. } => {
                panic!("Expected FileSystem mode");
            }
        }
    }

    #[test]
    fn store_mode_filesystem_on_open() {
        let dir = tempfile::tempdir().unwrap();

        // Initialize a store
        Store::init_at(dir.path()).unwrap();

        // Open it again
        let store = Store::open_from(dir.path()).unwrap();

        assert!(store.is_filesystem_mode());
        assert!(!store.is_state_branch_mode());
    }

    #[test]
    fn store_mode_filesystem_from_subdirectory() {
        let dir = tempfile::tempdir().unwrap();

        // Initialize a store
        Store::init_at(dir.path()).unwrap();

        // Create a subdirectory and open from there
        let subdir = dir.path().join("src").join("deeply").join("nested");
        fs::create_dir_all(&subdir).unwrap();

        let store = Store::open_from(&subdir).unwrap();

        assert!(store.is_filesystem_mode());
        assert_eq!(store.root(), dir.path().join(DOCKET_DIR));
    }

    #[test]
    fn store_mode_enum_equality() {
        let root = PathBuf::from("/test/path/.docket");
        let mode1 = StoreMode::FileSystem { root: root.clone() };
        let mode2 = StoreMode::FileSystem { root: root.clone() };
        let mode3 = StoreMode::FileSystem {
            root: PathBuf::from("/other/path/.docket"),
        };
        let mode4 = StoreMode::StateBranch {
            repo_root: PathBuf::from("/test/path"),
        };

        assert_eq!(mode1, mode2);
        assert_ne!(mode1, mode3);
        assert_ne!(mode1, mode4);
    }

    #[test]
    fn store_mode_clone() {
        let root = PathBuf::from("/test/path/.docket");
        let mode = StoreMode::FileSystem { root };
        let cloned = mode.clone();

        assert_eq!(mode, cloned);
    }

    #[test]
    fn store_mode_debug() {
        let mode = StoreMode::FileSystem {
            root: PathBuf::from("/test"),
        };
        let debug_str = format!("{:?}", mode);
        assert!(debug_str.contains("FileSystem"));
        assert!(debug_str.contains("/test"));
    }

    #[test]
    fn open_fails_without_docket_dir() {
        let dir = tempfile::tempdir().unwrap();

        // Don't initialize - should fail to open
        let result = Store::open_from(dir.path());

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not a docket repository"));
    }

    #[test]
    fn state_branch_name_constant() {
        // Verify the constant is set correctly
        assert_eq!(STATE_BRANCH_NAME, "docket-state");
    }
}
