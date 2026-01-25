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

GBA will:
1. Execute each phase from the design spec
2. Commit changes after each phase
3. Run code review
4. Verify against acceptance criteria
5. Create a pull request

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

## Configuration

### Project Configuration (`.gba/config.yml`)

```yaml
# Agent configuration
agent:
  permission_mode: auto  # auto | manual | none
  # budget_limit: 10.0   # Optional cost limit in USD

# Prompt configuration
prompts:
  include:
    - ~/.config/gba/prompts  # Additional prompt directories

# Git configuration
git:
  auto_commit: true
  branch_pattern: "feature/{id}-{slug}"

# Review configuration
review:
  enabled: true
  provider: codex  # codex | claude
```

### Feature State (`.gba/<id>_<slug>/state.yml`)

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
│   └── 0001_my-feature/            # Feature workspace
│       ├── specs/
│       │   ├── design.md           # Design document
│       │   └── verification.md     # Acceptance criteria
│       └── state.yml               # Execution state
├── .trees/                         # Git worktrees (gitignored)
│   └── 0001_my-feature/            # Feature branch worktree
└── .gba.md                         # Repository AI documentation
```

## Architecture

GBA consists of three main crates:

- **gba-cli**: Command-line interface with TUI support
- **gba-core**: Execution engine using Claude Agent SDK
- **gba-pm**: Prompt template management with MiniJinja

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
