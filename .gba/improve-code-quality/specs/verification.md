# Verification: Improve Code Quality

## Verification Criteria

### Phase 1: DRY Fixes

#### 1.1 TaskStats Unification
- [ ] `gba_core::TaskStats` has `#[derive(Serialize, Deserialize)]`
- [ ] `gba_core::TaskStats` has `#[serde(rename_all = "camelCase")]`
- [ ] No `TaskStats` struct exists in `apps/gba-cli/src/state.rs`
- [ ] `gba-cli` compiles and uses `gba_core::TaskStats`
- [ ] Existing YAML state files deserialize correctly (backward compatible)

#### 1.2 Token Usage Helper
- [ ] `TaskStats::update_from_usage()` method exists
- [ ] Method handles missing keys gracefully
- [ ] `engine.rs` uses `update_from_usage()` instead of inline extraction
- [ ] `session.rs` uses `update_from_usage()` instead of inline extraction
- [ ] No duplicated token extraction code remains

#### 1.3 Agent Options Merging
- [ ] `merge_options()` helper exists in `config.rs`
- [ ] Helper handles all option fields: model, permission_mode, max_turns, cwd
- [ ] `Engine::build_agent_options()` uses helper
- [ ] `SessionBuilder::build()` uses helper
- [ ] No duplicated option merging code remains

### Phase 2: SRP Improvements

#### 2.1 Session Message Processing
- [ ] Single `process_message()` method exists
- [ ] Method accepts optional handler parameter
- [ ] `process_message_no_handler()` removed
- [ ] `process_message_with_handler()` removed
- [ ] `send()` and `send_stream()` both use unified method

#### 2.2 MessageProcessor Extraction
- [ ] `message.rs` module exists
- [ ] `MessageProcessor` struct handles message type dispatch
- [ ] `MessageProcessor` handles stats updates
- [ ] `Engine::process_streaming_message()` uses `MessageProcessor`
- [ ] `Session::process_message()` uses `MessageProcessor`
- [ ] No duplicated message handling logic between Engine and Session

### Phase 3: EventHandler ISP

#### 3.1 Trait Splitting
- [ ] `TextHandler` trait exists with `on_text()` method
- [ ] `ToolHandler` trait exists with `on_tool_use()` and `on_tool_result()` methods
- [ ] `ErrorHandler` trait exists with `on_error()` method
- [ ] `LifecycleHandler` trait exists with `on_complete()` method
- [ ] `EventHandler` super-trait combines all four
- [ ] Blanket implementation: `impl<T> EventHandler for T where T: TextHandler + ToolHandler + ErrorHandler + LifecycleHandler`

#### 3.2 Implementation Updates
- [ ] `PrintEventHandler` implements all sub-traits
- [ ] `CollectingEventHandler` implements all sub-traits
- [ ] Existing code using `EventHandler` continues to work
- [ ] New code can implement only needed sub-traits

### Phase 4: Test Improvements

#### 4.1 Integration Tests
- [ ] `test_cli_help` runs without `#[ignore]`
- [ ] `test_cli_version` runs without `#[ignore]`
- [ ] `test_list_without_init` runs without `#[ignore]`
- [ ] `test_status_feature_not_found` runs without `#[ignore]`
- [ ] `test_init_command` has specific assertions (not accepting any error)
- [ ] `test_run_dry_run_option` has specific assertions

#### 4.2 Message Processing Tests
- [ ] Tests exist for `MessageProcessor` with `Message::Assistant`
- [ ] Tests exist for `MessageProcessor` with `Message::User` (tool results)
- [ ] Tests exist for `MessageProcessor` with `Message::Result`
- [ ] Tests exist for stats accumulation
- [ ] Tests exist for error conditions

#### 4.3 EventHandler Tests
- [ ] Tests verify `PrintEventHandler::on_text()` produces output
- [ ] Tests verify `PrintEventHandler::on_error()` produces stderr output
- [ ] Tests verify auto-flush behavior
- [ ] Tests verify result preview truncation (200 char limit)

#### 4.4 Session Tests
- [ ] Tests exist for `SessionBuilder` option merging
- [ ] Tests exist for base options override behavior
- [ ] Tests exist for task config application

#### 4.5 Engine Tests
- [ ] Tests exist for non-preset system prompt rendering
- [ ] Tests exist for missing template fallback (returns preset default)
- [ ] Tests exist for agent options with permission mode

---

## Build Verification

```bash
# All must pass
cargo build --workspace
cargo test --workspace
cargo +nightly fmt -- --check
cargo clippy --workspace -- -D warnings
```

---

## Backward Compatibility

1. **API Compatibility**: `EventHandler` trait remains usable as before
2. **Serialization Compatibility**: Existing `state.yml` files deserialize correctly
3. **CLI Compatibility**: All existing CLI commands work unchanged

---

## Code Quality Metrics

After refactoring:
- No duplicate `TaskStats` definition
- No duplicate token extraction code
- No duplicate option merging code
- Single message processing path in Session
- Shared message processing between Engine and Session
- All tests pass without `#[ignore]` (except tests requiring Claude CLI)
