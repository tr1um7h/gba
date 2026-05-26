# Feature: command-recover

## Summary

Add a `gba recover <slug>` command that rolls back state.yml to allow resuming a failed `run` from the failure point, without performing any git operations.

## Motivation

When `gba run` fails mid-pipeline (phase execution, code review, or verification), the only option is `--restart` which resets all progress. This is wasteful. A `recover` command should intelligently roll back only the failed portion of state so the user can continue with `gba run`.

## Behavior

### Command

```
gba recover <slug>
```

### Preconditions

- state.yml must exist, otherwise error
- state.yml `status` must be `Failed`, otherwise error with guidance
- worktree must exist, otherwise error

### Failure Point Detection (priority order)

| Priority | Condition | Failure Stage |
|----------|-----------|---------------|
| 1 | Any phase has `Failed` or `InProgress` status | Phase execution |
| 2 | `result.review` exists and status is not `Passed`/`Skipped` | Code review |
| 3 | `result.verification` exists and status is not `Passed`/`Skipped` | Verification |
| 4 | None of the above (e.g., PR creation failure) | Other |

### State Modification Rules

#### Phase failure

Phases before the failed one are untouched (remain `Completed`).

| Field | Before | After |
|-------|--------|-------|
| `status` | `Failed` | `InProgress` |
| `error` | `Some("...")` | `None` |
| `current_phase` | (failed index) | failed phase index |
| `phases[i]` (i < failed) | `Completed` | `Completed` (unchanged) |
| `phases[failed].status` | `Failed` or `InProgress` | `Pending` |
| `phases[failed].started_at` | `Some(...)` or `None` | `None` |
| `phases[failed].completed_at` | `None` | `None` |
| `phases[failed].commit_sha` | `None` | `None` |
| `phases[failed].stats` | `Some(...)` or `None` | `None` |
| `phases[j]` (j > failed).status | `Pending` | `Pending` (unchanged) |
| `total_stats` | (accumulated) | (unchanged) |
| `result` | (unchanged) | (unchanged) |

Resume behavior: `detect_resume_point` finds the first `Pending` phase (= failed index). `gba run` resumes from that phase.

#### Review failure (all phases Completed)

| Field | Before | After |
|-------|--------|-------|
| `status` | `Failed` | `InProgress` |
| `error` | `Some("...")` | `None` |
| `current_phase` | last phase index | last phase index |
| `phases[*]` | `Completed` | `Completed` (unchanged) |
| `result.review` | `Some(CheckResultState{...})` | `None` |
| `result.verification` | `None` | `None` (unchanged) |
| `total_stats` | (accumulated) | (unchanged) |

Resume behavior: `detect_resume_point` returns `phases.len()` (all Completed), `execute_phases_inner` loop skips entirely, pipeline proceeds directly to review.

#### Verification failure (review passed/skipped)

| Field | Before | After |
|-------|--------|-------|
| `status` | `Failed` | `InProgress` |
| `error` | `Some("...")` | `None` |
| `current_phase` | last phase index | last phase index |
| `phases[*]` | `Completed` | `Completed` (unchanged) |
| `result.review` | `Some(CheckResultState{status: Passed/Skipped, ...})` | `None` |
| `result.verification` | `Some(CheckResultState{...})` | `None` |
| `total_stats` | (accumulated) | (unchanged) |

Resume behavior: same as review failure — phases skipped, review and verification both run again. Both are cleared because user's manual `git reset` will likely undo review fix commits too.

#### PR or other failure

| Field | Before | After |
|-------|--------|-------|
| `status` | `Failed` | `InProgress` |
| `error` | `Some("...")` | `None` |
| all other fields | (unchanged) | (unchanged) |

Resume behavior: `detect_resume_point` returns `phases.len()`, phases skipped, all checks and PR creation run.

### Edge Cases

- **First phase fails (index 0)**: No prior completed phase, no commit SHA to suggest. Output "No prior commit available for reset suggestion."
- **Worktree has uncommitted changes**: Warn user, suggest they commit or stash before running
- **Status is not Failed**: Print error with guidance ("only features with Failed status can be recovered")

### Git Handling

- **No git operations** — no reset, commit, or push
- Check worktree for uncommitted changes (`git status --porcelain`)
- If dirty, warn user and suggest manual `git reset --hard <last-good-commit-sha>`
- Report last good commit SHA from the last completed phase (if any)

### Output

```
Feature 'command-recover' recovered.

Recovery summary:
  Failed at: phase 'implementation' (index 2/4)
  Rolled back: phases 2-3 → pending
  Last good commit: def5678 (phase 'setup')

Git status:
  ⚠ Working tree has uncommitted changes.
  To undo failed phase changes: git reset --hard def5678

Next step: gba run command-recover
```

## Files

| File | Change |
|------|--------|
| `apps/gba-cli/src/cli.rs` | Add `Recover { slug }` subcommand |
| `apps/gba-cli/src/commands/recover.rs` | New — recover logic |
| `apps/gba-cli/src/commands/mod.rs` | Register recover module |
| `apps/gba-cli/src/main.rs` | Dispatch Recover command |
| `apps/gba-cli/src/commands/run.rs` | Fix `detect_resume_point`: all phases Completed → return `phases.len()` instead of `len()-1` |

No changes to `gba-core` or `state.rs` data structures.

## Phases

- add-recover-command: Add `Recover` subcommand to cli.rs, register module in mod.rs, dispatch in main.rs
- implement-recover-logic: Implement failure detection, state rollback, git status check, and output formatting in commands/recover.rs
- add-tests: Add unit tests for failure detection, state rollback, and edge cases
