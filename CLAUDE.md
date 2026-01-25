# Docket - Developer Guide for Claude

This document provides context for working on the docket codebase.

## Terminology

Docket tracks **changes** - units of work in a DAG (Directed Acyclic Graph). A "change" can be anything from a small bug fix to a large initiative. Changes can have children (sub-changes) and dependencies (blocked by other changes).

## Architecture Overview

Docket is a Rust CLI application using **event sourcing** for persistence. All state is derived from immutable event logs.

```
src/
├── main.rs         # CLI entry point (clap derive)
├── lib.rs          # Module exports
├── change.rs       # Data types: Status, Priority, Change, ChangeMetadata
├── event.rs        # Event system: Event, EventData, derive_change()
├── store.rs        # Storage: find repo, read/write events
├── config.rs       # Configuration loading
└── commands/       # One module per CLI command
    ├── mod.rs
    ├── init.rs
    ├── new.rs
    ├── list.rs
    ├── show.rs
    ├── update.rs
    ├── log.rs
    ├── work.rs
    ├── cleanup.rs
    └── status/
        ├── mod.rs
        ├── transitions.rs  # approve
        ├── done.rs
        └── jj.rs          # Jujutsu helpers
```

## Event System

The core persistence model. Events are appended to JSONL files in `.docket/changes/{id}.jsonl`.

### Event Types (event.rs)

```rust
pub enum EventData {
    Created { title, priority, body, parent },
    StatusChanged { from, to },
    Updated { title: Option, body: Option },
    PriorityChanged { from, to },
    DependencyAdded { blocked_by },
    DependencyRemoved { blocked_by },
    ScratchpadAppended { content },
    ParentChanged { old_parent, new_parent },
}
```

### Key Functions

- `append_event(path, event)` - Append JSON line to file
- `read_events(path)` - Read and sort events by timestamp
- `derive_change(events)` - Replay events to compute current change state

### Data Flow

1. Command validates preconditions
2. Command creates Event with timestamp and UUID
3. Event appended to JSONL file via `store.append_event()`
4. Current state derived by replaying all events

## Status Flow

```
Draft → Approved → InProgress → Review → Done
                      ↓           ↓
                   Blocked    (reject back to InProgress)
                      ↓
                   Paused
```

- `new` creates changes in Draft
- `approve` transitions Draft → Approved (auto-approves children)
- `work` transitions Approved → InProgress
- `review` transitions InProgress → Review
- `reject` transitions Review → InProgress
- `done` transitions any → Done (auto-completes parent when all children done)

## Code Conventions

### Error Handling

Uses `anyhow::Result<T>` with context:

```rust
fn example() -> Result<()> {
    do_thing().context("failed to do thing")?;
    Ok(())
}
```

### Output

Uses `colored` crate. Common patterns:

```rust
println!("{} Created change: {}", "✓".green(), id);
println!("{} Starting work on {}", "→".blue(), title);
println!("{} Warning: uncommitted changes", "!".yellow());
```

### ID Handling

- 4-character random alphanumeric IDs
- Prefix matching supported: `abc` matches `abc1`
- Generated in `store.rs:generate_id()`

### Command Structure

Each command in `commands/`:
- Takes parsed CLI args
- Opens store with `Store::open()?`
- Validates preconditions
- Creates and appends events
- Returns `Result<()>`

## Key Files

| Location | Purpose |
|----------|---------|
| `.docket/changes/*.jsonl` | Event logs (one per change) |
| `.docket/config.toml` | User configuration |
| `ws-{id}/` | Jujutsu workspaces for active work |
| `docs/schema-versioning.md` | Event schema versioning policy |

## Testing

```bash
cargo test
```

- `tests/integration_tests.rs` - End-to-end command tests
- `tests/fixture_tests.rs` - Event replay with JSONL fixtures
- Unit tests in `event.rs`

## Common Tasks

### Adding a New Command

1. Create `src/commands/newcmd.rs`
2. Add module to `src/commands/mod.rs`
3. Add subcommand to CLI enum in `main.rs`
4. Add match arm to run the command

### Adding a New Event Type

1. Add variant to `EventData` in `event.rs`
2. Update `derive_change()` to handle it
3. Create command that emits the event

### Working on a Change

The `work` command creates a workspace directory (ws-{id}). Use the `current` command to get the current change ID when inside a workspace:

```bash
cargo run -- current
cargo run -- show $(cargo run -- current)
```

## Dependencies

- `clap` - CLI parsing
- `serde`/`serde_json` - Serialization
- `chrono` - Timestamps
- `colored` - Terminal colors
- `dialoguer` - Interactive prompts
- `anyhow` - Error handling
- `uuid`, `rand` - ID generation
- `glob` - File patterns
- `tempfile` - Testing
