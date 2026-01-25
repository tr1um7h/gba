# Changelog

All notable changes to this project will be documented in this file. See [conventional commits](https://www.conventionalcommits.org/) for commit guidelines.

---
## [gba-pm-v0.2.2] - 2026-01-25

### Bug Fixes

- **(clean)** preserve .gba directory for feature history - ([bfa11de](https://github.com/commit/bfa11dede0c061664c7cfb8162a8800262d6d96b)) - Tyr Chen
- **(clean)** suppress gh CLI progress output to fix console formatting - ([5c56972](https://github.com/commit/5c56972628eb2714c558fc46ba226d090324ea30)) - Tyr Chen
- **(gba-pm)** enhance init and execute prompts - ([fbb57ed](https://github.com/commit/fbb57ed791b2e07412ec05a3cf04108dc9b60454)) - Tyr Chen
- **(plan)** strengthen two-approval workflow and state.yml format - ([393cc9b](https://github.com/commit/393cc9b2d6737d90d89ef31de0678e2af48b1e64)) - Tyr Chen
- **(plan)** restrict file operations to .gba directory only - ([b02d590](https://github.com/commit/b02d590606eac637a1dcb8950487805ceafbcd8b)) - Tyr Chen
- **(plan)** improve state.yml format validation and error messages - ([6c1761f](https://github.com/commit/6c1761f20cf0a68dc6cf6a3dbe38621008a755e5)) - Tyr Chen
- **(plan)** only exit on explicit /done command, no auto-exit - ([59451e9](https://github.com/commit/59451e90bcc753aa76602488707b7e47252e7936)) - Tyr Chen
- **(run)** improve review/fix loop with better keyword matching and prompts - ([6ca40ee](https://github.com/commit/6ca40ee2a285eb90a90240dbcc46dcbf701960cb)) - Tyr Chen
- **(tui)** improve pipeline robustness and camelCase serialization - ([9c52592](https://github.com/commit/9c52592620162450ff0b8f5f69c61d8a504d4a8e)) - Tyr Chen
- **(tui)** clear terminal after exiting to prevent garbled output - ([dd375f0](https://github.com/commit/dd375f0efd952b9783caf436c56722cb69ea239e)) - Tyr Chen
- **(tui)** support Unicode input with grapheme cluster handling - ([7b779e9](https://github.com/commit/7b779e9dc588b831e9b852077b880e023a5369f8)) - Tyr Chen
- **(tui)** properly reset cursor and clear screen after TUI exit - ([47802fa](https://github.com/commit/47802fa5798525471b7fe84823c7639af57e79b0)) - Tyr Chen
- **(tui)** use ratatui::restore() for proper terminal cleanup - ([7ae80db](https://github.com/commit/7ae80db90a57a0a8572db2cf5b08cd75ad7dda63)) - Tyr Chen
- resolve tool use concurrency error and improve logging - ([5c1da93](https://github.com/commit/5c1da93cd1c6d9de1274a768abb5b8f9a1880b8b)) - Tyr Chen
- use Tools field for CLI compatibility and improve initialization - ([1d042da](https://github.com/commit/1d042da1e446370ac95d4be1cdd979644f16c8e1)) - Tyr Chen

### Documentation

- add GBA design specification - ([4d3d27a](https://github.com/commit/4d3d27a5b99263fb3ed3aeed425c2262bf55a193)) - Tyr Chen
- update design spec with config/state structure and prompts - ([84fe092](https://github.com/commit/84fe092898afe503530cb56062d92c33c52114cd)) - Tyr Chen

### Features

- **(clean)** add gba clean command to cleanup merged/closed PRs - ([af6a7d5](https://github.com/commit/af6a7d578c2402ffe9f6e2640fa3bfe20c344796)) - Tyr Chen
- **(clean)** update clean logic to delete closed PRs by default - ([26a3004](https://github.com/commit/26a30041295cb062157faa08da94a88a20a01a9e)) - Tyr Chen
- **(gba-pm)** add prompt templates - ([7801f37](https://github.com/commit/7801f37f0e9a962b8e7f716f44ed98068e68c5bd)) - Tyr Chen
- **(list)** show all features including those in planning - ([ccebb5d](https://github.com/commit/ccebb5dea9b4ecc97d04d0835c9d53563b7cd2d8)) - Tyr Chen
- **(plan)** create worktree first and organize specs under feature slug - ([36c07b9](https://github.com/commit/36c07b94ac1dbf8b262ff3e6730992a67980e9f3)) - Tyr Chen
- **(plan)** auto-detect default branch instead of hardcoding main - ([4fb5168](https://github.com/commit/4fb51689ca0250d6b3dd4eb66f779b73ffedb244)) - Tyr Chen
- **(plan)** support resuming incomplete planning sessions - ([fef8d9b](https://github.com/commit/fef8d9b669e2ff298ca8402d00a16c656003b204)) - Tyr Chen
- **(run)** add review/fix loop and improved execute flow - ([08cefed](https://github.com/commit/08cefedf701fa7f1ace7bc8c524aaf020a84c94f)) - Tyr Chen
- **(run)** integrate TUI for better execution progress display - ([06b75c4](https://github.com/commit/06b75c4de3db7e4cf02b456b609ecb61e0f560be)) - Tyr Chen
- **(run)** use LLM for PR creation with detailed descriptions - ([bb10080](https://github.com/commit/bb100807da94883fdcd45496d6e12d892cb090fe)) - Tyr Chen
- **(session)** use BypassPermissions mode for unattended execution - ([da16001](https://github.com/commit/da16001058019a78c9facf0f8579083b6b102b00)) - Tyr Chen
- **(state)** persist PR info to state.yml after PR creation (#4) - ([d8fd6cc](https://github.com/commit/d8fd6cc51c9e11739431e50837b3fca0a7ebd441)) - Tyr Chen
- **(tui)** improve plan TUI responsiveness and workflow - ([60a7712](https://github.com/commit/60a7712959916566225ed0e094c3508afa4855c2)) - Tyr Chen
- **(tui)** integrate code review and verification into TUI - ([805db5c](https://github.com/commit/805db5c5a5f1d917fc852140879773f4b30c3781)) - Tyr Chen
- **(update-readme)** comprehensive README.md update with complete documentation (#3) - ([a6fd891](https://github.com/commit/a6fd8918c23e2e17bc1bd28e9fe48dd3947016f7)) - Tyr Chen
- implement GBA (Geektime Bootcamp Agent) CLI tool (#1) - ([7e6e963](https://github.com/commit/7e6e9637cadc8802b3ccf880a5752ec39cdaee3a)) - Tyr Chen
- use TUI for run. Update plan logic. - ([d3bb7a6](https://github.com/commit/d3bb7a6a99e6702720b6a402452d7702aa65adbc)) - Tyr Chen

### Miscellaneous Chores

- init the project - ([f49dddd](https://github.com/commit/f49dddd0606bea4137bfb0811a2e4a6c88401369)) - Tyr Chen
- add claude-agent-sdk-rs as submodule - ([1b77bce](https://github.com/commit/1b77bce6138c4b9c0558d8e4f91ef5449faa8ec0)) - Tyr Chen
- add .chats back - ([f62dade](https://github.com/commit/f62dadef8f4a8c3b8941501c6c61588e647e3ba0)) - Tyr Chen
- move claude-agent-sdk-rs submodule to vendors directory - ([c2a4c35](https://github.com/commit/c2a4c352204ac6aa324351bdb5cbd70f2b16c548)) - Tyr Chen
- update CLAUDE.md - ([26864ba](https://github.com/commit/26864ba8b24ff6eb8adaca81848122b2e7658145)) - Tyr Chen
- add chat - ([8fcc10d](https://github.com/commit/8fcc10dcd69609028bd84acb60ec6612d86ef94a)) - Tyr Chen
- bump version to 0.2.1 - ([f75e921](https://github.com/commit/f75e921d666766e17202421e5190c8de0bb6aff0)) - Tyr Chen

### Refactoring

- **(gba-pm)** simplify prompt templates following convention over configuration - ([8c5b946](https://github.com/commit/8c5b946702c99af5fe5543ca8c3bae2503568e23)) - Tyr Chen
- **(plan)** generate state.yml in code after TUI exits - ([e18c720](https://github.com/commit/e18c7206c77a90674b242812685967375b60aef9)) - Tyr Chen
- **(run)** use worktree as working directory and update paths - ([786272d](https://github.com/commit/786272d359d990bcf8095652bb021d4827a8ad59)) - Tyr Chen
- **(run)** extract check-fix loop to reduce duplication - ([8f0978b](https://github.com/commit/8f0978b97844a037917ce9c1e216eb514030b059)) - Tyr Chen
- **(run)** introduce TaskContext to eliminate repeated worktree_path calculation - ([51026c6](https://github.com/commit/51026c643523c3548302a4eb1904376f12836b4a)) - Tyr Chen
- **(run)** introduce CheckResult enum for proper error handling semantics - ([83158c4](https://github.com/commit/83158c48c542505a02f22e3ea202c5dacd2b56a9)) - Tyr Chen
- **(run)** make keyword detection more robust with strict patterns - ([257dc7c](https://github.com/commit/257dc7c579cbcb3f243dc28b6c3ec6f0415c4262)) - Tyr Chen
- **(run)** break run_run into smaller orchestration functions - ([f305fc4](https://github.com/commit/f305fc42c8c5b5234a9546d33d0ae238d4e670ff)) - Tyr Chen
- convert to workspace structure - ([e550a48](https://github.com/commit/e550a48df45a6752e102622269219266667efa9f)) - Tyr Chen
- move task definitions to root tasks/ directory - ([4c0e26a](https://github.com/commit/4c0e26ac1adc45a15eef973d2183aa87b5c1eaa6)) - Tyr Chen
- DRY improvements - extract common utilities and remove unused code - ([fcc5ab3](https://github.com/commit/fcc5ab3bf5d0e62db077c688e906612cc79ef4dd)) - Tyr Chen

### Tests

- **(clean)** add comprehensive unit tests for clean command - ([0cf58d1](https://github.com/commit/0cf58d1f30b4077093dc10f43cab02c2dd97b794)) - Tyr Chen

<!-- generated by git-cliff -->
