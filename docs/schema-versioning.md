# Event Schema Versioning

Docket uses event sourcing for persistence, storing all state changes as immutable events in JSONL files. This document describes the versioning system that enables forward and backward compatibility as the event schema evolves.

## Current Version

The current event schema version is **1** (defined as `CURRENT_EVENT_VERSION` in `src/event.rs`).

## How Versioning Works

Every event includes a `version` field indicating which schema version it uses:

```json
{"version": 1, "id": "...", "bug_id": "...", "timestamp": "...", "type": "created", "data": {...}}
```

Events created before versioning was introduced (pre-0.2) don't have a `version` field. The parser treats these as version 1 for backward compatibility.

## Upgrade Policy

When making changes to the event schema, follow these guidelines:

### Non-Breaking Changes (No Version Bump)

The following changes are safe and don't require incrementing the version:

- **Adding new optional fields** with `#[serde(default)]` - old events without the field will use the default value
- **Adding new event types** - old events won't have these types, but the parser handles known types only
- **Adding new enum variants** to `EventData` - existing events won't use them

Example: Adding an optional `actor` field to events (already done).

### Breaking Changes (Version Bump Required)

The following changes require incrementing `CURRENT_EVENT_VERSION`:

- **Removing fields** that were previously required
- **Changing field types** in incompatible ways (e.g., `String` to `u32`)
- **Renaming fields** without providing aliases
- **Changing the structure** of event data significantly

When making breaking changes:

1. Increment `CURRENT_EVENT_VERSION` in `src/event.rs`
2. Update `derive_bug()` to handle both old and new versions
3. Document the change in this file under "Version History"
4. Consider providing a migration path (see below)

## Handling Deprecated Event Types

When an event type becomes obsolete:

1. **Mark it as deprecated** with a doc comment:
   ```rust
   /// Deprecated: [reason]. Kept for backwards compatibility.
   #[serde(rename = "old_name")]
   OldEventType { ... }
   ```

2. **Ignore it during replay** in `derive_bug()`:
   ```rust
   EventData::OldEventType { .. } => {
       // Deprecated: [reason], ignore
   }
   ```

3. **Never remove the variant** - existing event logs may contain these events and must remain parseable.

Example: `ChangeLinked` is deprecated and ignored during replay but remains in the enum for compatibility.

## Migration Strategy

Docket does **not** automatically migrate old event files. This is intentional:

- Event logs are append-only and immutable by design
- Migration would require rewriting files, risking data loss
- Old events remain valid through backward-compatible parsing

If a future version requires migration:

1. The migration will be opt-in via a command like `docket migrate`
2. Original files will be backed up before modification
3. Migration will be idempotent (safe to run multiple times)
4. The changelog will clearly document when migration is recommended

## Version History

### Version 1 (Current)

Initial versioned schema. Includes:

- Core event types: `Created`, `StatusChanged`, `Updated`, `PriorityChanged`
- Deprecated: `ChangeLinked` (ignored during replay)
- Extended event types: `Blocked`, `Unblocked`, `Paused`, `Resumed`
- Metadata events: `ChangelogTypeSet`, `VersionAdded`, `VersionRemoved`, `TagAdded`, `TagRemoved`
- Dependency events: `DependencyAdded`, `DependencyRemoved`
- Optional `actor` field for tracking which machine created an event
- Optional `is_epic` and `parent_epic` fields in `Created` events

## Best Practices for Future Changes

1. **Prefer additive changes** - add new optional fields rather than modifying existing ones
2. **Use serde attributes** - `#[serde(default)]`, `#[serde(skip_serializing_if)]`, and `#[serde(alias)]` provide flexibility
3. **Test backward compatibility** - add tests that parse events without new fields
4. **Document everything** - update this file when making schema changes
