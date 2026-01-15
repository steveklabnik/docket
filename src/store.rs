use anyhow::{anyhow, Context, Result};
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

    /// Path to a specific bug's event log file
    fn bug_path(&self, id: &str) -> PathBuf {
        self.bugs_dir().join(format!("{}.jsonl", id))
    }

    /// List all bugs
    pub fn list_bugs(&self) -> Result<Vec<Bug>> {
        let pattern = self.bugs_dir().join("*.jsonl");
        let pattern_str = pattern
            .to_str()
            .ok_or_else(|| anyhow!("invalid path encoding"))?;

        let mut bugs = Vec::new();

        for entry in glob::glob(pattern_str)? {
            let path = entry?;
            let events = event::read_events(&path)?;

            match event::derive_bug(&events) {
                Ok(bug) => bugs.push(bug),
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

    /// Get a specific bug by ID (supports prefix matching)
    pub fn get_bug(&self, id: &str) -> Result<Bug> {
        // First try exact match
        let exact_path = self.bug_path(id);
        if exact_path.exists() {
            let events = event::read_events(&exact_path)?;
            return event::derive_bug(&events);
        }

        // Try prefix match
        let pattern = self.bugs_dir().join(format!("{}*.jsonl", id));
        let pattern_str = pattern
            .to_str()
            .ok_or_else(|| anyhow!("invalid path encoding"))?;

        let matches: Vec<_> = glob::glob(pattern_str)?.collect::<Result<Vec<_>, _>>()?;

        match matches.len() {
            0 => Err(anyhow!(
                "bug not found: '{}'\n\
                 Run 'docket list' to see all bugs, or 'docket new' to create one.",
                id
            )),
            1 => {
                let events = event::read_events(&matches[0])?;
                event::derive_bug(&events)
            }
            _ => {
                let ids: Vec<_> = matches
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

    /// Append an event to a bug's event log
    pub fn append_event(&self, event: &Event) -> Result<()> {
        let path = self.bug_path(&event.bug_id);
        event::append_event(&path, event)
    }

    /// Get all events for a bug
    pub fn get_events(&self, id: &str) -> Result<Vec<Event>> {
        let path = self.bug_path(id);
        if !path.exists() {
            return Err(anyhow!(
                "bug not found: '{}'\n\
                 Run 'docket list' to see all bugs.",
                id
            ));
        }
        event::read_events(&path)
    }

    /// Resolve a bug ID prefix to the full ID
    pub fn resolve_id(&self, id: &str) -> Result<String> {
        // First try exact match
        let exact_path = self.bug_path(id);
        if exact_path.exists() {
            return Ok(id.to_string());
        }

        // Try prefix match
        let pattern = self.bugs_dir().join(format!("{}*.jsonl", id));
        let pattern_str = pattern
            .to_str()
            .ok_or_else(|| anyhow!("invalid path encoding"))?;

        let matches: Vec<_> = glob::glob(pattern_str)?.collect::<Result<Vec<_>, _>>()?;

        match matches.len() {
            0 => Err(anyhow!(
                "bug not found: '{}'\n\
                 Run 'docket list' to see all bugs.",
                id
            )),
            1 => {
                let full_id = matches[0]
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| anyhow!("invalid file name"))?;
                Ok(full_id.to_string())
            }
            _ => {
                let ids: Vec<_> = matches
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

            let path = self.bug_path(&id);
            if !path.exists() {
                return Ok(id);
            }
        }

        Err(anyhow!("failed to generate unique ID after 100 attempts"))
    }

    /// Generate the next child ID for an epic (e.g., abc1.1, abc1.2, abc1.3)
    pub fn generate_child_id(&self, parent_id: &str) -> Result<String> {
        // Find existing children to determine next number
        let pattern = self.bugs_dir().join(format!("{}.*.jsonl", parent_id));
        let pattern_str = pattern
            .to_str()
            .ok_or_else(|| anyhow!("invalid path encoding"))?;

        let mut max_num: u32 = 0;
        for path in glob::glob(pattern_str)?.flatten() {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                // Extract the number after the dot (e.g., "abc1.3" -> 3)
                if let Some(num_str) = stem.strip_prefix(&format!("{}.", parent_id)) {
                    if let Ok(num) = num_str.parse::<u32>() {
                        max_num = max_num.max(num);
                    }
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
