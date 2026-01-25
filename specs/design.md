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
│   ├── config.yml                  # GBA 项目配置文件
│   ├── 0001_<feature-slug>/        # 功能工作区
│   │   ├── specs/
│   │   │   ├── design.md           # 功能设计文档
│   │   │   ├── verification.md     # 测试/验证标准
│   │   │   └── ...
│   │   ├── docs/
│   │   │   └── impl_details.md     # 实现笔记
│   │   └── state.yml               # 执行状态
│   └── 0002_<feature-slug>/
│       └── ...
├── .trees/                         # Git worktrees (已 gitignore)
│   ├── 0001_<feature-slug>/        # 功能隔离分支
│   └── 0002_<feature-slug>/
├── .gba.md                         # 仓库 AI 文档
└── CLAUDE.md                       # 引用 .gba.md (如存在)
```

## 配置文件

### `.gba/config.yml` - 项目配置

此配置文件存储项目级别的 GBA 设置，用于：
- 覆盖默认的 agent 行为（模型、权限模式等）
- 指定额外的提示词模板目录
- 配置 git 行为（自动提交、分支命名）
- 配置代码审查选项

```yaml
# .gba/config.yml

# Agent 配置
agent:
  # 使用的 Claude 模型（可选，SDK 处理默认值）
  # model: claude-sonnet-4-20250514

  # 权限模式: auto | manual | none
  permission_mode: auto

  # 预算限制（美元，可选）
  # budget_limit: 10.0

# 提示词配置
prompts:
  # 额外的提示词目录（可选）
  include:
    - ~/.config/gba/prompts

# Git 配置
git:
  # 每个阶段后自动提交
  auto_commit: true

  # 分支命名模式
  # 可用变量: {id}, {slug}
  branch_pattern: "feature/{id}-{slug}"

# 代码审查配置
review:
  # 是否启用代码审查
  enabled: true

  # 审查提供者: codex | claude
  provider: codex
```

### `state.yml` - 功能执行状态

每个功能的执行状态存储在 `.gba/<feature-id>/state.yml` 中，用于：
- 跟踪执行进度，支持中断后恢复
- 记录每个阶段的执行结果和成本
- 存储最终的 PR 链接

```yaml
# .gba/0001_add-user-auth/state.yml

# 功能基本信息
feature:
  id: "0001"
  slug: add-user-auth
  created_at: "2024-01-15T10:30:00Z"
  updated_at: "2024-01-15T14:20:00Z"

# 执行状态: planned | in_progress | completed | failed
status: in_progress

# 当前执行到的阶段索引（从 0 开始）
current_phase: 2

# Git 信息
git:
  worktree_path: .trees/0001_add-user-auth
  branch: feature/0001-add-user-auth
  base_branch: main

# 阶段执行记录
phases:
  - name: setup
    status: completed  # pending | in_progress | completed | failed
    started_at: "2024-01-15T10:35:00Z"
    completed_at: "2024-01-15T10:42:00Z"
    commit_sha: abc1234
    # 执行统计
    stats:
      turns: 5
      input_tokens: 12500
      output_tokens: 8300
      cost_usd: 0.15

  - name: implementation
    status: completed
    started_at: "2024-01-15T10:45:00Z"
    completed_at: "2024-01-15T11:30:00Z"
    commit_sha: def5678
    stats:
      turns: 12
      input_tokens: 45000
      output_tokens: 32000
      cost_usd: 0.58

  - name: testing
    status: in_progress
    started_at: "2024-01-15T11:35:00Z"
    completed_at: null
    commit_sha: null
    stats:
      turns: 3
      input_tokens: 8000
      output_tokens: 5500
      cost_usd: 0.10

  - name: review
    status: pending
    started_at: null
    completed_at: null
    commit_sha: null
    stats: null

  - name: verification
    status: pending
    started_at: null
    completed_at: null
    commit_sha: null
    stats: null

# 总体统计
total_stats:
  turns: 20
  input_tokens: 65500
  output_tokens: 45800
  cost_usd: 0.83

# 最终结果
result:
  pr_url: null  # 完成后填入: https://github.com/owner/repo/pull/123
  pr_number: null
  merged: false

# 错误信息（如果失败）
error: null
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
│  .gba/0001_<slug>/state.yml              # 执行状态                  │
│  .trees/0001_<slug>/                     # Git worktree             │
└─────────────────────────────────────────────────────────────────────┘
```

### 3. `gba run <feature-slug>` - 执行计划

执行支持**断点恢复**：如果执行过程中被中断（Ctrl+C、网络问题、系统崩溃等），下次运行 `gba run` 会自动从上次中断的位置继续执行。

```
┌───────────────────┐     ┌─────────────────┐     ┌──────────────────┐
│ gba run <slug>    │────▶│  加载 state.yml │────▶│  检查断点/恢复    │
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
│  [✓] verification: 验证系统                                         │
│  [✓] 提交 PR                                                       │
│                                                                     │
│  执行完成！                                                         │
│  PR: https://github.com/owner/repo/pull/123                        │
│  总计: 20 turns, $0.83 USD                                         │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘

# 中断恢复示例
$ gba run add-user-auth
检测到未完成的执行，从 phase 3 (testing) 继续...
[✓] phase 3: testing (继续)
[✓] 提交 phase 3
...
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
    Verification,   // 验证测试
    Custom(String), // 自定义任务（模板名称）
}

/// 任务执行结果
pub struct TaskResult {
    pub success: bool,
    pub output: String,
    pub artifacts: Vec<Artifact>,
    pub stats: TaskStats,
}

/// 任务执行统计
pub struct TaskStats {
    pub turns: u32,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
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

    /// 获取当前会话统计
    pub fn stats(&self) -> &TaskStats;
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

        /// 从特定阶段恢复（覆盖自动检测）
        #[arg(long)]
        from_phase: Option<usize>,

        /// 试运行（不提交或推送）
        #[arg(long)]
        dry_run: bool,

        /// 强制重新开始（忽略已有进度）
        #[arg(long)]
        restart: bool,
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
              │  • 更新 state.yml            │
              └──────────────────────────────┘
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

## 提示词模板

所有提示词模板存放在 `crates/gba-pm/templates/` 目录下。

### 模板文件结构

```
crates/gba-pm/templates/
├── init/
│   └── system.j2           # 初始化系统提示词
├── plan/
│   └── system.j2           # 规划系统提示词
├── execute/
│   ├── system.j2           # 执行系统提示词
│   └── phase.j2            # 单阶段执行提示词
├── review/
│   └── system.j2           # 代码审查系统提示词
└── verification/
    └── system.j2           # 验证系统提示词
```

### 模板内容

#### `init/system.j2` - 初始化系统提示词

```jinja
{# init/system.j2 - Repository initialization prompt #}
You are a repository analyzer helping to initialize GBA (Geektime Bootcamp Agent) for a software project.

## Your Task

Analyze the repository at `{{ repo_path }}` and generate documentation that will help AI assistants understand and work with this codebase.

## Repository Information

{% if readme_content %}
### README Content
```
{{ readme_content }}
```
{% endif %}

{% if file_tree %}
### File Structure
```
{{ file_tree }}
```
{% endif %}

## Instructions

1. **Analyze the repository structure** and identify:
   - Primary programming language(s)
   - Framework(s) and major dependencies
   - Build system and package manager
   - Project architecture pattern (monorepo, microservices, etc.)

2. **Identify important directories** that need documentation:
   - Source code directories
   - Configuration directories
   - Test directories
   - Documentation directories

3. **Generate `.gba.md`** with the following sections:
   - Project Overview (1-2 paragraphs)
   - Technology Stack (bullet list)
   - Directory Structure (with descriptions)
   - Development Setup (if determinable)
   - Key Conventions (coding style, naming, etc.)

4. **Update `CLAUDE.md`** (if it exists) to include a reference to `.gba.md`

## Output Format

Use the file writing tools to:
1. Create `.gba.md` with comprehensive project documentation
2. Update `CLAUDE.md` if it exists, otherwise create it with a reference to `.gba.md`
3. Create `.gba/config.yml` with default configuration
```

#### `plan/system.j2` - 规划系统提示词

```jinja
{# plan/system.j2 - Feature planning prompt #}
You are a software architect helping to plan a new feature for a software project.

## Context

**Repository:** {{ repo_path }}
**Feature Slug:** {{ feature_slug }}
**Feature ID:** {{ feature_id }}

{% if gba_md_content %}
### Project Documentation
```
{{ gba_md_content }}
```
{% endif %}

{% if existing_specs %}
### Existing Feature Specs
{% for spec in existing_specs %}
- {{ spec.id }}: {{ spec.slug }} ({{ spec.status }})
{% endfor %}
{% endif %}

## Your Task

Engage in a conversation with the user to understand their feature requirements and create a detailed implementation plan.

## Planning Process

1. **Gather Requirements**
   - Ask clarifying questions about the feature
   - Understand the scope and constraints
   - Identify dependencies on existing code

2. **Design the Solution**
   - Propose an implementation approach
   - Break down into phases (each phase should be independently committable)
   - Identify potential risks and mitigations

3. **Generate Specifications** (when user approves)
   - Create `design.md` with:
     - Feature overview
     - Technical approach
     - Phase breakdown with clear deliverables
     - File changes per phase
   - Create `verification.md` with:
     - Test cases
     - Acceptance criteria
     - Manual verification steps

4. **Setup Git Worktree**
   - Create branch: `{{ branch_pattern | replace("{id}", feature_id) | replace("{slug}", feature_slug) }}`
   - Initialize worktree at `.trees/{{ feature_id }}_{{ feature_slug }}`

## Guidelines

- Each phase should be small enough to complete in one session
- Each phase should result in working, testable code
- Phases should build on each other incrementally
- Include clear commit messages for each phase

## Output

When the user approves the plan:
1. Create `.gba/{{ feature_id }}_{{ feature_slug }}/specs/design.md`
2. Create `.gba/{{ feature_id }}_{{ feature_slug }}/specs/verification.md`
3. Create `.gba/{{ feature_id }}_{{ feature_slug }}/state.yml` with status: planned
4. Create git worktree and branch

Inform the user: "Plan finished. Please call `gba run {{ feature_slug }}` to execute."
```

#### `execute/system.j2` - 执行系统提示词

```jinja
{# execute/system.j2 - Phase execution prompt #}
You are a software developer implementing a planned feature.

## Context

**Repository:** {{ repo_path }}
**Feature:** {{ feature_id }}_{{ feature_slug }}
**Worktree:** {{ worktree_path }}

{% if design_spec %}
### Design Specification
```markdown
{{ design_spec }}
```
{% endif %}

{% if verification_spec %}
### Verification Criteria
```markdown
{{ verification_spec }}
```
{% endif %}

## Execution State

**Status:** {{ status }}
**Current Phase:** {{ current_phase }} / {{ total_phases }}

{% if is_resuming %}
### ⚠️ RESUMING FROM INTERRUPTION

This execution was previously interrupted. You are resuming from phase {{ current_phase }} ({{ current_phase_name }}).

**Previous Progress:**
{% for phase in completed_phases %}
- ✓ Phase {{ loop.index }}: {{ phase.name }} (commit: {{ phase.commit_sha }})
{% endfor %}
- → Phase {{ current_phase }}: {{ current_phase_name }} (IN PROGRESS - resuming)

**Important:**
- Review the current state of the worktree
- Check what was already implemented in the interrupted phase
- Continue from where you left off, do not redo completed work
- If unsure about the state, check git status and recent changes
{% endif %}

### Phases
{% for phase in phases %}
- {% if phase.status == 'completed' %}✓{% elif phase.status == 'in_progress' %}→{% else %}○{% endif %} Phase {{ loop.index }}: {{ phase.name }} {% if phase.status == 'completed' %}(commit: {{ phase.commit_sha }}){% endif %}
{% endfor %}

## Your Task

Execute **Phase {{ current_phase }}: {{ current_phase_name }}**

### Phase Requirements
{{ current_phase_requirements }}

## Guidelines

1. **Work in the worktree directory:** `{{ worktree_path }}`
2. **Make incremental changes** - commit frequently if the phase is large
3. **Run tests** after making changes to ensure nothing is broken
4. **Follow project conventions** as documented in `.gba.md`

## On Completion

When you finish this phase:
1. Ensure all tests pass
2. Commit your changes with message: `feat({{ feature_slug }}): {{ current_phase_name }}`
3. Report completion so the next phase can begin

## On Error

If you encounter an error that blocks progress:
1. Document the error clearly
2. Suggest potential solutions
3. The execution will be paused for user intervention
```

#### `execute/phase.j2` - 单阶段用户提示词

```jinja
{# execute/phase.j2 - Single phase execution user prompt #}
Execute Phase {{ phase_number }}: {{ phase_name }}

{% if is_resuming %}
**Note:** This phase was interrupted. Resume from where you left off.
Check the current state before making changes.
{% endif %}

Requirements:
{{ phase_requirements }}

Expected deliverables:
{% for deliverable in deliverables %}
- {{ deliverable }}
{% endfor %}
```

#### `review/system.j2` - 代码审查系统提示词

```jinja
{# review/system.j2 - Code review prompt #}
You are a code reviewer performing a thorough review of implemented changes.

## Context

**Repository:** {{ repo_path }}
**Feature:** {{ feature_id }}_{{ feature_slug }}
**Branch:** {{ branch_name }}

### Changes to Review
```diff
{{ git_diff }}
```

### Design Specification
```markdown
{{ design_spec }}
```

### Verification Criteria
```markdown
{{ verification_spec }}
```

## Review Checklist

1. **Correctness**
   - Does the implementation match the design specification?
   - Are all requirements addressed?
   - Are edge cases handled?

2. **Code Quality**
   - Is the code readable and maintainable?
   - Are functions/methods appropriately sized?
   - Is there unnecessary duplication?

3. **Testing**
   - Are there adequate tests?
   - Do tests cover edge cases?
   - Are tests meaningful (not just for coverage)?

4. **Security**
   - Are there any security vulnerabilities?
   - Is input validation adequate?
   - Are secrets handled properly?

5. **Performance**
   - Are there any obvious performance issues?
   - Are there unnecessary allocations or loops?

## Output Format

Provide your review in the following format:

### Summary
[Overall assessment: APPROVED / NEEDS_CHANGES / BLOCKED]

### Issues Found
[List any issues, categorized by severity: critical, major, minor, suggestion]

### Recommended Changes
[Specific changes to make, if any]

If changes are needed, implement them directly.
```

#### `verification/system.j2` - 验证系统提示词

```jinja
{# verification/system.j2 - Verification prompt #}
You are a QA engineer verifying that an implemented feature meets its acceptance criteria.

## Context

**Repository:** {{ repo_path }}
**Feature:** {{ feature_id }}_{{ feature_slug }}
**Branch:** {{ branch_name }}

### Verification Criteria
```markdown
{{ verification_spec }}
```

### Implementation Summary
{% for phase in completed_phases %}
- Phase {{ loop.index }}: {{ phase.name }}
  - Commit: {{ phase.commit_sha }}
  - Changes: {{ phase.files_changed }} files
{% endfor %}

## Your Task

1. **Run all tests**
   ```bash
   # Run the project's test suite
   ```

2. **Verify each acceptance criterion**
   - Go through each criterion in the verification spec
   - Execute the verification steps
   - Document pass/fail for each

3. **Perform integration check**
   - Ensure the feature works with existing functionality
   - Check for regressions

4. **Final validation**
   - Build the project
   - Run any linters or static analysis
   - Verify documentation is updated if needed

## Output Format

### Test Results
[Summary of test execution]

### Acceptance Criteria Results
| Criterion | Status | Notes |
|-----------|--------|-------|
| [criterion 1] | ✓ PASS / ✗ FAIL | [notes] |
| ... | ... | ... |

### Overall Verdict
[VERIFIED / FAILED]

### Issues Found
[List any issues that need to be addressed]

If verification fails, document what needs to be fixed before the PR can be created.
```

## 开发计划

### 第一阶段：基础设施 (gba-pm + gba-core 基础)

**任务：**
1. 实现完整模板支持的 `PromptManager`
2. 实现基本的 `Engine` 单次任务执行
3. 添加核心错误类型
4. 编写模板渲染单元测试
5. 创建所有提示词模板

**交付物：**
- 可用的提示词管理器
- 能执行简单任务的基本引擎
- 完整的模板目录结构

### 第二阶段：交互式会话 (gba-core 流式支持)

**任务：**
1. 实现多轮对话的 `Session`
2. 添加带 `EventHandler` trait 的流式支持
3. 实现对话历史管理
4. 添加 `TaskStats` 统计收集

**交付物：**
- 交互式会话支持
- 流式响应
- 执行统计

### 第三阶段：CLI 命令 (gba-cli)

**任务：**
1. 实现 `gba init` 命令
2. 实现 `gba list` 和 `gba status` 命令
3. 实现 `config.yml` 和 `state.yml` 解析
4. 实现正确的错误处理和用户反馈

**交付物：**
- 可用的 `init` 命令
- 功能列表和状态查看
- 配置和状态管理

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
2. 实现断点恢复机制
3. 构建阶段执行流水线
4. 添加自动提交支持
5. 集成代码审查和验证步骤
6. 实现 PR 创建

**交付物：**
- 完整执行流水线（支持恢复）
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
│   │       ├── stats.rs          # 统计收集
│   │       ├── config.rs         # 配置
│   │       └── error.rs          # 错误类型
│   └── gba-pm/
│       ├── Cargo.toml
│       ├── src/
│       │   ├── lib.rs            # 公共导出
│       │   ├── manager.rs        # PromptManager
│       │   └── error.rs          # 错误类型
│       └── templates/            # 提示词模板
│           ├── init/
│           │   └── system.j2
│           ├── plan/
│           │   └── system.j2
│           ├── execute/
│           │   ├── system.j2
│           │   └── phase.j2
│           ├── review/
│           │   └── system.j2
│           └── verification/
│               └── system.j2
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
│           ├── config.rs         # config.yml 解析
│           └── state.rs          # state.yml 管理
└── specs/
    └── design.md                 # 本文档
```
