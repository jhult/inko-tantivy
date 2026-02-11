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

## Landing the Plane (Session Completion)

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   bd sync
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
