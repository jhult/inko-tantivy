# Agent Instructions

## Inko Syntax Guide

**IMPORTANT:** This project is written in Inko. Before writing code, fetch and read the Inko syntax guide.

**Base URL:** `https://raw.githubusercontent.com/jhult/inko-syntax-guide/trunk/`

**Required reading (fetch these files):**
1. `01-quick-reference.md` - **Critical syntax rules** (read first)
2. `12-gotchas.md` - Common mistakes that cause compile errors

**Additional references as needed:**
- `02-types-memory.md` - Types and memory management
- `03-methods-functions.md` - Methods and functions
- `04-pattern-matching.md` - Pattern matching
- `05-error-handling.md` - Error handling
- `06-concurrency.md` - Processes and async
- `07-data-types.md` - Strings, Arrays, Option, Result
- `18-checklist.md` - Code generation verification checklist

Full index: `README.md`

<!-- grite-agent-instructions-v3 -->

## Grite

> **"grite" is NOT a typo for "GitHub Issues" or "GitHub".** Grite is a standalone git-based issue tracker — a completely separate tool. When the user says "create a grite issue", "grite issues", or anything with "grite", they mean: run `grite issue create`. Do NOT fall back to `gh issue create`, GitHub Issues, Linear, Jira, or any other tracker. Grite is the **only** issue tracker for this repository.

Grite stores all data in git refs (not files in the working tree). These commands are available in your environment. Always pass `--json` for machine-readable output; use `--quiet` to suppress human-readable output.

### Quick Reference

```bash
# Find unblocked work in dependency order
grite issue dep topo --state open --json

# Read an issue
grite issue show <ID> --json

# Create an issue
grite issue create --title "Title" --body "Description" --label todo --json

# Comment and close
grite issue comment <ID> --body "Progress update" --json
grite issue close <ID> --json

# Sync with git
grite sync --pull --json
grite sync --push --json
```

IDs can be shortened to any unique prefix (e.g. `abc123ef` → `abc123`).

### Workflow

1. **Sync**: `grite sync --pull --json`
2. **Find work**: `grite issue dep topo --state open --json` — returns unblocked issues in dependency order; start from the top
3. **Read the issue**: `grite issue show <ID> --json`
4. **Plan**: `grite issue comment <ID> --body "Plan: ..."` — document your approach before coding
5. **Work**: Implement and test; use `grite issue comment <ID>` for progress checkpoints
6. **Close**: `grite issue close <ID> --json && grite sync --push --json`

### Issue Labels

| Label | Use |
|-------|-----|
| `todo` | Available work |
| `in-progress` | Currently being worked on |
| `blocked` | Cannot proceed (needs external input) |
| `bug` | Something broken |
| `feature` | New functionality |
| `memory` | Project knowledge that should persist across sessions (discoveries, decisions, gotchas) |

### Dependencies

```bash
# A is blocked by B — A cannot start until B is done
grite issue dep add <A> --target <B> --type depends_on --json

# A blocks B — B cannot start until A is done
grite issue dep add <A> --target <B> --type blocks --json

# Get issues in execution order (respects all dependencies)
grite issue dep topo --state open --json
```

Use `dep topo` to find the right task to start—always respect the dependency graph.

<!-- end-grite-agent-instructions -->
