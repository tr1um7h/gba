# gba remove 命令

## Overview

`gba remove <slug>` 是 `gba plan <slug>` 的逆操作，用于放弃一个 feature 并清理所有相关资源（worktree、branch、specs、state）。

## Technical Approach

`gba plan` 创建的资源及清理方式：

| 资源 | 路径 | 清理方式 |
|------|------|----------|
| Git worktree | `.trees/{slug}/` | `GitRepo::remove_worktree` (force) |
| Git branch | `feature/{id}-{slug}` | `GitRepo::delete_branch` (force) |
| Feature 目录 | `.trees/{slug}/.gba/{slug}/` | 随 worktree 一起删除 |

清理失败时回退到 `fs::remove_dir_all`。

## CLI 接口

```
gba remove <slug>         # 交互式确认后删除
gba remove <slug> --force # 跳过确认直接删除
```

### 确认逻辑

需要确认的场景：
- **InProgress** — 可能正在运行，警告用户 "该 feature 可能仍在执行中，确认要放弃吗？"
- **有未提交改动**（dirty worktree）— 本地代码修改将丢失

不需要确认的场景：
- Planning / Planned / Failed — 无有价值的数据
- Completed（PR 已 merged/closed）— 工作已合并

`--force` 跳过所有确认。

## Phases

- add-remove-command: 在 cli.rs 添加 Remove 子命令定义，在 main.rs 添加分发逻辑，在 commands/mod.rs 注册模块
- implement-remove-handler: 创建 commands/remove.rs 实现 run_remove 函数，包含状态检查、确认提示、资源清理逻辑
- add-unit-tests: 为 remove 命令编写单元测试覆盖各状态和确认场景

## Files

- `apps/gba-cli/src/cli.rs` — 添加 `Remove { slug, force }` 命令变体
- `apps/gba-cli/src/commands/remove.rs` — 新文件，remove 命令实现
- `apps/gba-cli/src/commands/mod.rs` — 注册 remove 模块
- `apps/gba-cli/src/main.rs` — 添加 Command::Remove 分发
