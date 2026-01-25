# Design: Improve Code Quality

## Overview

Comprehensive refactoring of the GBA codebase to address DRY violations, SOLID principle issues, and test coverage gaps identified during architecture review.

## Goals

1. Eliminate code duplication (DRY violations)
2. Improve Single Responsibility Principle (SRP) adherence
3. Improve Interface Segregation Principle (ISP) for EventHandler
4. Strengthen test coverage and quality
5. Fix integration tests

## Non-Goals

- Mocking ClaudeClient for async tests (waiting for SDK support)
- Major architectural changes to crate boundaries
- Performance optimizations

---

## Phase 1: DRY Fixes in gba-core

### 1.1 Unify TaskStats

**Problem**: `TaskStats` is duplicated in `gba-core/src/task.rs` and `gba-cli/src/state.rs`.

**Solution**:
- Add `Serialize` and `Deserialize` derives to `gba_core::TaskStats`
- Remove duplicate `TaskStats` from `gba-cli/src/state.rs`
- Update `gba-cli` imports to use `gba_core::TaskStats`

**Files**:
- `crates/gba-core/src/task.rs` - Add serde derives
- `apps/gba-cli/src/state.rs` - Remove duplicate, update imports

### 1.2 Extract Token Usage Helper

**Problem**: Token extraction logic duplicated in `engine.rs:566-575` and `session.rs:488-495`.

**Solution**:
- Create `extract_token_usage()` helper function in `task.rs`
- Use helper in both `engine.rs` and `session.rs`

**Files**:
- `crates/gba-core/src/task.rs` - Add `TaskStats::update_from_usage()` method
- `crates/gba-core/src/engine.rs` - Use new method
- `crates/gba-core/src/session.rs` - Use new method

### 1.3 Consolidate Agent Options Building

**Problem**: Option merging logic duplicated in `engine.rs:493-506` and `session.rs:571-584`.

**Solution**:
- Create `merge_agent_options()` helper function in `config.rs`
- Use helper in both `Engine::build_agent_options()` and `SessionBuilder::build()`

**Files**:
- `crates/gba-core/src/config.rs` - Add `merge_options()` helper
- `crates/gba-core/src/engine.rs` - Use helper
- `crates/gba-core/src/session.rs` - Use helper

---

## Phase 2: SRP Improvements

### 2.1 Unify Message Processing in Session

**Problem**: `process_message_no_handler()` and `process_message_with_handler()` share ~70% identical code.

**Solution**:
- Create single `process_message()` method that accepts `Option<&mut dyn EventHandler>`
- Use `Option::map()` or conditional calls for handler events
- Remove the two separate methods

**Files**:
- `crates/gba-core/src/session.rs` - Refactor message processing

### 2.2 Extract Message Processing Logic

**Problem**: `Engine::process_streaming_message()` and `Session::process_message_with_handler()` have similar logic.

**Solution**:
- Create `MessageProcessor` struct with shared processing logic
- Both `Engine` and `Session` use `MessageProcessor`

**Files**:
- `crates/gba-core/src/message.rs` - New module with `MessageProcessor`
- `crates/gba-core/src/lib.rs` - Add module declaration
- `crates/gba-core/src/engine.rs` - Use `MessageProcessor`
- `crates/gba-core/src/session.rs` - Use `MessageProcessor`

---

## Phase 3: EventHandler ISP Improvement

### 3.1 Split EventHandler Trait

**Problem**: `EventHandler` trait mixes text, tool, error, and lifecycle handling concerns.

**Solution**:
- Create focused sub-traits:
  - `TextHandler` - `on_text()`
  - `ToolHandler` - `on_tool_use()`, `on_tool_result()`
  - `ErrorHandler` - `on_error()`
  - `LifecycleHandler` - `on_complete()`
- Keep `EventHandler` as a super-trait combining all four
- Update implementations to implement sub-traits

**New trait hierarchy**:
```rust
pub trait TextHandler: Send {
    fn on_text(&mut self, text: &str) {}
}

pub trait ToolHandler: Send {
    fn on_tool_use(&mut self, tool: &str, input: &serde_json::Value) {}
    fn on_tool_result(&mut self, result: &str) {}
}

pub trait ErrorHandler: Send {
    fn on_error(&mut self, error: &str) {}
}

pub trait LifecycleHandler: Send {
    fn on_complete(&mut self) {}
}

pub trait EventHandler: TextHandler + ToolHandler + ErrorHandler + LifecycleHandler {}

// Blanket implementation
impl<T> EventHandler for T where T: TextHandler + ToolHandler + ErrorHandler + LifecycleHandler {}
```

**Files**:
- `crates/gba-core/src/event.rs` - Split traits, update implementations
- `crates/gba-core/src/lib.rs` - Update exports if needed

---

## Phase 4: Test Quality Improvements

### 4.1 Fix Integration Tests

**Problem**: All integration tests are `#[ignore]` and have overly permissive assertions.

**Solution**:
- Remove `#[ignore]` from tests that don't require Claude CLI
- Make assertions stricter and more specific
- Add conditional compilation for tests requiring external dependencies
- Fix `test_init_command` to not accept arbitrary failures as success

**Files**:
- `apps/gba-cli/tests/cli_integration.rs` - Fix all tests

### 4.2 Add Unit Tests for Message Processing

**Problem**: No tests for message processing logic.

**Solution**:
- Add tests for `MessageProcessor` (from Phase 2)
- Test different message types (Assistant, User, Result)
- Test stats accumulation
- Test error handling paths

**Files**:
- `crates/gba-core/src/message.rs` - Add comprehensive tests

### 4.3 Add EventHandler Behavior Tests

**Problem**: EventHandler tests only check configuration, not actual behavior.

**Solution**:
- Test `PrintEventHandler` output using captured stdout
- Test `CollectingEventHandler` edge cases
- Test error message formatting

**Files**:
- `crates/gba-core/src/event.rs` - Add behavior tests

### 4.4 Add Session Unit Tests

**Problem**: Session tests don't cover builder options merging or connection state.

**Solution**:
- Test `SessionBuilder` option merging logic
- Test connection state transitions
- Test history management edge cases

**Files**:
- `crates/gba-core/src/session.rs` - Add more unit tests

### 4.5 Add Engine Unit Tests

**Problem**: Engine tests don't cover all prompt rendering paths.

**Solution**:
- Test non-preset system prompt rendering
- Test missing template fallback behavior
- Test agent options building with all configurations

**Files**:
- `crates/gba-core/src/engine.rs` - Add more unit tests

---

## Phases

- dry-fixes: Eliminate code duplication in gba-core (TaskStats, token extraction, options merging)
- srp-improvements: Unify message processing and extract MessageProcessor
- event-handler-isp: Split EventHandler into focused sub-traits
- test-improvements: Fix integration tests and add comprehensive unit tests

---

## File Change Summary

### New Files
- `crates/gba-core/src/message.rs` - MessageProcessor for shared message handling

### Modified Files

**gba-core**:
- `src/lib.rs` - Add message module, update exports
- `src/task.rs` - Add serde derives to TaskStats, add `update_from_usage()` method
- `src/config.rs` - Add `merge_options()` helper function
- `src/engine.rs` - Use MessageProcessor, use helpers, add tests
- `src/session.rs` - Use MessageProcessor, unify message processing, use helpers, add tests
- `src/event.rs` - Split into sub-traits, add behavior tests

**gba-cli**:
- `src/state.rs` - Remove duplicate TaskStats, import from gba-core
- `tests/cli_integration.rs` - Fix tests, remove ignores, stricter assertions

---

## Testing Strategy

1. **Unit Tests**: Each new helper function and trait gets unit tests
2. **Integration Tests**: Fix existing CLI integration tests
3. **Regression**: Ensure all existing tests continue to pass
4. **Coverage**: Target critical paths in message processing and event handling

---

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| Breaking API changes | Keep EventHandler backward compatible via blanket impl |
| Regression in message processing | Comprehensive tests before refactoring |
| Integration test flakiness | Use feature flags for tests requiring external deps |
