# Docket

Task tracking for AI-assisted development.

Docket is a command-line bug/task tracker designed to integrate with [Claude Code](https://claude.ai/code) and [Jujutsu](https://martinvonz.github.io/jj/) version control. It manages work items through their lifecycle while automatically handling workspaces and commit messages.

## Installation

> **Note:** This project is *not* the `docket` crate on crates.io (that's an unrelated project).

### Using cargo-binstall (recommended)

If you have [cargo-binstall](https://github.com/cargo-bins/cargo-binstall) installed, you can download pre-built binaries:

```bash
cargo binstall --git https://github.com/steveklabnik/docket docket
```

### From source

```bash
cargo install --git https://github.com/steveklabnik/docket
```

### Manual build

```bash
git clone https://github.com/steveklabnik/docket
cd docket
cargo build --release
# Binary is at target/release/docket
```

## Quick Start

```bash
# Initialize docket in your project
docket init

# Create a new bug
docket new "Fix login validation"

# View all bugs
docket list

# Show bug details
docket show <id>

# Start working (creates workspace + launches Claude)
docket work <id>

# When done, mark complete
docket done <id>
```

## Workflow

Docket follows a simple status flow:

```
Draft → Approved → InProgress → Done
```

1. **Draft**: New bugs start here. Use for ideas and rough specs.
2. **Approved**: Bug is ready to be worked on. (`docket approve <id>`)
3. **InProgress**: Work has started. (`docket work <id>`)
4. **Done**: Work is complete. (`docket done <id>`)

### Working with Claude

The `work` command creates a Jujutsu workspace and launches Claude with the bug context:

```bash
docket work abc1
```

This:
- Creates a `ws-abc1` workspace (if needed)
- Sets `DOCKET_BUG=abc1` environment variable
- Launches Claude with the `/docket-implement` skill

Claude can then read the bug with `cargo run -- show $DOCKET_BUG` and update progress.

### Completing Work

When finished:

```bash
docket done abc1
```

This generates a commit message using Claude's `/docket-describe` skill and updates the Jujutsu change.

## Commands

| Command   | Description                                           |
|-----------|-------------------------------------------------------|
| `init`    | Initialize a new docket repository                    |
| `new`     | Create a new bug                                      |
| `list`    | List all bugs (use `--all` to include done)           |
| `show`    | Show details of a bug                                 |
| `update`  | Update a bug's title, body, or priority               |
| `approve` | Mark a bug as approved for work                       |
| `done`    | Mark a bug as done                                    |
| `work`    | Start working on a bug (creates workspace + Claude)   |
| `log`     | Show event history for a bug                          |
| `cleanup` | Clean up a workspace after work is complete           |
| `completions` | Generate shell completions                        |

Run `docket <command> --help` for detailed options.

## Shell Completions

Docket supports tab completion for bash, zsh, and fish shells. This enables completion of command names, options, and bug IDs.

### Bash

```bash
# Add to ~/.bashrc or generate once:
docket completions bash > ~/.local/share/bash-completion/completions/docket

# Or source directly:
eval "$(docket completions bash)"
```

### Zsh

```bash
# Add to your fpath (e.g., ~/.zfunc):
docket completions zsh > ~/.zfunc/_docket

# Make sure ~/.zfunc is in your fpath (add to ~/.zshrc):
fpath=(~/.zfunc $fpath)
autoload -Uz compinit && compinit
```

### Fish

```bash
docket completions fish > ~/.config/fish/completions/docket.fish
```

After installation, restart your shell or source your config file. You can then use TAB to complete commands, options, and bug IDs.

## Configuration

Create `.docket/config.toml` to customize behavior:

```toml
[work]
skip_permissions = true  # Skip Claude permission prompts
auto_implement = true    # Auto-run /docket-implement skill
```

## Example Workflow

```bash
# 1. Initialize docket in your project
docket init

# 2. Create a bug with a goal and acceptance criteria
docket new "Add user authentication"
# Opens editor to write the bug body

# 3. Review and approve when ready
docket list
docket approve a1b2

# 4. Start working (launches Claude in a workspace)
docket work a1b2

# 5. Claude reads the bug and implements it
# (Inside Claude session)
# cargo run -- show $DOCKET_BUG
# ... implement the feature ...
# cargo run -- update $DOCKET_BUG  # Update progress

# 6. When complete, mark done (generates commit message)
docket done a1b2

# 7. Clean up the workspace
docket cleanup a1b2
```

## Storage

Bugs are stored as JSONL event logs in `.docket/bugs/`, organized using directory sharding for scalability:

```
.docket/bugs/
├── a/
│   ├── abc1.jsonl
│   └── a9x2.jsonl
├── b/
│   └── bk47.jsonl
└── ...
```

The first character of each bug ID determines its shard directory. This structure:
- Reduces per-directory file count for large repositories
- Improves file system performance with thousands of bugs
- Becomes beneficial at ~500+ bugs (varies by file system)

Backward compatibility is maintained: bugs in the flat structure (`.docket/bugs/*.jsonl`) are automatically migrated to sharded directories when updated.

## Bug Format

The recommended body format:

```markdown
## Goal

What needs to be accomplished.

## Acceptance Criteria

- [ ] First requirement
- [ ] Second requirement

## Context

Background information and constraints.

## Log

- 2025-01-15: Started implementation
- 2025-01-15: Completed feature X
```

## License

MIT
