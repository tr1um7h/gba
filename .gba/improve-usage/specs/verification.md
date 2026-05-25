# Verification: Improve Usage Experience

## Test Cases

### 1. Plan State Check

#### Test: Running 状态禁止 plan
```bash
# 当 session 状态为 Running 时
gba plan
# 期望：错误提示 "Cannot enter plan mode while session is running"
```

#### Test: Completed 状态禁止 plan
```bash
# 当 session 状态为 Completed 时
gba plan
# 期望：错误提示 "Session is already completed"
```

#### Test: Planning 状态允许 plan
```bash
# 当 session 状态为 Planning 时
gba plan
# 期望：成功进入 plan 模式
```

#### Test: Planned 状态允许 plan
```bash
# 当 session 状态为 Planned 时
gba plan
# 期望：成功进入 plan 模式
```

#### Test: Error 状态允许 plan
```bash
# 当 session 状态为 Error 时
gba plan
# 期望：成功进入 plan 模式
```

### 2. Plan Completion Output

#### Test: 成功时显示路径
```bash
gba plan
# 完成 plan 后
# 期望输出包含：
# Planning completed successfully.
# Design document: /path/to/.gba/<feature>/specs/design.md
```

#### Test: 路径正确性
- 验证路径是绝对路径
- 验证路径指向正确的 design.md 文件
- 验证路径与 session 的 feature slug 匹配

### 3. Agent Rounds Configuration

#### Test: 默认值
- 验证新创建的 AgentConfig 中 rounds 默认为 3

#### Test: 配置解析
```yaml
# gba.yaml
agent:
  model: "claude-sonnet-4-6"
  temperature: 0.7
  timeout_secs: 300
  rounds: 5
```
- 验证配置正确解析 rounds 字段

#### Test: 轮次限制
- 在 run_app 中验证 verification/review 迭代不超过配置的 rounds 次数
- 当达到最大轮次时，应用应正常退出并报告状态

## Regression Tests

- [ ] 现有 plan 命令功能正常
- [ ] 现有 run 命令功能正常
- [ ] 配置文件向后兼容（未设置 rounds 时使用默认值）
