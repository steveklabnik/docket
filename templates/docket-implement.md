# Docket Implementation Guide

You are helping implement a bug tracked by docket. Read the current bug context and guide the implementation work.

## Instructions

1. **Get the bug context** by running `docket show $DOCKET_BUG` to understand what needs to be implemented. This will display:
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

4. **Update progress** as you work:
   - Use `docket update $DOCKET_BUG` to update the bug body with progress
   - Add timestamped entries to the Log section describing what was accomplished
   - Format log entries as: `- YYYY-MM-DD: Description of progress`
   - Check off acceptance criteria by changing `- [ ]` to `- [x]`

5. **Mark the bug as done** when all acceptance criteria are complete:
   - Use `docket done $DOCKET_BUG --auto` to mark the bug as done
   - The `--auto` flag verifies that all acceptance criteria checkboxes (`- [x]`) are checked
   - If any criteria remain unchecked, the command will fail with a helpful message
   - Use `--force` with `--auto` to override if you need to mark done despite unchecked criteria

## Getting Started

Begin by running `docket show $DOCKET_BUG` and presenting a summary of the bug to the user, then propose an implementation plan.
