---
name: land-the-plane
description: Automated session wrap-up — close completed beads issues, commit via jj, and generate a resume prompt for the next agent. Use when ending a work session or when the user says "land the plane", "wrap up", or "end session".
---

You are closing out a work session. Execute every step below in order, fully automated — no stops unless something genuinely fails.

## Step 1: Audit Beads State

Run both commands and display a summary table:

```bash
bd list --status=in_progress
bd list --status=open
```

Show the user what's in flight and what's still open.

## Step 2: Close Completed Issues

Based on the conversation context and the audit above, identify which in_progress issues are actually done this session. Close them in one command:

```bash
bd close <id1> <id2> ...
```

If there are no completed issues, skip this step and note it.

## Step 3: Commit Changes

Invoke the `/commit` skill to inspect `jj diff`, group changes into appropriately scoped conventional commits, and advance the working copy. If there are no uncommitted changes, skip this step and note it.

## Step 4: Final Beads Snapshot

Run the audit again to confirm closed issues are gone and capture what remains:

```bash
bd list --status=open
bd list --status=in_progress
```

## Step 5: Generate Resume Prompt

Output a fenced block titled **Resume Prompt** that a fresh agent can copy-paste at the start of the next session. It must include:

- **What was accomplished** — summary of closed issues and commits from this session
- **Open issues** — beads IDs and titles of everything still in_progress or open
- **Blockers** — any known blockers or dependencies between open issues
- **First action** — the exact command to run first (`bd show <id>` for the highest-priority open issue, or `bd ready` if unclear)
- **Quality gate reminder** — a note to run `prek` before writing any new code

Format it as a copy-pasteable markdown block, not prose.
