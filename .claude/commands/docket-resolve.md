# Resolve Merge Conflicts

Resolve jj merge conflicts in the current workspace after a rebase operation.

## Instructions

1. Run `jj status` to see which files have conflicts

2. For each conflicted file, read it and resolve the conflicts by editing

## jj Conflict Format

jj conflicts look like this:
```
<<<<<<< conflict N of M
+++++++ [destination commit info]
content from destination (what we rebased onto)
%%%%%%% diff from: [base] to: [our branch]
+lines we added
-lines we removed
>>>>>>> conflict N of M ends
```

**How to resolve**: The `+++++++` section shows what's in the destination (trunk). The `%%%%%%%` section shows what WE changed (as a diff). You need to COMBINE both:
- Keep the destination's content (`+++++++` section)
- ALSO apply our changes (the `+` lines from `%%%%%%%`)

For example, if destination added `pub mod edit;` and we added `pub mod completions;`, the resolution should have BOTH:
```
pub mod completions;
pub mod edit;
```

## Steps

1. Read each conflicted file
2. For each conflict block:
   - Look at what destination added (`+++++++` section)
   - Look at what we added (`+` lines in `%%%%%%%` section)
   - Combine both sets of changes
   - Remove ALL conflict markers
3. Run `jj status` to verify conflicts are resolved

## Output

Output a brief summary of what was resolved.
