use anyhow::{anyhow, Context, Result};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use rand::Rng;
use std::fs;
use std::path::{Path, PathBuf};

use crate::bug::Bug;
use crate::event::{self, Event};

const DOCKET_DIR: &str = ".docket";
const BUGS_DIR: &str = "bugs";
const ID_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
const ID_LENGTH: usize = 4;

pub struct Store {
    root: PathBuf,
}

impl Store {
    /// Find .docket/ directory by walking up from current directory
    pub fn open() -> Result<Self> {
        let current = std::env::current_dir()?;
        Self::open_from(&current)
    }

    /// Find .docket/ directory by walking up from specified directory
    pub fn open_from(start: &Path) -> Result<Self> {
        let mut current = start.to_path_buf();

        loop {
            let docket_path = current.join(DOCKET_DIR);
            if docket_path.is_dir() {
                return Ok(Store { root: docket_path });
            }

            if !current.pop() {
                return Err(anyhow!(
                    "not a docket repository (or any parent): .docket directory not found.\n\
                     Run 'docket init' to initialize a new docket repository."
                ));
            }
        }
    }

    /// Initialize a new .docket/ directory in the current location
    pub fn init() -> Result<Self> {
        let current = std::env::current_dir()?;
        Self::init_at(&current)
    }

    /// Initialize a new .docket/ directory at specified location
    pub fn init_at(path: &Path) -> Result<Self> {
        let root = path.join(DOCKET_DIR);

        if root.exists() {
            return Err(anyhow!("docket already initialized at {}", root.display()));
        }

        let bugs_dir = root.join(BUGS_DIR);
        fs::create_dir_all(&bugs_dir)
            .with_context(|| format!("failed to create {}", bugs_dir.display()))?;

        // Create .gitignore for cache directory
        let gitignore_path = root.join(".gitignore");
        fs::write(&gitignore_path, ".cache/\n")
            .with_context(|| format!("failed to create {}", gitignore_path.display()))?;

        Ok(Store { root })
    }

    /// Path to the bugs directory
    fn bugs_dir(&self) -> PathBuf {
        self.root.join(BUGS_DIR)
    }

    /// Get the shard key (first character) for a bug ID
    fn shard_key(id: &str) -> Option<char> {
        id.chars().next()
    }

    /// Path to a specific bug's event log file (sharded structure)
    /// Returns the sharded path: .docket/bugs/{first_char}/{id}.jsonl
    fn bug_path(&self, id: &str) -> PathBuf {
        if let Some(shard) = Self::shard_key(id) {
            self.bugs_dir()
                .join(shard.to_string())
                .join(format!("{}.jsonl", id))
        } else {
            // Fallback for empty ID (shouldn't happen in practice)
            self.bugs_dir().join(format!("{}.jsonl", id))
        }
    }

    /// Path to the legacy flat location for a bug
    fn bug_path_flat(&self, id: &str) -> PathBuf {
        self.bugs_dir().join(format!("{}.jsonl", id))
    }

    /// Find the actual path where a bug file exists
    /// Checks sharded path first, then falls back to flat path for backward compatibility
    fn find_bug_path(&self, id: &str) -> Option<PathBuf> {
        let sharded = self.bug_path(id);
        if sharded.exists() {
            return Some(sharded);
        }
        let flat = self.bug_path_flat(id);
        if flat.exists() {
            return Some(flat);
        }
        None
    }

    /// Migrate a bug file from flat to sharded structure if needed
    fn migrate_to_sharded(&self, id: &str) -> Result<()> {
        let flat_path = self.bug_path_flat(id);
        if !flat_path.exists() {
            return Ok(()); // Nothing to migrate
        }

        let sharded_path = self.bug_path(id);
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

    /// List all bugs (searches both sharded and flat structures)
    pub fn list_bugs(&self) -> Result<Vec<Bug>> {
        let bugs_dir = self.bugs_dir();

        // Pattern for sharded structure: .docket/bugs/*/*.jsonl
        let sharded_pattern = bugs_dir.join("*").join("*.jsonl");
        let sharded_pattern_str = sharded_pattern
            .to_str()
            .ok_or_else(|| anyhow!("invalid path encoding"))?;

        // Pattern for flat structure (backward compat): .docket/bugs/*.jsonl
        let flat_pattern = bugs_dir.join("*.jsonl");
        let flat_pattern_str = flat_pattern
            .to_str()
            .ok_or_else(|| anyhow!("invalid path encoding"))?;

        let mut bugs = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();

        // Search sharded structure first
        for entry in glob::glob(sharded_pattern_str)? {
            let path = entry?;
            let events = event::read_events(&path)?;

            match event::derive_bug(&events) {
                Ok(bug) => {
                    seen_ids.insert(bug.id().to_string());
                    bugs.push(bug);
                }
                Err(e) => {
                    eprintln!(
                        "warning: failed to derive bug from {}: {}",
                        path.display(),
                        e
                    );
                }
            }
        }

        // Search flat structure for any bugs not yet migrated
        for entry in glob::glob(flat_pattern_str)? {
            let path = entry?;
            let events = event::read_events(&path)?;

            match event::derive_bug(&events) {
                Ok(bug) => {
                    // Only add if not already found in sharded structure
                    if !seen_ids.contains(bug.id()) {
                        bugs.push(bug);
                    }
                }
                Err(e) => {
                    eprintln!(
                        "warning: failed to derive bug from {}: {}",
                        path.display(),
                        e
                    );
                }
            }
        }

        // Sort by created date, newest first
        bugs.sort_by(|a, b| b.metadata.created.cmp(&a.metadata.created));

        Ok(bugs)
    }

    /// Get a specific bug by ID (supports prefix and fuzzy matching)
    pub fn get_bug(&self, id: &str) -> Result<Bug> {
        // First try exact match (checks sharded then flat)
        if let Some(path) = self.find_bug_path(id) {
            let events = event::read_events(&path)?;
            return event::derive_bug(&events);
        }

        // Try prefix match in both structures
        let prefix_matches = self.find_matching_bugs(id)?;

        match prefix_matches.len() {
            0 => {
                // No prefix matches, try fuzzy matching
                let fuzzy_matches = self.find_fuzzy_matches(id)?;
                match fuzzy_matches.len() {
                    0 => Err(anyhow!(
                        "bug not found: '{}'\n\
                         Run 'docket list' to see all bugs, or 'docket new' to create one.",
                        id
                    )),
                    1 => {
                        let path = self
                            .find_bug_path(&fuzzy_matches[0])
                            .ok_or_else(|| anyhow!("internal error: fuzzy match path not found"))?;
                        let events = event::read_events(&path)?;
                        event::derive_bug(&events)
                    }
                    _ => {
                        // Check if top matches have the same score (truly ambiguous)
                        // For now, just report ambiguity with all fuzzy matches
                        Err(anyhow!(
                            "ambiguous bug ID '{}', fuzzy matches: {}",
                            id,
                            fuzzy_matches.join(", ")
                        ))
                    }
                }
            }
            1 => {
                let events = event::read_events(&prefix_matches[0])?;
                event::derive_bug(&events)
            }
            _ => {
                let ids: Vec<_> = prefix_matches
                    .iter()
                    .filter_map(|p| p.file_stem())
                    .filter_map(|s| s.to_str())
                    .collect();
                Err(anyhow!(
                    "ambiguous bug ID '{}', matches: {}",
                    id,
                    ids.join(", ")
                ))
            }
        }
    }

    /// Find all bug files matching a prefix (searches both sharded and flat structures)
    fn find_matching_bugs(&self, prefix: &str) -> Result<Vec<PathBuf>> {
        let bugs_dir = self.bugs_dir();
        let mut matches = Vec::new();
        let mut seen_stems = std::collections::HashSet::new();

        // Search sharded structure: .docket/bugs/*/{prefix}*.jsonl
        let sharded_pattern = bugs_dir.join("*").join(format!("{}*.jsonl", prefix));
        if let Some(pattern_str) = sharded_pattern.to_str() {
            for entry in glob::glob(pattern_str)? {
                let path = entry?;
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    seen_stems.insert(stem.to_string());
                    matches.push(path);
                }
            }
        }

        // Search flat structure: .docket/bugs/{prefix}*.jsonl
        let flat_pattern = bugs_dir.join(format!("{}*.jsonl", prefix));
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

    /// Find all bug IDs in the repository (for fuzzy matching)
    fn list_all_bug_ids(&self) -> Result<Vec<String>> {
        let bugs_dir = self.bugs_dir();
        let mut ids = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // Search sharded structure: .docket/bugs/*/*.jsonl
        let sharded_pattern = bugs_dir.join("*").join("*.jsonl");
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

        // Search flat structure: .docket/bugs/*.jsonl
        let flat_pattern = bugs_dir.join("*.jsonl");
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

    /// Find bugs using fuzzy matching
    /// Returns bug IDs sorted by match score (best match first)
    fn find_fuzzy_matches(&self, query: &str) -> Result<Vec<String>> {
        let all_ids = self.list_all_bug_ids()?;
        let matcher = SkimMatcherV2::default();

        let mut scored: Vec<(String, i64)> = all_ids
            .into_iter()
            .filter_map(|id| matcher.fuzzy_match(&id, query).map(|score| (id, score)))
            .collect();

        // Sort by score descending (best matches first)
        scored.sort_by(|a, b| b.1.cmp(&a.1));

        Ok(scored.into_iter().map(|(id, _)| id).collect())
    }

    /// Append an event to a bug's event log
    /// If the bug exists in flat structure, migrates it to sharded first
    pub fn append_event(&self, event: &Event) -> Result<()> {
        // Migrate from flat to sharded if needed
        self.migrate_to_sharded(&event.bug_id)?;

        // Get the sharded path and ensure directory exists
        let path = self.bug_path(&event.bug_id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }

        event::append_event(&path, event)
    }

    /// Get all events for a bug
    pub fn get_events(&self, id: &str) -> Result<Vec<Event>> {
        let path = self.find_bug_path(id).ok_or_else(|| {
            anyhow!(
                "bug not found: '{}'\n\
                 Run 'docket list' to see all bugs.",
                id
            )
        })?;
        event::read_events(&path)
    }

    /// Resolve a bug ID prefix to the full ID (supports prefix and fuzzy matching)
    pub fn resolve_id(&self, id: &str) -> Result<String> {
        // First try exact match (checks sharded then flat)
        if self.find_bug_path(id).is_some() {
            return Ok(id.to_string());
        }

        // Try prefix match in both structures
        let prefix_matches = self.find_matching_bugs(id)?;

        match prefix_matches.len() {
            0 => {
                // No prefix matches, try fuzzy matching
                let fuzzy_matches = self.find_fuzzy_matches(id)?;
                match fuzzy_matches.len() {
                    0 => Err(anyhow!(
                        "bug not found: '{}'\n\
                         Run 'docket list' to see all bugs.",
                        id
                    )),
                    1 => Ok(fuzzy_matches[0].clone()),
                    _ => Err(anyhow!(
                        "ambiguous bug ID '{}', fuzzy matches: {}",
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
                    "ambiguous bug ID '{}', matches: {}",
                    id,
                    ids.join(", ")
                ))
            }
        }
    }

    /// Generate a unique bug ID
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

            // Check both sharded and flat paths to ensure uniqueness
            if self.find_bug_path(&id).is_none() {
                return Ok(id);
            }
        }

        Err(anyhow!("failed to generate unique ID after 100 attempts"))
    }

    /// Generate the next child ID for an epic (e.g., abc1.1, abc1.2, abc1.3)
    pub fn generate_child_id(&self, parent_id: &str) -> Result<String> {
        let bugs_dir = self.bugs_dir();

        // Find existing children in both sharded and flat structures
        let mut max_num: u32 = 0;

        // Helper to extract child number from path
        let extract_num = |path: &Path| -> Option<u32> {
            let stem = path.file_stem()?.to_str()?;
            let num_str = stem.strip_prefix(&format!("{}.", parent_id))?;
            num_str.parse::<u32>().ok()
        };

        // Search sharded structure: .docket/bugs/*/{parent_id}.*.jsonl
        let sharded_pattern = bugs_dir.join("*").join(format!("{}.*.jsonl", parent_id));
        if let Some(pattern_str) = sharded_pattern.to_str() {
            for path in glob::glob(pattern_str)?.flatten() {
                if let Some(num) = extract_num(&path) {
                    max_num = max_num.max(num);
                }
            }
        }

        // Search flat structure: .docket/bugs/{parent_id}.*.jsonl
        let flat_pattern = bugs_dir.join(format!("{}.*.jsonl", parent_id));
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

    /// Check if a workspace directory exists for a given bug ID
    pub fn has_workspace(&self, bug_id: &str) -> bool {
        if let Some(workspaces_dir) = self.workspaces_dir() {
            let ws_path = workspaces_dir.join(format!("ws-{}", bug_id));
            ws_path.is_dir()
        } else {
            false
        }
    }

    /// Get the workspace name for a bug ID if it exists
    pub fn workspace_name(&self, bug_id: &str) -> Option<String> {
        if self.has_workspace(bug_id) {
            Some(format!("ws-{}", bug_id))
        } else {
            None
        }
    }
}
