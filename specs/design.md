# GBA (Geektime Bootcamp Agent) 设计文档

## 概述

GBA 是一个封装 Claude Agent SDK 的命令行工具，帮助开发者以结构化、AI 辅助的方式规划和实现功能。它提供三个主要命令：`init`、`plan` 和 `run`。

## 核心架构

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              gba-cli (命令行层)                              │
│                           clap (参数解析) + ratatui (TUI)                    │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────────────────────────────┐  ┌───────────────────────────────┐ │
│  │         gba-core (执行引擎)          │  │    gba-pm (提示词管理器)        │ │
│  │                                     │  │                               │ │
│  │  • Engine: 编排任务执行              │  │  • 模板加载                    │ │
│  │  • Session: 管理多轮对话             │  │  • 上下文渲染                  │ │
│  │  • Task: 表示工作单元                │  │  • 提示词组合                  │ │
│  │                                     │  │                               │ │
│  └──────────────┬──────────────────────┘  └───────────────────────────────┘ │
│                 │                                                           │
├─────────────────┼───────────────────────────────────────────────────────────┤
│                 ▼                                                           │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │                    claude-agent-sdk-rs                              │    │
│  │                  (双向流式通信, 工具调用)                             │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                                                                             │
├─────────────────────────────────────────────────────────────────────────────┤
│                              tokio (异步运行时)                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## 目录结构

GBA 在目标仓库中创建并管理以下目录结构：

```
<repo>/
├── .gba/                           # GBA 元数据目录
│   ├── config.toml                 # GBA 配置文件
│   ├── 0001_<feature-slug>/        # 功能工作区
│   │   ├── specs/
│   │   │   ├── design.md           # 功能设计文档
│   │   │   ├── verification.md     # 测试/验证标准
│   │   │   └── ...
│   │   ├── docs/
│   │   │   └── impl_details.md     # 实现笔记
│   │   └── state.json              # 执行状态
│   └── 0002_<feature-slug>/
│       └── ...
├── .trees/                         # Git worktrees (已 gitignore)
│   ├── 0001_<feature-slug>/        # 功能隔离分支
│   └── 0002_<feature-slug>/
├── .gba.md                         # 仓库 AI 文档
└── CLAUDE.md                       # 引用 .gba.md (如存在)
```

## 命令工作流

### 1. `gba init` - 初始化项目

```
┌──────────────┐     ┌─────────────────┐     ┌──────────────────────┐
│  gba init    │────▶│  Claude Agent   │────▶│     分析仓库结构      │
└──────────────┘     │      SDK        │     └──────────┬───────────┘
                     └─────────────────┘                │
                                                        ▼
┌──────────────────────────────────────────────────────────────────┐
│                           初始化任务                              │
├──────────────────────────────────────────────────────────────────┤
│  1. 检查是否已初始化 (.gba 存在) → 如是则退出                       │
│  2. 创建 .gba/ 目录结构                                           │
│  3. 创建 .trees/ 目录                                             │
│  4. 分析仓库结构（重要目录）                                        │
│  5. 生成 .gba.md 仓库文档                                         │
│  6. 更新 CLAUDE.md 添加对 .gba.md 的引用                          │
│  7. 将 .trees 添加到 .gitignore                                   │
└──────────────────────────────────────────────────────────────────┘
```

**输出示例：**
```
$ gba init
正在为 GBA 初始化当前项目...
✓ 已创建 .gba/ 目录
✓ 已创建 .trees/ 目录
✓ 已分析仓库结构
✓ 已生成 .gba.md
✓ 已更新 CLAUDE.md
完成！项目已初始化。
```

### 2. `gba plan <feature-slug>` - 交互式规划

```
┌───────────────────┐     ┌─────────────────┐     ┌──────────────────┐
│ gba plan <slug>   │────▶│  Ratatui TUI    │────▶│     聊天界面      │
└───────────────────┘     └─────────────────┘     └────────┬─────────┘
                                                           │
                          ┌────────────────────────────────┘
                          ▼
┌─────────────────────────────────────────────────────────────────────┐
│                         交互式规划会话                               │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  用户 ◀──────────────────────────────────────────────▶ 助手         │
│                                                                     │
│  助手: Can you let me know feature details?                        │
│  用户: 我想构建一个 web 前端，把 gba 的功能放在 web 上                  │
│  助手: 我计划用这样的思路来构建：...                                   │
│  用户: 需要修改...                                                   │
│  助手: 好的。这是修改后的思路：... 是否生成 spec?                       │
│  用户: 同意                                                          │
│  助手: 我将在 .trees 下生成 git worktree (branch from main)          │
│  助手: 开始生成 spec... spec 已生成。请 review。                       │
│  用户: 没意见                                                        │
│                                                                     │
│  Plan finished. Please call `gba run` to execute                    │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────────────┐
│                           生成的产物                                 │
├─────────────────────────────────────────────────────────────────────┤
│  .gba/0001_<slug>/specs/design.md        # 设计文档                  │
│  .gba/0001_<slug>/specs/verification.md  # 测试标准                  │
│  .gba/0001_<slug>/state.json             # 执行状态                  │
│  .trees/0001_<slug>/                     # Git worktree             │
└─────────────────────────────────────────────────────────────────────┘
```

### 3. `gba run <feature-slug>` - 执行计划

```
┌───────────────────┐     ┌─────────────────┐     ┌──────────────────┐
│ gba run <slug>    │────▶│    加载状态      │────▶│     执行阶段      │
└───────────────────┘     └─────────────────┘     └────────┬─────────┘
                                                           │
                          ┌────────────────────────────────┘
                          ▼
┌─────────────────────────────────────────────────────────────────────┐
│                           执行流水线                                 │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  $ gba run <feature-slug>                                          │
│  正在执行...                                                        │
│  [✓] 生成目录                                                       │
│  [✓] phase 1: 构建 observer                                        │
│  [✓] 提交 phase 1                                                  │
│  [✓] phase 2: 构建 测试                                            │
│  [✓] 提交 phase 2                                                  │
│  [✓] codex review                                                  │
│  [✓] 处理 review 结果                                               │
│  [✓] 验证系统                                                       │
│  [✓] 提交 PR                                                       │
│                                                                     │
│  执行完成！PR: https://github.com/...                               │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

## Crate 规格说明

### gba-pm (提示词管理器)

**职责：** 使用 MiniJinja 加载、渲染和管理提示词模板。

```rust
// 公共接口

/// 提示词管理器，负责加载和渲染模板
pub struct PromptManager { ... }

impl PromptManager {
    /// 创建新的提示词管理器
    pub fn new() -> Self;

    /// 从目录加载模板 (*.j2, *.jinja, *.jinja2)
    pub fn load_dir(&mut self, path: impl AsRef<Path>) -> Result<&mut Self>;

    /// 从字符串添加模板
    pub fn add(&mut self, name: &str, content: &str) -> Result<&mut Self>;

    /// 使用上下文渲染模板
    pub fn render(&self, name: &str, ctx: impl Serialize) -> Result<String>;

    /// 直接渲染字符串模板（一次性使用）
    pub fn render_str(&self, template: &str, ctx: impl Serialize) -> Result<String>;

    /// 列出所有模板名称
    pub fn names(&self) -> Vec<&str>;
}

/// 提示词操作错误
pub enum PromptError {
    TemplateNotFound(String),
    RenderError(minijinja::Error),
    IoError(std::io::Error),
}
```

**模板示例：**
```jinja
{# init_system.j2 #}
你正在分析一个仓库以帮助初始化 GBA。

仓库路径: {{ repo_path }}
{% if readme_content %}
README 内容:
{{ readme_content }}
{% endif %}

请分析仓库结构并识别：
1. 使用的主要编程语言
2. 需要文档化的重要目录
3. 构建系统和依赖项
```

### gba-core (核心执行引擎)

**职责：** 使用 Claude Agent SDK 编排 AI 辅助工作流。

```rust
// 公共接口

/// 核心执行引擎
pub struct Engine { ... }

impl Engine {
    /// 使用配置创建引擎
    pub fn new(config: EngineConfig) -> Result<Self>;

    /// 运行任务并返回结果
    pub async fn run(&self, task: Task) -> Result<TaskResult>;

    /// 运行任务并流式传输事件
    pub async fn run_stream(
        &self,
        task: Task,
        handler: impl EventHandler,
    ) -> Result<TaskResult>;

    /// 创建交互式会话
    pub fn session(&self) -> Session;
}

/// 引擎配置
#[derive(TypedBuilder)]
pub struct EngineConfig {
    /// 工作目录
    pub workdir: PathBuf,

    /// 提示词管理器实例
    pub prompts: PromptManager,

    /// Claude agent 选项（可选覆盖）
    #[builder(default)]
    pub agent_options: Option<AgentOptions>,
}

/// 要执行的任务
pub struct Task {
    /// 任务类型决定使用哪个提示词模板
    pub kind: TaskKind,

    /// 用于提示词渲染的上下文变量
    pub context: serde_json::Value,

    /// 可选的系统提示词覆盖
    pub system_prompt: Option<String>,
}

pub enum TaskKind {
    Init,           // 初始化仓库
    Plan,           // 规划功能
    Execute,        // 执行阶段
    Review,         // 代码审查
    Custom(String), // 自定义任务（模板名称）
}

/// 任务执行结果
pub struct TaskResult {
    pub success: bool,
    pub output: String,
    pub artifacts: Vec<Artifact>,
}

/// 多轮对话的交互式会话
pub struct Session { ... }

impl Session {
    /// 发送消息并获取响应
    pub async fn send(&mut self, message: &str) -> Result<String>;

    /// 流式发送
    pub async fn send_stream(
        &mut self,
        message: &str,
        handler: impl EventHandler,
    ) -> Result<String>;

    /// 获取对话历史
    pub fn history(&self) -> &[Message];

    /// 清除历史
    pub fn clear(&mut self);
}

/// 流式传输的事件处理 trait
pub trait EventHandler: Send + Sync {
    fn on_text(&mut self, text: &str);
    fn on_tool_use(&mut self, tool: &str, input: &serde_json::Value);
    fn on_tool_result(&mut self, result: &str);
    fn on_error(&mut self, error: &str);
    fn on_complete(&mut self);
}

/// 引擎操作错误
pub enum EngineError {
    ConfigError(String),
    PromptError(gba_pm::PromptError),
    AgentError(claude_agent_sdk_rs::Error),
    IoError(std::io::Error),
}
```

### gba-cli (命令行界面)

**职责：** 通过 CLI 和 TUI 进行用户交互。

```rust
// 命令结构

/// 主 CLI 应用
#[derive(Parser)]
#[command(name = "gba", about = "Geektime Bootcamp Agent")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// 工作目录（默认当前目录）
    #[arg(short, long, global = true)]
    pub workdir: Option<PathBuf>,

    /// 详细输出
    #[arg(short, long, global = true)]
    pub verbose: bool,
}

#[derive(Subcommand)]
pub enum Command {
    /// 在当前仓库初始化 GBA
    Init,

    /// 规划新功能（交互式 TUI）
    Plan {
        /// 功能标识（如 "add-login"）
        slug: String,
    },

    /// 执行已规划的功能
    Run {
        /// 要执行的功能标识
        slug: String,

        /// 从特定阶段恢复
        #[arg(long)]
        from_phase: Option<usize>,

        /// 试运行（不提交或推送）
        #[arg(long)]
        dry_run: bool,
    },

    /// 列出所有功能
    List,

    /// 显示功能状态
    Status {
        /// 功能标识
        slug: String,
    },
}
```

**TUI 组件：**
```
┌─────────────────────────────────────────────────────────────────┐
│  GBA Plan: add-user-auth                              [Ctrl+C] │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │ 助手: 能告诉我更多关于你想实现的认证功能吗？                    ││
│  │                                                            ││
│  │ 用户: 我想要支持 Google 和 GitHub 的 OAuth2 认证              ││
│  │                                                            ││
│  │ 助手: 明白了。这是我建议的方案：                               ││
│  │ 1. 添加 oauth2 crate 依赖                                   ││
│  │ 2. 创建带有 provider 抽象的 auth 模块                        ││
│  │ 3. 实现 Google provider                                    ││
│  │ 4. 实现 GitHub provider                                    ││
│  │ 5. 添加 login/callback 路由                                 ││
│  │                                                            ││
│  │ 是否继续生成 spec？                                          ││
│  └─────────────────────────────────────────────────────────────┘│
│                                                                 │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │ > 好的，请生成 spec                                         ││
│  └─────────────────────────────────────────────────────────────┘│
│                                                                 │
│  [Enter] 发送  [Ctrl+C] 退出  [↑↓] 历史                          │
└─────────────────────────────────────────────────────────────────┘
```

## 数据流

```
┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────────────┐
│   CLI    │───▶│  Engine  │───▶│ Prompts  │───▶│   渲染后的提示词   │
└──────────┘    └────┬─────┘    └──────────┘    └────────┬─────────┘
                     │                                    │
                     │         ┌──────────────────────────┘
                     │         ▼
                     │    ┌──────────────────┐
                     │───▶│ Claude Agent SDK │
                     │    └────────┬─────────┘
                     │             │
                     │    ┌────────┴─────────┐
                     │    │     流式响应      │
                     │    └────────┬─────────┘
                     │             │
                     ▼             ▼
              ┌──────────────────────────────┐
              │    事件处理器 (TUI 显示)       │
              │  • 显示文本                   │
              │  • 显示工具使用               │
              │  • 更新进度                   │
              └──────────────────────────────┘
```

## 状态管理

功能状态持久化在 `state.json` 中：

```json
{
  "feature_slug": "add-user-auth",
  "feature_id": "0001",
  "created_at": "2024-01-15T10:30:00Z",
  "status": "in_progress",
  "current_phase": 2,
  "phases": [
    {
      "name": "setup",
      "status": "completed",
      "commit_sha": "abc123"
    },
    {
      "name": "implementation",
      "status": "completed",
      "commit_sha": "def456"
    },
    {
      "name": "testing",
      "status": "in_progress",
      "commit_sha": null
    }
  ],
  "worktree_branch": "feature/0001-add-user-auth",
  "pr_url": null
}
```

## 错误处理策略

```rust
// 使用 thiserror 的统一错误类型
#[derive(Debug, thiserror::Error)]
pub enum GbaError {
    #[error("未初始化。请先运行 `gba init`。")]
    NotInitialized,

    #[error("功能未找到: {0}")]
    FeatureNotFound(String),

    #[error("功能已存在: {0}")]
    FeatureExists(String),

    #[error("无效状态: {0}")]
    InvalidState(String),

    #[error("Git 错误: {0}")]
    Git(String),

    #[error("Agent 错误: {0}")]
    Agent(#[from] EngineError),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
}
```

## 开发计划

### 第一阶段：基础设施 (gba-pm + gba-core 基础)

**任务：**
1. 实现完整模板支持的 `PromptManager`
2. 实现基本的 `Engine` 单次任务执行
3. 添加核心错误类型
4. 编写模板渲染单元测试
5. 创建初始提示词模板 (init, plan, execute)

**交付物：**
- 可用的提示词管理器
- 能执行简单任务的基本引擎
- 模板目录结构

### 第二阶段：交互式会话 (gba-core 流式支持)

**任务：**
1. 实现多轮对话的 `Session`
2. 添加带 `EventHandler` trait 的流式支持
3. 实现对话历史管理
4. 添加会话持久化（可选恢复）

**交付物：**
- 交互式会话支持
- 流式响应
- 事件处理基础设施

### 第三阶段：CLI 命令 (gba-cli)

**任务：**
1. 实现 `gba init` 命令
2. 实现 `gba list` 和 `gba status` 命令
3. 添加配置文件支持
4. 实现正确的错误处理和用户反馈

**交付物：**
- 可用的 `init` 命令
- 功能列表和状态查看
- 配置管理

### 第四阶段：TUI 规划界面

**任务：**
1. 构建基于 ratatui 的聊天界面
2. 实现带 TUI 的 `gba plan` 命令
3. 添加 git worktree 管理
4. 实现 spec 生成工作流

**交付物：**
- 交互式规划 TUI
- Spec 生成
- Git worktree 集成

### 第五阶段：执行流水线

**任务：**
1. 实现 `gba run` 命令
2. 构建阶段执行流水线
3. 添加自动提交支持
4. 集成代码审查步骤
5. 实现 PR 创建

**交付物：**
- 完整执行流水线
- 自动提交
- PR 集成

### 第六阶段：完善与文档

**任务：**
1. 添加完善的错误消息
2. 改进 TUI 美观度
3. 编写用户文档
4. 添加集成测试
5. 性能优化

**交付物：**
- 生产就绪的 CLI
- 用户文档
- 测试覆盖

## 文件组织

```
gba/
├── Cargo.toml                    # Workspace 定义
├── crates/
│   ├── gba-core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs            # 公共导出
│   │       ├── engine.rs         # Engine 实现
│   │       ├── session.rs        # Session 管理
│   │       ├── task.rs           # Task 类型
│   │       ├── event.rs          # 事件处理
│   │       ├── config.rs         # 配置
│   │       └── error.rs          # 错误类型
│   └── gba-pm/
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs            # 公共导出
│           ├── manager.rs        # PromptManager
│           └── error.rs          # 错误类型
├── apps/
│   └── gba-cli/
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs           # 入口点
│           ├── cli.rs            # CLI 定义
│           ├── commands/
│           │   ├── mod.rs
│           │   ├── init.rs       # gba init
│           │   ├── plan.rs       # gba plan
│           │   ├── run.rs        # gba run
│           │   ├── list.rs       # gba list
│           │   └── status.rs     # gba status
│           ├── tui/
│           │   ├── mod.rs
│           │   ├── app.rs        # TUI 应用
│           │   ├── chat.rs       # 聊天组件
│           │   ├── input.rs      # 输入处理
│           │   └── progress.rs   # 进度显示
│           └── state.rs          # 功能状态管理
└── prompts/                      # 默认提示词模板
    ├── init_system.j2
    ├── init_user.j2
    ├── plan_system.j2
    ├── plan_user.j2
    ├── execute_system.j2
    ├── execute_phase.j2
    └── review_system.j2
```

## 配置

`~/.config/gba/config.toml` 或 `.gba/config.toml`：

```toml
[agent]
# 使用的 Claude 模型（可选，SDK 处理默认值）
# model = "claude-sonnet-4-20250514"

# 权限模式
permission_mode = "auto"  # auto, manual, none

# 预算限制（美元，可选）
# budget_limit = 10.0

[prompts]
# 额外的提示词目录
include = ["~/.config/gba/prompts"]

[git]
# 每个阶段后自动提交
auto_commit = true

# 分支命名模式
branch_pattern = "feature/{id}-{slug}"

[review]
# 启用代码审查
enabled = true

# 审查提供者
provider = "codex"  # codex, claude
```
