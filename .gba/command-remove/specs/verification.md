# gba remove 验证标准

## CLI 解析

- `gba remove my-feature` 正确解析为 `Command::Remove { slug: "my-feature", force: false }`
- `gba remove my-feature --force` 正确解析为 `Command::Remove { slug: "my-feature", force: true }`
- `gba remove` 缺少 slug 报错

## 状态检查

- feature 不存在时返回错误 `FeatureNotFound`
- GBA 未初始化时返回错误 `NotInitialized`

## 确认逻辑

- InProgress 状态：未传 `--force` 时需要用户确认
- Dirty worktree（有未提交改动）：未传 `--force` 时需要用户确认
- Planning / Planned / Failed / Completed 状态：无需确认直接执行
- `--force` 跳过所有确认

## 资源清理

- 成功调用 `git worktree remove` 清理 `.trees/{slug}/`
- 成功调用 `git branch -D` 删除 `feature/{id}-{slug}` 分支
- worktree remove 失败时回退到 `fs::remove_dir_all`

## 边界情况

- worktree 存在但无 state.yml（plan 未完成）：仍能清理 worktree 和 branch
- branch 已被删除：不报错，静默跳过
- worktree 已被手动删除：清理对应 branch，不报错

## 构建

- `cargo build` 通过
- `cargo test` 通过
- `cargo clippy -- -D warnings` 通过
- `cargo +nightly fmt` 无格式问题
