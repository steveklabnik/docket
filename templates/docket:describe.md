# Generate Commit Description

Generate a commit message for the current changes following Google's CL description best practices.

## Instructions

1. Run `jj show --git` to see the current changes (files modified and diff)
2. Run `docket show $DOCKET_BUG` to get the bug title and description for context

## Commit Message Format

**First line**: Short imperative summary of WHAT changed
- Use imperative mood: "Add", "Fix", "Remove" (not "Added", "Fixing", "Removed")
- Should be searchable and stand alone
- Keep it concise but descriptive

**Blank line**

**Body**: Explain WHY this change was made
- The problem being solved
- Why this approach was chosen
- Any context a future reader would need
- Reference the bug ID naturally

## Guidelines

- The body explains reasoning, not just restates the diff
- Future developers should understand whether they can safely modify this code
- This will be a permanent part of version control history

## Output

Output ONLY the commit message text. No markdown formatting, no code blocks, no explanation - just the raw commit message ready to be passed to `jj describe`.
