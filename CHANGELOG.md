# Changelog

## Unreleased

### Added

- Added TUI layout presets, trace session/timeline/graph views, result filtering/grouping commands, health summaries, persistent layout/filter state, and matching `tui-script` assertions.
- Added trace graph filters/collapse controls, filtered session diffs, session rename/merge/split commands, and trace store verify/repair/compact maintenance commands.
- Added DAP advanced breakpoint metadata for CLI launch/profile/session smoke/adapter sessions plus saved-profile transcript export.
- Added benchmark baseline/compare commands, TUI source rows in `bench all`, and smoke tiers (`fast`, `full`, `release`) with coverage for the new trace/DAP/bench/TUI surfaces.
- Added active trace sessions across CLI/TUI with `trace use/current`, TUI `trace session/current/sessions`, current-session trace panels, and current-session DAP profile generation.
- Added batch semantic tracing from `--targets-file` and `--from-query`, deduplicated semantic trace records, fallback confidence/reason metadata, and trace graph export in text/json/mermaid/dot.
- Added `tui-script assert ...` checks plus non-interactive `bench tui` source-load baselines and trace graph timing.
- Added config doctor checks for TUI key conflicts, theme color conflicts, and saved DAP profile conflicts or missing launch inputs.
- Added lightweight syntax highlighting for the ratatui preview pane and richer color styling across results, trace, debug, and activity panels.
- Added semantic trace recording with `trace semantic` and TUI `: trace semantic [relation]`, linking LSP/index graph edges into trace sessions.
- Added `tui-script` for headless TUI command playback, including selection, movement, waits, DAP smoke commands, and text/JSON summaries.
- Added query/index profiling and `index verify` for source/filter visibility, latency probes, and cache health checks.
- Added opt-in `graph semantic --cache` with refresh support for repeatable semantic graph diagnostics.
- Added `dap doctor` diagnostics for saved profiles, breakpoint paths, launch/attach validity, and adapter availability.
- Added TUI theme configuration for color, syntax highlighting, and low-color terminal behavior.
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
- Added interactive DAP/TUI controls for start/refresh/continue/pause/step, watch expressions, evaluation, and advanced breakpoint fields.
- Added LSP edit workflows for rename/code-action apply previews plus organize imports, outline, breadcrumbs, and semantic token inspection.
- Added `index daemon` and `index daemon-status` with polling refresh and heartbeat files.
- Added `trace insights` reports that summarize debug events, unresolved entries, hot files, and index-correlated nearby symbols.
- Added plugin env expansion, pre/post hooks, execution plans, schema output, strict doctor mode, and custom `{var.KEY}` variables.
- Added fielded `query` and `service query` commands for index/trace filtering by kind, language, path, text, status, priority, session, and tags.
- Added `service start/status/snapshot/stop` for foreground workspace snapshot generation across index, LSP provider health, trace, plugins, and workspace profile state.
- Added top-level `bench` probes for search, index, trace store, preview reads, and combined workspace benchmark reports.
- Added workspace profile management plus `workspace config-doctor` and `workspace config-schema`.
- Added `trace replay-plan` exports that turn a trace session into reproducible trace and optional DAP profile commands.
- Added TUI DAP continue/pause/step controls, watch clearing, and stopped-location jump/open commands.
- Added CI coverage for the full CLI smoke script, including the P57-P64 command surface.
- Added interactive TUI real-DAP sessions via `dap real <adapter-command>`, saved DAP profile launch with `dap start <profile>`, and active breakpoint synchronization with `dap sync` / `break sync`.
- Added structured DAP snapshots for threads, stack frames, scopes, variables, stop reasons, last events, and adapter errors while keeping the existing text summary fields.
- Added a partitioned TUI Debug panel for session, stack, variables, watches, and events, plus trace-to-breakpoint/profile commands.
- Added semantic query sources: `--source semantic` for LSP workspace symbols and `--source auto` for index + trace + LSP fusion without changing the fast default `all` source.
- Added index schema v3 content hashes, per-file symbol counts, indexed timestamps, richer incremental build reports, and hash-first stale detection.
- Added `workspace plan` and TUI Activity startup summaries for non-blocking index/LSP/profile readiness checks.
- Added P65-P72 tests and smoke coverage for workspace plans, semantic/auto query, index mutation refresh, DAP typed snapshots, and new TUI command suggestions.
- Added DAP adapter discovery with `dap adapters`, automatic adapter selection for `dap adapter-session auto`, and breakpoint verification reporting.
- Added TUI DAP restart/terminate/disconnect/adapters commands plus verified breakpoint rendering in the Debug panel.
- Added DAP stopped-location trace metadata with stack, variable, watch, and breakpoint context for later investigation replay.
- Added query `source:` field filters, query-plan explanation, latency output, and warning thresholds for `query` and `service query`.
- Added `workspace workflows` diagnostic templates and lazy startup-plan guidance for on-demand LSP/DAP and explicit index prewarm.
- Added P73-P80 smoke coverage for adapter discovery, query explanation/timing, source filtering, service query explanation, and workflow templates.
- Added global config `schema_version` compatibility diagnostics for legacy and future `fcs.toml` files.
- Added `scripts/release-check.sh fast|full` so local iteration can use a lightweight gate while release candidates still run the full package gate.
- Added DAP attach request support, adapter templates, capability reporting, and process-id-aware launch/profile/session-smoke commands.
- Added TUI DAP thread/frame selection plus variable expansion and paging commands for deeper debug-session inspection.
- Added grouped query execution with `OR`, `NOT`, parentheses, richer explain filters, and semantic-to-index fallback when LSP is unavailable.
- Added `index shards` planning reports for large workspaces and extended smoke coverage for shard planning, query AST explain, semantic fallback, DAP templates, and mock attach sessions.
- Added DAP adapter template schemas with launch/attach field lists, notes, and argument previews, plus DAP session state, last request/error, and variable page metadata in snapshots.
- Added richer TUI Debug panel status lines for DAP state, selected thread/frame, variable paging, and last request/error.
- Added a `search-to-debug-loop` workflow that connects query results, trace entries, semantic graph fallback, DAP profile generation, and the TUI debug surface.
- Added Query Engine v2 matching controls: `--mode fuzzy|exact|regex`, built-in query macros, saved workspace queries, and score explanations for `query` and `service query`.
- Added writable index shard caches with `index shards --write`, `index shard-status`, and `index shard-query`, including automatic fallback to the main index when shard metadata is stale.
- Added `graph semantic --fallback index` so semantic graph commands can still return index-derived edges when LSP is unavailable or empty.
- Added `workspace doctor-bundle` text/json support bundles with workspace, config, index, service, DAP, workflow, and saved-query diagnostics.
- Added P89-P96 unit and smoke coverage for DAP template/state metadata, Query v2, shard cache query/status, graph fallback, diagnostic workflows, and doctor bundles.

### Verification

- Expected release gate: `rtk cargo test`.
- Recommended full local gate: `rtk scripts/smoke.sh`.
- Recommended fast package gate: `rtk scripts/release-check.sh fast`.
- Recommended full package gate: `rtk scripts/release-check.sh full`.
- Verified locally with `rtk cargo clippy -- -D warnings`, `rtk cargo test`, index/workspace/query/service/bench/graph/DAP/trace CLI smoke, and TUI startup/command-palette smoke.

## 0.6.3

### Added

- Introduced the ratatui TUI workspace for persistent search, trace, LSP, and debug workflows.
- Added workspace-scoped trace persistence, trace export, debug profiles, history, and clangd-backed semantic navigation.
- Kept legacy skim picker commands for one-shot search, file, and symbol workflows.
