# Design: Improve Usage Experience

## Overview

改进 GBA 系统的使用体验，包括 plan 命令的状态检查放宽、完成时显示设计文档路径，以及添加 Agent 轮次配置。

## Changes

### 1. Plan Command State Check (`apps/gba-cli/src/commands/plan.rs`)

**Current**: 仅允许 `SessionStatus::Planning` 状态进入 plan

**New**: 只要不是 `Running` 或 `Completed` 状态，都可以进入 plan

- 允许进入 plan 的状态：`Planning`, `Planned`, `Error`
- 禁止进入 plan 的状态：`Running`, `Completed`

当处于禁止状态时，返回友好的错误消息提示用户当前状态。

### 2. Plan Completion Output (`apps/gba-cli/src/commands/plan.rs`)

在 plan 成功完成后，除了现有的成功消息，额外打印 design.md 的完整路径：

```
Planning completed successfully.
Design document: /absolute/path/to/.gba/<feature-slug>/specs/design.md
```

路径通过 `session.specs_path()` 或类似方法获取。

### 3. Agent Rounds Configuration (`crates/gba-core/src/config.rs`)

在 `AgentConfig` 结构体中添加 `rounds` 字段：

```rust
pub struct AgentConfig {
    pub model: String,
    pub temperature: f32,
    pub timeout_secs: u64,
    pub rounds: u32,  // 新增：控制 verification/review 最大迭代次数
}
```

默认值：`3`

在 `apps/gba-cli/src/web/run_app.rs` 中使用此配置限制 verification/review 的最大迭代轮次。

## Phases

- state-check: 修改 plan 命令的状态检查逻辑，允许 Planning/Planned/Error 状态进入
- completion-output: 在 plan 完成时打印 design.md 路径
- agent-config: 在 AgentConfig 中添加 rounds 字段并设置默认值
- rounds-integration: 在 run_app 中使用 rounds 配置限制迭代次数
