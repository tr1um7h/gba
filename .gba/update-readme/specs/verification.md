# Verification: Update README.md

## Acceptance Criteria

### 1. Command Documentation Complete

- [ ] `gba init` command is documented
- [ ] `gba plan <slug>` command is documented
- [ ] `gba run <slug>` command is documented with all flags (`--from-phase`, `--dry-run`, `--restart`)
- [ ] `gba list` command is documented
- [ ] `gba status <slug>` command is documented
- [ ] `gba clean` command is documented with flags (`--dry-run`, `--force`)

### 2. Directory Structure Accurate

- [ ] `.trees/<slug>/` format is correctly documented (not `<id>_<slug>`)
- [ ] `.gba/<slug>/` format is correctly documented
- [ ] Branch naming pattern `feature/{id}-{slug}` is documented

### 3. Execution Pipeline Documented

- [ ] Phase execution with TUI progress display is mentioned
- [ ] Auto-commit behavior after each phase is documented
- [ ] Code review loop with fix iterations is described
- [ ] Verification loop with fix iterations is described
- [ ] LLM-powered PR creation is mentioned

### 4. Architecture Section Updated

- [ ] All task types are listed (Init, Plan, Execute, Review, Verification, Fix, Pr)
- [ ] Three crates are documented (gba-cli, gba-core, gba-pm)
- [ ] Engine's dual mode operation (single-shot and interactive sessions) is mentioned

### 5. Configuration Options Complete

- [ ] `agent.model` option documented
- [ ] `agent.permission_mode` option documented with values
- [ ] `agent.budget_limit` option documented
- [ ] `prompts.include` option documented
- [ ] `git.auto_commit` option documented
- [ ] `git.branch_pattern` option documented
- [ ] `review.enabled` option documented
- [ ] `review.provider` option documented with values

### 6. Quality Checks

- [ ] README renders correctly as Markdown
- [ ] Code examples are accurate
- [ ] No broken links or references
- [ ] Consistent formatting throughout
