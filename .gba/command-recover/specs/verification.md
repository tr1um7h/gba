# Verification: command-recover

## Build & Lint

- `cargo build` passes
- `cargo clippy -- -D warnings` passes
- `cargo +nightly fmt` passes

## Unit Tests

- Failure point detection: phase failure, review failure, verification failure, PR failure
- State rollback: verify state.yml fields are correctly reset for each failure type
- Edge cases: first phase failure (no prior completed phase), all phases completed but review fails

## Manual Verification

1. Create a feature that fails during phase execution (e.g., force failure in phase 2 of 3)
2. Run `gba recover <slug>` — verify state.yml shows phase 2 as `Pending`, status as `InProgress`
3. Run `gba run <slug>` — verify it resumes from phase 2
4. Test review failure scenario: recover should clear `result.review`
5. Test with dirty worktree: verify warning message appears
6. Test with clean worktree: verify no warning
7. Test on non-failed feature: verify error message
