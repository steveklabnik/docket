# Generate Commit Description

Generate a commit message for the current changes following Google's CL description best practices.

## Instructions

1. Run `jj show --git` to see the current changes (files modified and diff)
2. Run `cargo run -- show $DOCKET_BUG` to get the bug title, description, and changelog type for context

## Commit Message Format

**First line**: Short imperative summary of WHAT changed
- Use imperative mood: "Add", "Fix", "Remove" (not "Added", "Fixing", "Removed")
- Should be searchable and stand alone
- Keep it concise but descriptive
- If the bug has a changelog type, use it to inform the verb:
  - feature → "Add ..."
  - fix → "Fix ..."
  - change → "Update ...", "Improve ...", "Refactor ..."
  - removed → "Remove ..."
  - security → "Fix ..." (mention security aspect)

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

Wrap your commit message in `<commit>` tags:

```
<commit>
Your commit message here
</commit>
```

Output ONLY the commit message inside the tags. No preamble, no explanation, no phrases like "Here's the commit message:" - just the raw commit message text.

DO NOT include text like:
- "Based on the changes..."
- "Here's the commit message:"
- "This commit implements..."

Start directly with the imperative verb (Add, Fix, Update, etc.).
