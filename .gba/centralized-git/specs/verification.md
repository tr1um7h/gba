# Verification: Centralized Git Operations

## Build Verification

- [ ] `cargo build` succeeds without errors
- [ ] `cargo clippy -- -D warnings` passes
- [ ] `cargo +nightly fmt --check` passes
- [ ] `cargo test` passes

## Code Quality Checks

### Phase 1: core-git-module

- [ ] `git.rs` created in `crates/gba-core/src/`
- [ ] `lib.rs` exports `pub mod git`
- [ ] `GitRepo` struct implemented with all methods:
  - `new()`, `workdir()`
  - `current_branch()`, `delete_branch()`, `detect_default_branch()`
  - `create_worktree()`, `remove_worktree()`
  - `head_short_sha()`, `add()`, `commit()`, `push()`
- [ ] `GitHub` struct implemented with `pr_status()` method
- [ ] `PrStatus` enum defined with `Open`, `Merged`, `Closed` variants
- [ ] `EngineError` has `GitError` and `GitHubError` variants
- [ ] Proper error handling with context messages
- [ ] Doc comments on all public items

### Phase 2: migrate-cli-commands

- [ ] `utils.rs`: Uses `GitRepo::detect_default_branch()`
- [ ] `plan.rs`: Uses `GitRepo::current_branch()` and `create_worktree()`
- [ ] `list.rs`: Uses `GitRepo::current_branch()`
- [ ] `clean.rs`: Uses `GitRepo` and `GitHub` operations, `PrStatus` from core
- [ ] `run.rs`: Uses `GitRepo::head_short_sha()`, `add()`, `commit()`, `push()`
- [ ] No direct `Command::new("git")` or `Command::new("gh")` calls remain in CLI
- [ ] `PrStatus` removed from `clean.rs` (use `gba_core::git::PrStatus`)

## Functional Verification

- [ ] `gba plan <feature>` still creates worktrees correctly
- [ ] `gba run <feature>` still commits and pushes state updates
- [ ] `gba list` still shows feature information with branch names
- [ ] `gba clean` still detects PR status and removes worktrees

## Grep Verification

After migration, these commands should return no results:

```bash
# No direct git commands in CLI (except imports)
grep -r 'Command::new("git")' apps/gba-cli/src/

# No direct gh commands in CLI
grep -r 'Command::new("gh")' apps/gba-cli/src/
```

The centralized module should contain all git operations:

```bash
# All git operations in core
grep -r 'Command::new("git")' crates/gba-core/src/git.rs
grep -r 'Command::new("gh")' crates/gba-core/src/git.rs
```
