# Docket - Developer Guide for Claude

This document provides context for working on the docket codebase.

## Architecture Overview

Docket is a Rust CLI application using **event sourcing** for persistence. All state is derived from immutable event logs.

```
src/
├── main.rs         # CLI entry point (clap derive)
├── lib.rs          # Module exports
├── bug.rs          # Data types: Status, Priority, Bug, BugMetadata
├── event.rs        # Event system: Event, EventData, derive_bug()
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
        ├── transitions.rs  # approve, start
        ├── done.rs
        └── jj.rs          # Jujutsu helpers
```

## Event System

The core persistence model. Events are appended to JSONL files in `.docket/bugs/{id}.jsonl`.

### Event Types (event.rs)

```rust
pub enum EventData {
    Created { title, priority, body },
    StatusChanged { from, to },
    Updated { title: Option, body: Option },
    PriorityChanged { from, to },
    ChangeLinked { change_id },  // Deprecated, ignored
}
```

### Key Functions

- `append_event(path, event)` - Append JSON line to file
- `read_events(path)` - Read and sort events by timestamp
- `derive_bug(events)` - Replay events to compute current Bug state

### Data Flow

1. Command validates preconditions
2. Command creates Event with timestamp and UUID
3. Event appended to JSONL file via `store.append_event()`
4. Current state derived by replaying all events

## Status Flow

```
Draft → Approved → InProgress → Done
```

- `new` creates bugs in Draft
- `approve` transitions Draft → Approved
- `start` or `work` transitions Approved → InProgress
- `done` transitions any → Done

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
println!("{} Bug created: {}", "✓".green(), id);
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
| `.docket/bugs/*.jsonl` | Event logs (one per bug) |
| `.docket/config.toml` | User configuration |
| `ws-{id}/` | Jujutsu workspaces for active work |

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
2. Update `derive_bug()` to handle it
3. Create command that emits the event

### Working on a Bug

The `work` command sets `DOCKET_BUG` env var. Access it:

```bash
cargo run -- show $DOCKET_BUG
cargo run -- update $DOCKET_BUG
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
