# Design: Update README.md

## Summary

Update README.md to accurately reflect the current codebase, including all commands, architecture, execution pipeline, and configuration options.

## Changes Required

### 1. Add `gba clean` Command Documentation

The `clean` command cleans up worktrees for merged/closed PRs but is not documented.

**Location**: Commands section

**Content to add**:
- Command: `gba clean [--dry-run] [--force]`
- Purpose: Clean up worktrees for closed/merged PRs
- Flags: `--dry-run` (preview), `--force` (also clean closed PRs)

### 2. Fix Directory Structure Naming

The current README shows `0001_my-feature/` but the code uses just the slug.

**Corrections**:
- `.trees/my-feature/` (not `0001_my-feature/`)
- `.gba/my-feature/` (not `0001_my-feature/`)
- Branch format remains: `feature/{id}-{slug}` (e.g., `feature/0001-my-feature`)

### 3. Document Complete Execution Pipeline

The `gba run` command has a sophisticated pipeline not fully documented.

**Add section explaining**:
1. Phase execution with TUI progress display
2. Auto-commit after each phase
3. Code review loop (up to 3 fix iterations)
4. Verification loop (up to 3 fix iterations)
5. LLM-powered PR creation with detailed descriptions

### 4. Document Task Types in Architecture

Add documentation for all task types used by the engine:
- `Init` - Repository initialization
- `Plan` - Interactive feature planning
- `Execute` - Phase execution
- `Review` - Code review (read-only)
- `Verification` - Verify against acceptance criteria
- `Fix` - Fix issues from review/verification
- `Pr` - Generate PR description
- `Custom` - User-defined tasks

### 5. Update Feature Workflow Section

Enhance the workflow description to include:
- TUI chat interface for planning
- TUI progress display for execution
- Check-fix loops with max 3 iterations
- Resume capability with automatic checkpoint detection

### 6. Verify Configuration Options

Ensure documented config matches `GbaConfig` struct:
- `agent.model` (optional)
- `agent.permission_mode` (auto/manual/none)
- `agent.budget_limit` (optional, USD)
- `prompts.include` (additional prompt directories)
- `git.auto_commit` (default: true)
- `git.branch_pattern` (default: "feature/{id}-{slug}")
- `review.enabled` (default: true)
- `review.provider` (codex/claude)

## Phases

- update-readme: Update README.md with all documented changes including clean command, directory structure fix, execution pipeline, task types, and configuration verification
