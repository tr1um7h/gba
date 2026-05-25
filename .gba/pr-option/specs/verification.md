# PR Option Verification

## Test Cases

### Config Tests

1. **Default Config Values**
   - `auto_pr` 默认为 `true`
   - `auto_push` 默认为 `false`
   - 序列化和反序列化正常

2. **Custom Config Values**
   - 可以在 `config.yml` 中设置 `autoPr: false`
   - 可以在 `config.yml` 中设置 `autoPush: true`
   - 自定义值正确加载

### Git Remote Detection Tests

1. **Has Remote Origin**
   - 存在 `origin` 远程时返回 `true`
   - 远程 URL 可以是 HTTPS 或 SSH

2. **No Remote Origin**
   - 本地仓库（无 remote）返回 `false`
   - 远程名不是 `origin` 时返回 `false`

### Integration Tests

1. **PR Creation Skipped When autoPr=false**
   - 设置 `autoPr: false`
   - 运行 `gba run <feature>`
   - PR 创建阶段被跳过
   - 日志显示跳过原因

2. **PR Creation Skipped For Local Repo**
   - 在无 remote 的本地仓库运行
   - PR 创建阶段被跳过
   - 日志显示跳过原因

3. **Push Skipped When autoPush=false**
   - 设置 `autoPush: false`
   - 状态文件被 commit
   - 但不执行 push

4. **Push Executed When autoPush=true**
   - 设置 `autoPush: true`
   - 状态文件被 commit 并 push

5. **Push Skipped For Local Repo**
   - 在无 remote 的本地仓库运行
   - 即使有 `autoPush: true`，push 被跳过
   - 仅执行 commit

## Manual Verification Steps

1. 创建新 feature 并运行：
   ```bash
   gba plan "test feature"
   gba run test-feature
   ```
   - 验证 PR 被创建（默认行为）

2. 修改 `.gba/config.yml`：
   ```yaml
   git:
     autoPr: false
   ```
   - 运行 feature，验证 PR 未创建

3. 修改配置：
   ```yaml
   git:
     autoPr: true
     autoPush: true
   ```
   - 运行 feature，验证 commit 和 push 都执行

4. 在本地仓库（无 remote）运行：
   - 验证 PR 未创建
   - 验证 push 被跳过
