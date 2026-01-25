# GBA (Geektime Bootcamp Agent)

[![Build Status](https://github.com/tyrchen/gba/workflows/build/badge.svg)](https://github.com/tyrchen/gba/actions)

GBA is a CLI tool that wraps the Claude Agent SDK to help developers plan and implement features through structured, AI-assisted workflows.

## Features

- **Interactive Planning**: Plan features through a TUI chat interface with Claude
- **Automated Execution**: Execute planned features with automatic phase progression
- **Resume Support**: Continue from interruptions with automatic checkpoint detection
- **Code Review**: Integrated AI-powered code review before PR creation
- **Verification**: Automated verification against acceptance criteria
- **Git Integration**: Automatic worktree creation, commits, and PR generation

## Installation

```bash
# Build from source
cargo install --path apps/gba-cli

# Or build in development mode
cargo build --release -p gba-cli
```

## Prerequisites

- Rust 1.85+ (2024 edition)
- Git
- [Claude Code CLI](https://docs.anthropic.com/en/docs/claude-code) installed and configured
- GitHub CLI (`gh`) for PR creation

## Quick Start

### 1. Initialize a Repository

```bash
cd your-project
gba init
```

This creates:
- `.gba/` - GBA metadata directory
- `.trees/` - Git worktrees (gitignored)
- `.gba.md` - Repository documentation for AI context

### 2. Plan a Feature

```bash
gba plan add-user-auth
```

Opens an interactive TUI where you can:
- Discuss feature requirements with Claude
- Review and refine the implementation plan
- Generate design specs and verification criteria

### 3. Execute the Plan

```bash
gba run add-user-auth
```

GBA executes a sophisticated pipeline with TUI progress display:

1. **Phase Execution** - Execute each phase from the design spec with real-time progress
2. **Auto-Commit** - Commit changes after each phase completes
3. **Code Review Loop** - AI-powered code review with up to 3 fix iterations
4. **Verification Loop** - Verify against acceptance criteria with up to 3 fix iterations
5. **PR Creation** - Generate detailed PR description using LLM

### 4. Check Status

```bash
# List all features
gba list

# Show detailed status
gba status add-user-auth
```

## Commands

### `gba init`

Initialize GBA in the current repository.

```bash
gba init
```

### `gba plan <slug>`

Start interactive planning for a new feature.

```bash
gba plan my-feature
```

### `gba run <slug>`

Execute a planned feature.

```bash
# Normal execution
gba run my-feature

# Dry run (no commits or pushes)
gba run my-feature --dry-run

# Restart from beginning
gba run my-feature --restart

# Resume from specific phase
gba run my-feature --from-phase 2
```

### `gba list`

List all features and their status.

```bash
gba list
```

### `gba status <slug>`

Show detailed status for a feature.

```bash
gba status my-feature
```

### `gba clean`

Clean up worktrees for merged or closed PRs.

```bash
# Preview what would be cleaned
gba clean --dry-run

# Clean worktrees for merged PRs only
gba clean

# Also clean closed (not merged) PRs
gba clean --force
```

The clean command:
- Scans `.trees/` for worktrees with associated PRs
- Removes worktrees and branches for merged PRs
- Preserves `.gba/<feature>/` directory for feature history
- With `--force`, also removes worktrees for closed (not merged) PRs

## Configuration

### Project Configuration (`.gba/config.yml`)

```yaml
# Agent configuration
agent:
  # model: claude-sonnet-4-20250514  # Optional: Claude model to use
  permission_mode: auto               # auto | manual | none
  # budget_limit: 10.0                # Optional: cost limit in USD

# Prompt configuration
prompts:
  include:
    - ~/.config/gba/prompts  # Additional prompt directories

# Git configuration
git:
  auto_commit: true                   # Commit after each phase (default: true)
  branch_pattern: "feature/{id}-{slug}"

# Review configuration
review:
  enabled: true                       # Enable code review (default: true)
  provider: codex                     # codex | claude
```

| Option | Description | Default |
|--------|-------------|---------|
| `agent.model` | Claude model to use | SDK default |
| `agent.permission_mode` | Permission handling: `auto`, `manual`, `none` | `auto` |
| `agent.budget_limit` | Cost limit in USD | None |
| `prompts.include` | Additional prompt template directories | `[]` |
| `git.auto_commit` | Auto-commit after each phase | `true` |
| `git.branch_pattern` | Branch naming pattern | `feature/{id}-{slug}` |
| `review.enabled` | Enable code review before PR | `true` |
| `review.provider` | Review provider: `codex`, `claude` | `codex` |

### Feature State (`.gba/<slug>/state.yml`)

Tracks execution progress including:
- Phase completion status
- Commit SHAs
- Execution statistics (turns, tokens, cost)
- PR information

## Directory Structure

```
your-project/
├── .gba/                           # GBA metadata
│   ├── config.yml                  # Project configuration
│   └── my-feature/                 # Feature workspace (by slug)
│       ├── specs/
│       │   ├── design.md           # Design document
│       │   └── verification.md     # Acceptance criteria
│       └── state.yml               # Execution state
├── .trees/                         # Git worktrees (gitignored)
│   └── my-feature/                 # Feature branch worktree (by slug)
└── .gba.md                         # Repository AI documentation
```

Note: Feature directories use the slug (e.g., `my-feature`), while branch names include the ID (e.g., `feature/0001-my-feature`).

## Architecture

GBA consists of three main crates:

- **gba-cli**: Command-line interface with TUI support
- **gba-core**: Execution engine using Claude Agent SDK
- **gba-pm**: Prompt template management with MiniJinja

### Task Types

The engine supports the following task types:

| Task | Description |
|------|-------------|
| `Init` | Repository initialization for GBA |
| `Plan` | Interactive feature planning through TUI chat |
| `Execute` | Phase execution with code changes |
| `Review` | Code review (read-only analysis) |
| `Verification` | Verify implementations against acceptance criteria |
| `Fix` | Fix issues identified in review or verification |
| `Pr` | Generate PR description using LLM |
| `Custom` | User-defined tasks with custom prompts |

Each task type has corresponding templates in the `tasks/` directory.

### Execution Pipeline

When running `gba run`, the engine follows this pipeline:

```
┌──────────────────────────────────────────────────────────────────┐
│                        Phase Execution                           │
│  ┌─────────┐   ┌─────────┐   ┌─────────┐                        │
│  │ Phase 1 │ → │ Phase 2 │ → │ Phase N │ → ...                  │
│  └────┬────┘   └────┬────┘   └────┬────┘                        │
│       ↓             ↓             ↓                              │
│   [Commit]      [Commit]      [Commit]                          │
└──────────────────────────────────────────────────────────────────┘
                              ↓
┌──────────────────────────────────────────────────────────────────┐
│                    Review & Fix Loop (max 3x)                    │
│  ┌────────┐   ┌─────────────┐   ┌─────┐                         │
│  │ Review │ → │ Issues? ────│─→ │ Fix │ ───────────┐            │
│  └────────┘   └──────┬──────┘   └──┬──┘            │            │
│                   No ↓             └───────────────┘            │
└──────────────────────────────────────────────────────────────────┘
                              ↓
┌──────────────────────────────────────────────────────────────────┐
│                 Verification & Fix Loop (max 3x)                 │
│  ┌────────────┐   ┌─────────────┐   ┌─────┐                     │
│  │ Verify vs  │ → │ Failed? ────│─→ │ Fix │ ───────────┐        │
│  │ Criteria   │   └──────┬──────┘   └──┬──┘            │        │
│  └────────────┘       No ↓             └───────────────┘        │
└──────────────────────────────────────────────────────────────────┘
                              ↓
┌──────────────────────────────────────────────────────────────────┐
│                       PR Creation                                │
│  Generate detailed PR description using LLM                      │
│  Create PR via GitHub CLI                                        │
└──────────────────────────────────────────────────────────────────┘
```

Key features:
- **TUI Progress Display**: Real-time progress visualization during execution
- **Automatic Checkpointing**: Resume from interruptions with `--from-phase`
- **Check-Fix Loops**: Up to 3 iterations to fix review/verification issues
- **LLM-Powered PR**: Generates detailed PR descriptions from commit history

## Development

```bash
# Run tests
cargo test

# Run with verbose output
cargo run -p gba-cli -- -v init

# Format code
cargo +nightly fmt

# Lint
cargo clippy -- -D warnings
```

## License

This project is distributed under the terms of MIT.

See [LICENSE](LICENSE.md) for details.

Copyright 2025 Tyr Chen
