# Docket Implementation Guide

You are helping implement a bug tracked by docket. Read the current bug context and guide the implementation work.

## Instructions

1. **Get the bug context** by running `cargo run -- show $DOCKET_BUG` to understand what needs to be implemented. This will display:
   - The bug title and ID
   - Status and priority
   - The body containing Goal, Acceptance Criteria, Context, and Log sections

2. **Understand the requirements** by carefully reading:
   - The **Goal** section to understand what needs to be accomplished
   - The **Acceptance Criteria** to know exactly what must be delivered
   - The **Context** section for background information and constraints

3. **Plan and implement** the work:
   - Create a todo list to track progress through the acceptance criteria
   - Work through each acceptance criterion systematically
   - Follow best practices for the codebase

4. **Run clippy before finishing**:
   - Run `cargo clippy -- -D warnings` to check for lints
   - Fix any warnings that clippy reports
   - This ensures CI won't fail due to clippy issues

5. **Update progress** as you work:
   - Use `cargo run -- update $DOCKET_BUG` to update the bug body with progress
   - Add timestamped entries to the Log section describing what was accomplished
   - Format log entries as: `- YYYY-MM-DD: Description of progress`
   - Check off acceptance criteria by changing `- [ ]` to `- [x]`

5. **Set changelog type** after completing work:
   - Check if the bug already has a changelog type set (shown in `cargo run -- show`)
   - If not set, ask the user: "What type of change is this for the changelog?"
   - Options: feature, fix, change, deprecated, removed, security, internal
   - Set it via: `cargo run -- update $DOCKET_BUG --changelog TYPE`
   - Use `internal` for changes that shouldn't appear in public changelogs

## Changelog Type Guide

| Type | When to use |
|------|-------------|
| feature | New functionality or capabilities |
| fix | Bug fixes |
| change | Changes to existing functionality |
| deprecated | Features that will be removed |
| removed | Features that were removed |
| security | Security-related fixes |
| internal | Refactoring, tests, docs, tooling (not in changelog) |

## Getting Started

Begin by running `cargo run -- show $DOCKET_BUG` and presenting a summary of the bug to the user, then propose an implementation plan.
