# PR Option Feature Design

## Overview

添加 Git 配置选项来控制自动 PR 创建和自动 Push 行为。

## Configuration

在 `.gba/config.yml` 的 `git` 部分添加两个新配置项：

```yaml
git:
  autoCommit: true
  autoPr: true      # 默认 true，false 时不自动创建 PR
  autoPush: false   # 默认 false，true 时自动 push 到远程
  branchPattern: "feature/{id}-{slug}"
```

## Phases

- config-types: 在 `GitConfig` 中添加 `auto_pr` 和 `auto_push` 字段
- git-remote-detection: 在 `GitRepo` 中添加检测远程仓库存在的方法
- pr-creation-gate: 在 `create_pull_request` 中添加配置检查和远程检测
- state-push-gate: 在 `commit_and_push_state_update` 中添加配置检查和远程检测

## Implementation Details

### 1. Config Types (`apps/gba-cli/src/config.rs`)

在 `GitConfig` 结构体中添加：

```rust
/// Default value for auto_pr setting.
const fn default_auto_pr() -> bool {
    true
}

/// Default value for auto_push setting.
const fn default_auto_push() -> bool {
    false
}

pub struct GitConfig {
    #[serde(default = "default_auto_commit")]
    pub auto_commit: bool,

    #[serde(default = "default_auto_pr")]
    pub auto_pr: bool,

    #[serde(default = "default_auto_push")]
    pub auto_push: bool,

    #[serde(default = "default_branch_pattern")]
    pub branch_pattern: String,
}
```

### 2. Git Remote Detection (`crates/gba-core/src/git.rs`)

在 `GitRepo` 中添加：

```rust
/// Check if the repository has a remote origin configured.
pub fn has_remote_origin(&self) -> bool {
    let output = self
        .git_cmd()
        .args(["remote", "get-url", "origin"])
        .output();

    matches!(output, Ok(output) if output.status.success())
}
```

### 3. PR Creation Gate (`apps/gba-cli/src/commands/run.rs`)

修改 `create_pull_request` 函数：

1. 添加 `config` 参数（或从上下文中获取）
2. 在函数开始时检查：
   - `config.git.auto_pr` 是否为 false → 跳过并返回提示信息
   - `repo.has_remote_origin()` 是否为 false → 跳过并返回提示信息

### 4. State Push Gate (`apps/gba-cli/src/commands/run.rs`)

修改 `commit_and_push_state_update` 函数：

1. 添加 `config` 参数（或从上下文中获取）
2. 在 push 之前检查：
   - `config.git.auto_push` 是否为 false → 仅 commit 不 push
   - `repo.has_remote_origin()` 是否为 false → 仅 commit 不 push

### 5. 调用点更新

在 `execute_full_pipeline_with_tui` 函数中：

1. 加载 `GbaConfig`
2. 将配置传递给 `create_pull_request` 和 `commit_and_push_state_update`

## Behavior Summary

| 场景 | autoPr | autoPush | 远程存在 | PR 创建 | Push 行为 |
|------|--------|----------|----------|---------|-----------|
| 默认配置 | true | false | 是 | 执行 | 仅 commit |
| 禁用 PR | false | - | 是 | 跳过 | 仅 commit |
| 启用 Push | true | true | 是 | 执行 | commit + push |
| 本地仓库 | true | true | 否 | 跳过 | 仅 commit |

## Local Repository Detection

本地仓库（跳过 PR 和 Push）的检测特征：

### 1. 无 Remote 配置
- `git remote` 返回空
- `git remote get-url origin` 返回错误 "No such remote 'origin'"

### 2. Remote URL 为本地路径
- 以 `file://` 开头的 URL
- 以 `/` 开头的绝对路径
- 以 `./` 或 `../` 开头的相对路径
- 不含 `@` 和 `:` 的纯路径（非 SSH 格式）

### 3. 检测实现

```rust
/// Check if the repository has a valid remote origin (not local).
pub fn has_remote_origin(&self) -> bool {
    let output = self
        .git_cmd()
        .args(["remote", "get-url", "origin"])
        .output();

    let Ok(output) = output else { return false };
    if !output.status.success() {
        return false;
    }

    let url = String::from_utf8_lossy(&output.stdout);
    let url = url.trim();

    // Check if it's a local path
    if url.starts_with("file://")
        || url.starts_with('/')
        || url.starts_with("./")
        || url.starts_with("../")
    {
        return false;
    }

    // Check for SSH format (git@host:path or user@host:path)
    // Examples: git@github.com:user/repo.git, deploy@server.com:/path/to/repo
    if url.contains('@') && url.contains(':') && !url.starts_with("http") {
        return true;
    }

    // Check for HTTPS format
    if url.starts_with("http://") || url.starts_with("https://") {
        return true;
    }

    // Check for SSH protocol prefix
    if url.starts_with("ssh://") {
        return true;
    }

    // Treat anything else as potentially local
    false
}
```

## Notes

- 保持向后兼容：默认 `auto_pr: true` 保持现有行为
- 默认 `auto_push: false` 避免意外推送到远程
- 本地仓库（无 remote 或 remote 为本地路径）自动跳过 PR 创建和 Push
- 所有跳过操作都应记录 info 日志
- 检测保守：不确定的 URL 视为本地仓库（安全优先）
