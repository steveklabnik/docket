use anyhow::{anyhow, Context, Result};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use rand::Rng;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use crate::change::Change;
use crate::event::{self, Event};

const DOCKET_DIR: &str = ".docket";
const CHANGES_DIR: &str = "changes";
const LEGACY_BUGS_DIR: &str = "bugs";
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
                let store = Store { root: docket_path };
                // Migrate from legacy bugs/ directory if needed
                store.migrate_bugs_to_changes()?;
                return Ok(store);
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

        let changes_dir = root.join(CHANGES_DIR);
        fs::create_dir_all(&changes_dir)
            .with_context(|| format!("failed to create {}", changes_dir.display()))?;

        // Create .gitignore for cache directory
        let gitignore_path = root.join(".gitignore");
        fs::write(&gitignore_path, ".cache/\n")
            .with_context(|| format!("failed to create {}", gitignore_path.display()))?;

        Ok(Store { root })
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
        // First try exact match (checks sharded then flat)
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

    /// Get all events for a change
    pub fn get_events(&self, id: &str) -> Result<Vec<Event>> {
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
        // First try exact match (checks sharded then flat)
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

            // Check both sharded and flat paths to ensure uniqueness
            if self.find_change_path(&id).is_none() {
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
        // Migrate from flat to sharded if needed
        self.migrate_to_sharded(change_id)?;

        // Get the sharded path and ensure directory exists
        let path = self.change_path(change_id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }

        Transaction::new(path)
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

/// A transaction for atomic multi-event writes
///
/// Collects events and writes them atomically using a temp file + rename pattern.
/// This ensures that either all events are written or none are, preventing
/// inconsistent state from crashes between event writes.
pub struct Transaction {
    /// Path to the change's event log file
    path: PathBuf,
    /// Events to append
    events: Vec<Event>,
    /// Whether to call fsync for durability
    fsync: bool,
}

impl Transaction {
    fn new(path: PathBuf) -> Result<Self> {
        Ok(Transaction {
            path,
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
    /// This writes all events to a temp file, optionally calls fsync,
    /// then atomically renames the temp file over the original.
    /// If the original file exists, its contents are preserved and new events appended.
    pub fn commit(self) -> Result<()> {
        if self.events.is_empty() {
            return Ok(());
        }

        // Read existing events if file exists
        let mut existing_content = String::new();
        if self.path.exists() {
            existing_content = fs::read_to_string(&self.path).with_context(|| {
                format!(
                    "failed to read existing events from {}",
                    self.path.display()
                )
            })?;
        }

        // Create temp file in same directory (required for atomic rename)
        let parent = self.path.parent().ok_or_else(|| {
            anyhow!(
                "change path has no parent directory: {}",
                self.path.display()
            )
        })?;

        let temp_path = parent.join(format!(
            ".{}.tmp.{}",
            self.path
                .file_name()
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
        for event in &self.events {
            let json = serde_json::to_string(event)?;
            writeln!(file, "{}", json)
                .with_context(|| format!("failed to write event to {}", temp_path.display()))?;
        }

        // Optionally fsync for durability
        if self.fsync {
            file.sync_all()
                .with_context(|| format!("failed to fsync {}", temp_path.display()))?;
        }

        // Close file handle before rename
        drop(file);

        // Atomic rename
        fs::rename(&temp_path, &self.path).with_context(|| {
            format!(
                "failed to rename {} to {}",
                temp_path.display(),
                self.path.display()
            )
        })?;

        // Optionally fsync the directory for extra durability
        if self.fsync {
            if let Ok(dir) = File::open(parent) {
                let _ = dir.sync_all();
            }
        }

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

        let mut tx = Transaction::new(path.clone()).unwrap();

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
        let mut tx = Transaction::new(path.clone()).unwrap();

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
        let tx = Transaction::new(path.clone()).unwrap();
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

        let mut tx = Transaction::new(path.clone()).unwrap().with_fsync(true);

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
        let mut tx = Transaction::new(path.clone()).unwrap();
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
}
