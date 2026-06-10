# Changelog

## Unreleased

### Added

- Added release readiness documentation for the ratatui tracing workspace, CLI smoke checks, trace export, and debug profile workflows.
- Added `scripts/smoke.sh` as a repeatable release smoke entrypoint. The script uses `rtk` for every shell command and keeps debug profile state under `/tmp` by default.
- Added lightweight clap command-tree tests to catch invalid CLI definitions before release.
- Added workspace index commands: `index status`, `index build`, and `index list`.
- Added index schema v2 metadata for file language, symbol language, symbol ranges, and lightweight parent symbols.
- Added incremental index builds that reuse cached symbols for unchanged files and only rescan new or modified files.
- Added `workspace advise` with build-system, language, clangd, index, and debug-target setup guidance.
- Added `graph imports` and clangd-backed `graph semantic` output in text or JSON format.
- Added graph Mermaid/DOT export plus basic `--depth`, `--fanout`, and repeatable `--exclude` controls.
- Added DAP launch request generation, request bundles, and persisted DAP profiles.
- Added a mockable DAP client with frame codec, request/response/event handling, and `dap session-smoke`.
- Added trace session listing, archive/unarchive, and per-session Markdown/JSON reports.
- Added trace entry note/status/priority updates plus session timeline and session diff exports.
- Added configurable global/project actions with `{workspace}`, `{file}`, `{line}`, and `{symbol}` template expansion.
- Added `actions list` and `actions run` CLI commands, including dry-run command preview.
- Added provider-aware LSP navigation for clangd and rust-analyzer, including workspace advice for Rust projects.
- Added a TUI command palette with completion/history, source/query commands, preview lock/scroll, and Debug-source deletion.
- Added TUI workbench upgrades: help overlay, pinned results, jump history, tracking-cycle actions, command suggestions, and richer sidebar/activity panes.
- Added LSP provider health helpers and restart/retry-aware client plumbing for clangd/rust-analyzer workflows.
- Added index stale/corrupt detection and `index doctor` guidance.
- Added `workspace detect` and `workspace doctor` with auto-generated project config, health checks, log/cache/latency advice, and suggested actions.
- Added offline `graph modules` and `graph calls` views alongside import and semantic graph exports.
- Added trace replay and structured report data for hypothesis/evidence/conclusion-oriented debugging reports.
- Added real DAP adapter process transport and `dap adapter-session` for non-interactive initialize/launch/configurationDone sessions.
- Added TUI workspace state persistence for mode/query, pins, jump stack, breakpoints, preview lock, and command history.
- Added TUI Debug panel DAP mock session summaries with threads, stack frames, scopes, variables, events, and sent commands.
- Added `hover`, `workspace-symbols`, and `lsp health` CLI entries for deeper LSP workflows.
- Added `index query`, `index repair`, and `index bench`, plus schema/message output in index status and doctor reports.
- Added filtered `trace list` and standalone `trace structured` exports.
- Added built-in action templates with `actions templates`, `actions init`, and `actions doctor`.
- Added asynchronous TUI DAP worker snapshots and trace recording for DAP stopped locations.
- Added deeper LSP CLI helpers: `lsp highlights`, `lsp refs`, `lsp rename`, `lsp code-actions`, and `lsp call-tree`.
- Added trace-session-to-debugger and trace-session-to-DAP profile generation with `debug from-trace` and `dap from-trace`.
- Added index maintenance and observability commands: `index stats`, `index compact`, `index prewarm`, `index refresh`, `index query --timing`, and `index bench --query`.
- Added declarative plugin manifests with `plugin list/show/doctor/templates/commands/init/run`.
- Added man page generation and local install dry-run support with `fcs man` and `scripts/install-local.sh`.
- Added `scripts/release-check.sh` and release metadata for Cargo packaging.

### Verification

- Expected release gate: `rtk cargo test`.
- Recommended full local gate: `rtk scripts/smoke.sh`.
- Recommended package gate: `rtk scripts/release-check.sh`.
- Verified locally with `rtk cargo clippy -- -D warnings`, `rtk cargo test`, index/workspace/graph/DAP/trace CLI smoke, and TUI startup/command-palette smoke.

## 0.6.3

### Added

- Introduced the ratatui TUI workspace for persistent search, trace, LSP, and debug workflows.
- Added workspace-scoped trace persistence, trace export, debug profiles, history, and clangd-backed semantic navigation.
- Kept legacy skim picker commands for one-shot search, file, and symbol workflows.
