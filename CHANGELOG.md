# Changelog

All notable changes to docket will be documented in this file.

## [0.1.0] - 2026-01-14

Initial release of docket - task tracking for AI-assisted development.

### Core Features

- **Event-sourced persistence** - Bugs stored as append-only JSONL event logs in `.docket/bugs/`. Enables conflict-free merging across jj workspaces.
- **Status workflow** - `Draft → Approved → InProgress → Done` (plus `NotPlanned` for closed bugs)
- **Jujutsu integration** - Workspace management, automatic snapshots, and commit message generation

### Commands

| Command | Description |
|---------|-------------|
| `init` | Initialize a new docket repository |
| `new` | Create a new bug (supports `--body` for full body at creation) |
| `list` | List bugs with filtering by status and priority |
| `show` | Display bug details and acceptance criteria |
| `update` | Modify title, body, priority, or status |
| `approve` | Transition bug from Draft to Approved |
| `done` | Complete a bug with automatic workspace handling |
| `work` | Start working: creates jj workspace and launches Claude |
| `log` | View event history for a bug (supports `--json`) |
| `cleanup` | Remove workspace after work is complete |

### Claude Code Integration

- **`/docket:implement` skill** - Guides Claude through bug implementation
- **`/docket:describe` skill** - Generates commit messages from bug context
- **`docket work`** launches Claude with:
  - `DOCKET_BUG` environment variable set
  - Optional `--skip-permissions` and `--auto` flags
  - Configurable via `.docket/config.toml`

### Workflow Automation

- `docket done` automatically:
  - Detects and switches to workspace if run from trunk
  - Snapshots uncommitted changes with `jj`
  - Generates commit message from bug title
  - Creates a fresh change for next work (if not in workspace)
- `docket cleanup` (no args) cleans up workspaces for completed bugs

### Installation

```bash
# Pre-built binaries (recommended)
cargo binstall --git https://github.com/steveklabnik/docket docket

# From source
cargo install --git https://github.com/steveklabnik/docket
```

Note: The `docket` name on crates.io is taken by an unrelated project.

### Infrastructure

- GitHub Actions CI (build, test, clippy, fmt)
- Release workflow for multi-platform binaries (Linux, macOS, Windows)
- Comprehensive test suite (unit + integration)
