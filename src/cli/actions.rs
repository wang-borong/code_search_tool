use clap::CommandFactory;
use skim::prelude::*;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use super::args::{
    make_result, parse_file_arg, parse_location_arg, parse_preview_arg, resolve_ignore_file, resolve_location_for_root,
    resolve_path_for_root,
};
use super::defs::{
    BenchAction, Cli, Commands, DapAction, DebugAction, GraphAction, HistoryAction, IgnoreAction, IndexAction,
    LspAction, PluginAction, ProjectAction, ServiceAction, TraceAction, WorkspaceAction, WorkspaceProfileAction,
};
use super::picker::run_code_item_picker;
use fcs::core::{CodeItem, Location};
use fcs::errors::AppError;
use fcs::ignore::IgnoreFile;
use fcs::search;

fn handle_search(
    pattern: &str,
    directory: Option<&String>,
    options: &[String],
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    // Step 1: Search using regex + ignore crates + default ignore patterns
    fcs::history::record("search", pattern, directory)?;
    let mut final_options = config.search.rg_options.clone();
    final_options.extend(options.iter().cloned());

    let ignore_path = resolve_ignore_file(directory);

    let results = search::search(pattern, directory, &final_options, &config.search.ignore, &ignore_path)?;
    let flat = results.flat();

    if flat.is_empty() {
        println!("No matches found");
        return Ok(());
    }

    let mut current_pattern = "".to_string();
    let delimiter = regex::Regex::new(":").unwrap();

    loop {
        // Step 2: Interactive select using Skim
        let (tx, rx): (SkimItemSender, SkimItemReceiver) = unbounded();
        let items: Vec<Arc<dyn SkimItem>> = flat
            .iter()
            .map(|result| std::sync::Arc::new(result.clone()) as std::sync::Arc<dyn SkimItem>)
            .collect();
        let _ = tx.send(items);
        drop(tx);

        let bind_opts = config.skim.binds.clone();

        let skim_options = SkimOptionsBuilder::default()
            .height(config.skim.height.as_str())
            .min_height(config.skim.min_height.as_str())
            .multi(true)
            .delimiter(delimiter.clone())
            .color(config.skim.color.as_str())
            .exact(config.skim.exact)
            .tac(config.skim.tac)
            .cycle(config.skim.cycle)
            .bind(bind_opts)
            .preview("")
            .preview_window(config.skim.preview_window.as_str())
            .query(current_pattern.clone())
            .build()
            .map_err(|e| AppError::Skim(e.to_string()))?;

        let output = Skim::run_with(skim_options, Some(rx)).ok();
        if output.is_none() {
            break;
        }
        let output = output.unwrap();
        current_pattern = output.query.clone();
        if output.is_abort {
            break;
        }

        // Step 3: Open selected results in editor
        for item in output.selected_items.iter() {
            let display = item.output().to_string();
            if let Some(result) = flat.iter().find(|r| r.display == display) {
                let location = Location::new(&result.path, Some(result.line_num), None);
                fcs::trace::record_location(&location, &result.display, "search")?;
                fcs::editor::open_file(
                    Path::new(&result.path),
                    Some(result.line_num),
                    None,
                    config.editor.command.as_deref(),
                )?;
            }
        }
    }

    Ok(())
}

fn handle_files(
    directory: Option<&String>,
    query: Option<&String>,
    options: &[String],
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    if let Some(query) = query {
        fcs::history::record("files", query, directory)?;
    }
    let ignore_path = resolve_ignore_file(directory);
    let items = fcs::files::find_files(directory, options, &config.search.ignore, &ignore_path)?;

    if items.is_empty() {
        println!("No files found");
        return Ok(());
    }

    run_code_item_picker(&items, query, config)
}

fn handle_symbols(
    directory: Option<&String>,
    query: Option<&String>,
    options: &[String],
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    if let Some(query) = query {
        fcs::history::record("symbol", query, directory)?;
    }
    let ignore_path = resolve_ignore_file(directory);
    let items = fcs::symbols::find_symbols(directory, options, &config.search.ignore, &ignore_path)?;

    if items.is_empty() {
        println!("No symbols found");
        return Ok(());
    }

    run_code_item_picker(&items, query, config)
}

fn handle_workspace_status(directory: Option<&String>, config: &fcs::config::Config) -> Result<(), AppError> {
    let status = fcs::workspace::status(directory, &config.lsp.clangd_command)?;
    print_workspace_status(&status);
    Ok(())
}

fn handle_workspace_init(directory: Option<&String>, config: &fcs::config::Config) -> Result<(), AppError> {
    let status = fcs::workspace::init(directory, &config.lsp.clangd_command)?;
    print_workspace_status(&status);
    println!("Initialized fcs workspace cache: {}", status.cache_dir.display());
    Ok(())
}

fn handle_workspace_advise(directory: Option<&String>, config: &fcs::config::Config) -> Result<(), AppError> {
    let report = fcs::workspace::advise(directory, &config.lsp.clangd_command)?;
    print_workspace_advice(&report);
    Ok(())
}

fn handle_workspace_detect(directory: Option<&String>) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let detection = fcs::workspace::detect_project(&root)?;

    println!("Root: {}", root.display());
    println!("Project type: {}", detection.project_type);
    println!("Build systems: {}", display_list(&detection.build_systems));
    println!("Languages: {}", display_list(&detection.languages));
    println!("Index roots: {}", display_list(&detection.index_roots));
    if detection.debug_targets.is_empty() {
        println!("Debug targets: none");
    } else {
        println!("Debug targets:");
        for target in detection.debug_targets {
            println!("  {}", target.display());
        }
    }
    if detection.suggested_actions.is_empty() {
        println!("Suggested actions: none");
    } else {
        println!("Suggested actions:");
        for action in detection.suggested_actions {
            println!("  {}", action.name);
        }
    }
    Ok(())
}

fn handle_workspace_plan(directory: Option<&String>, config: &fcs::config::Config) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let plan = fcs::workspace::startup_plan(&root, config)?;

    println!("Root: {}", plan.root.display());
    for line in fcs::workspace::startup_plan_lines(&plan) {
        println!("{line}");
    }
    if let Some(message) = plan.index.message.as_deref() {
        println!("index_message: {message}");
    }
    println!("lsp_command: {}", plan.lsp.command);
    println!("lsp_message: {}", plan.lsp.message);
    Ok(())
}

fn handle_workspace_workflows(
    directory: Option<&String>,
    format: &str,
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let workflows = fcs::workspace::diagnostic_workflows(&root, config)?;
    print!("{}", fcs::workspace::format_diagnostic_workflows(&workflows, format)?);
    Ok(())
}

fn handle_workspace_profile(action: WorkspaceProfileAction) -> Result<(), AppError> {
    match action {
        WorkspaceProfileAction::Save {
            name,
            directory,
            description,
            index_roots,
        } => {
            let profile = fcs::workspace::save_profile(&name, directory.as_ref(), description, &index_roots)?;
            println!("Saved workspace profile: {}", format_workspace_profile(&profile, false));
        }
        WorkspaceProfileAction::List => {
            let store = fcs::workspace::list_profiles()?;
            if store.profiles.is_empty() {
                println!("No workspace profiles");
            } else {
                for profile in store.profiles {
                    let active = store.active.as_deref() == Some(profile.name.as_str());
                    println!("{}", format_workspace_profile(&profile, active));
                }
            }
        }
        WorkspaceProfileAction::Show { name } => {
            let profile = fcs::workspace::get_profile(&name)?;
            println!("{}", format_workspace_profile_detail(&profile));
        }
        WorkspaceProfileAction::Use { name } => {
            let profile = fcs::workspace::use_profile(&name)?;
            println!("Active workspace profile: {}", format_workspace_profile(&profile, true));
        }
        WorkspaceProfileAction::Current => match fcs::workspace::current_profile()? {
            Some(profile) => println!("{}", format_workspace_profile_detail(&profile)),
            None => println!("No active workspace profile"),
        },
        WorkspaceProfileAction::Delete { name } => {
            if fcs::workspace::delete_profile(&name)? {
                println!("Deleted workspace profile: {name}");
            } else {
                println!("Workspace profile not found: {name}");
            }
        }
    }
    Ok(())
}

fn handle_workspace_config_doctor(directory: Option<&String>, strict: bool) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let diagnostics = fcs::workspace::config_diagnostics(&root)?;
    let mut failed = 0usize;
    for diagnostic in &diagnostics {
        if !diagnostic.ok {
            failed += 1;
        }
        let status = if diagnostic.ok { "ok" } else { "fail" };
        println!("[{status}] {}: {}", diagnostic.name, diagnostic.detail);
    }
    if strict && failed > 0 {
        return Err(AppError::General(format!(
            "Project config doctor found {failed} failing diagnostic(s)"
        )));
    }
    Ok(())
}

fn handle_workspace_config_schema(format: &str) -> Result<(), AppError> {
    print!("{}", fcs::workspace::project_config_schema(format)?);
    Ok(())
}

fn format_workspace_profile(profile: &fcs::workspace::WorkspaceProfile, active: bool) -> String {
    let marker = if active { " [active]" } else { "" };
    format!(
        "{}{} root={} project_type={} index_roots={}",
        profile.name,
        marker,
        profile.root.display(),
        profile.project_type,
        display_list(&profile.index_roots)
    )
}

fn format_workspace_profile_detail(profile: &fcs::workspace::WorkspaceProfile) -> String {
    let mut output = String::new();
    output.push_str(&format!("name: {}\n", profile.name));
    output.push_str(&format!("root: {}\n", profile.root.display()));
    output.push_str(&format!("project_type: {}\n", profile.project_type));
    output.push_str(&format!("languages: {}\n", display_list(&profile.languages)));
    output.push_str(&format!("build_systems: {}\n", display_list(&profile.build_systems)));
    output.push_str(&format!("index_roots: {}\n", display_list(&profile.index_roots)));
    output.push_str(&format!(
        "description: {}\n",
        profile.description.as_deref().unwrap_or("-")
    ));
    output.push_str(&format!("updated_at_unix: {}\n", profile.updated_at_unix));
    output
}

fn handle_index_status(directory: Option<&String>) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let status = fcs::index::status(&root)?;

    println!("Root: {}", root.display());
    println!("Index: {}", status.path.display());
    println!("exists: {}", status.exists);
    println!("schema_status: {}", format_index_schema_status(status.schema_status));
    if status.exists {
        println!("version: {}", status.version.unwrap_or(0));
        println!("files: {}", status.file_count);
        println!("symbols: {}", status.symbol_count);
        println!("built_at_unix: {}", status.built_at_unix.unwrap_or(0));
        println!("changed_tracked_files: {}", status.changed_tracked_files);
        println!("missing_tracked_files: {}", status.missing_tracked_files);
        println!("stale: {}", status.is_stale);
        println!("corrupt: {}", status.is_corrupt);
    }
    if let Some(message) = status.message.as_deref() {
        println!("message: {message}");
    }
    Ok(())
}

fn handle_index_stats(directory: Option<&String>) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let stats = fcs::index::stats(&root)?;

    println!("Index: {}", stats.path.display());
    println!("exists: {}", stats.exists);
    println!("files: {}", stats.file_count);
    println!("symbols: {}", stats.symbol_count);
    println!("source_size_bytes: {}", stats.source_size_bytes);
    println!("index_size_bytes: {}", stats.index_size_bytes);
    println!("built_at_unix: {}", stats.built_at_unix.unwrap_or(0));
    println!("languages: {}", format_index_counts(&stats.languages));
    println!("symbol_kinds: {}", format_index_counts(&stats.symbol_kinds));
    Ok(())
}

fn handle_index_doctor(directory: Option<&String>) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let status = fcs::index::status(&root)?;

    println!("Root: {}", root.display());
    println!("Index: {}", status.path.display());
    println!("exists: {}", status.exists);
    println!("schema_status: {}", format_index_schema_status(status.schema_status));
    println!("stale: {}", status.is_stale);
    println!("corrupt: {}", status.is_corrupt);
    println!("changed_tracked_files: {}", status.changed_tracked_files);
    println!("missing_tracked_files: {}", status.missing_tracked_files);
    if let Some(message) = status.message.as_deref() {
        println!("message: {message}");
    }
    if !status.exists
        || status.is_stale
        || status.is_corrupt
        || status.changed_tracked_files > 0
        || status.missing_tracked_files > 0
    {
        println!("action: fcs index build {}", root.display());
    } else {
        println!("action: none");
    }
    Ok(())
}

fn handle_index_compact(directory: Option<&String>, dry_run: bool) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let report = fcs::index::compact(&root, dry_run)?;

    println!("Index: {}", report.path.display());
    println!("dry_run: {}", report.dry_run);
    println!("original_bytes: {}", report.original_bytes);
    println!("compacted_bytes: {}", report.compacted_bytes);
    println!("size_delta_bytes: {}", report.size_delta_bytes);
    println!("wrote: {}", report.wrote);
    Ok(())
}

fn handle_index_prewarm(directory: Option<&String>) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let report = fcs::index::prewarm(&root)?;

    println!("Index: {}", report.path.display());
    println!("loaded: {}", report.loaded);
    println!("files: {}", report.file_count);
    println!("symbols: {}", report.symbol_count);
    println!("bytes_touched: {}", report.bytes_touched);
    Ok(())
}

fn handle_index_refresh(
    directory: Option<&String>,
    options: &[String],
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let root_arg = root.to_string_lossy().to_string();
    let ignore_path = resolve_ignore_file(Some(&root_arg));
    let report = fcs::index::refresh(&root, options, &config.search.ignore, &ignore_path)?;

    println!("Index: {}", report.path.display());
    println!("rebuilt: {}", report.rebuilt);
    println!("reason: {}", report.reason);
    if let Some(build_report) = report.build_report {
        println!("files: {}", build_report.file_count);
        println!("symbols: {}", build_report.symbol_count);
        println!("unchanged_files: {}", build_report.unchanged_files);
        println!("changed_files: {}", build_report.changed_files);
        println!("removed_files: {}", build_report.removed_files);
        println!("added_files: {}", build_report.added_files);
        println!("reindexed_files: {}", build_report.reindexed_files);
        println!("reused_symbol_files: {}", build_report.reused_symbol_files);
        println!(
            "changed_paths_sample: {}",
            display_list(&build_report.changed_paths_sample)
        );
        println!(
            "removed_paths_sample: {}",
            display_list(&build_report.removed_paths_sample)
        );
        println!("elapsed_ms: {}", build_report.elapsed_ms);
    }
    Ok(())
}

fn handle_index_daemon(
    directory: Option<&String>,
    interval_ms: u64,
    max_cycles: Option<usize>,
    foreground: bool,
    options: &[String],
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let root_arg = root.to_string_lossy().to_string();
    let ignore_path = resolve_ignore_file(Some(&root_arg));
    let report = fcs::index::run_polling_daemon(
        &root,
        options,
        &config.search.ignore,
        &ignore_path,
        fcs::index::IndexDaemonOptions {
            interval_ms,
            max_cycles,
        },
    )?;

    println!("Index daemon heartbeat: {}", report.heartbeat_path.display());
    println!("cycles: {}", report.cycles.last().map_or(0, |cycle| cycle.cycle));
    println!("rebuilds: {}", report.rebuilds);
    if foreground {
        for cycle in &report.cycles {
            println!(
                "cycle {}: rebuilt={} reason={} files={} symbols={} elapsed_ms={}",
                cycle.cycle, cycle.rebuilt, cycle.reason, cycle.file_count, cycle.symbol_count, cycle.elapsed_ms
            );
        }
    } else if let Some(cycle) = report.cycles.last() {
        println!(
            "last_cycle: rebuilt={} reason={} files={} symbols={} elapsed_ms={}",
            cycle.rebuilt, cycle.reason, cycle.file_count, cycle.symbol_count, cycle.elapsed_ms
        );
    }
    Ok(())
}

fn handle_index_daemon_status(directory: Option<&String>) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let status = fcs::index::daemon_status(&root)?;

    println!("Heartbeat: {}", status.path.display());
    println!("exists: {}", status.exists);
    if let Some(heartbeat) = status.heartbeat {
        println!("root: {}", heartbeat.root);
        println!("pid: {}", heartbeat.pid);
        println!("started_at_unix: {}", heartbeat.started_at_unix);
        println!("updated_at_unix: {}", heartbeat.updated_at_unix);
        println!("interval_ms: {}", heartbeat.interval_ms);
        println!("cycles: {}", heartbeat.cycles);
        println!("rebuilds: {}", heartbeat.rebuilds);
        println!("last_rebuilt: {}", heartbeat.last_rebuilt);
        println!("last_reason: {}", heartbeat.last_reason);
        println!("files: {}", heartbeat.last_file_count);
        println!("symbols: {}", heartbeat.last_symbol_count);
        println!("stale: {}", status.stale);
    }
    Ok(())
}

fn handle_index_build(
    directory: Option<&String>,
    options: &[String],
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let root_arg = root.to_string_lossy().to_string();
    let ignore_path = resolve_ignore_file(Some(&root_arg));
    let report = fcs::index::build(&root, options, &config.search.ignore, &ignore_path)?;

    println!("Built index: {}", report.path.display());
    println!("files: {}", report.file_count);
    println!("symbols: {}", report.symbol_count);
    println!("unchanged_files: {}", report.unchanged_files);
    println!("changed_files: {}", report.changed_files);
    println!("removed_files: {}", report.removed_files);
    println!("added_files: {}", report.added_files);
    println!("reindexed_files: {}", report.reindexed_files);
    println!("reused_symbol_files: {}", report.reused_symbol_files);
    println!("changed_paths_sample: {}", display_list(&report.changed_paths_sample));
    println!("removed_paths_sample: {}", display_list(&report.removed_paths_sample));
    println!("elapsed_ms: {}", report.elapsed_ms);
    Ok(())
}

fn handle_index_list(directory: Option<&String>, kind: &str, limit: usize) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let kind = fcs::index::IndexListKind::parse(kind)?;
    let entries = fcs::index::list(&root, kind, limit)?;

    if entries.is_empty() {
        println!("Index is empty or missing");
        return Ok(());
    }

    for entry in entries {
        println!("{entry}");
    }
    Ok(())
}

fn handle_index_query(
    directory: Option<&String>,
    kind: &str,
    query: &str,
    limit: usize,
    timing: bool,
    warn_ms: Option<u64>,
) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let kind = fcs::index::IndexListKind::parse(kind)?;
    let start = Instant::now();
    let entries = fcs::index::query(&root, kind, query, limit)?;
    let elapsed_ms = start.elapsed().as_millis();

    if entries.is_empty() {
        println!("No indexed entries matched");
        if timing {
            println!("timing_ms: {elapsed_ms}");
        }
        return Ok(());
    }

    for entry in entries {
        println!("{entry}");
    }
    if timing {
        println!("timing_ms: {elapsed_ms}");
    }
    if warn_ms.is_some_and(|threshold| elapsed_ms > threshold as u128) {
        eprintln!("warning: index query took {elapsed_ms}ms");
    }
    Ok(())
}

fn handle_index_repair(
    directory: Option<&String>,
    options: &[String],
    force: bool,
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let status = fcs::index::status(&root)?;
    let needs_repair = !status.exists
        || status.is_stale
        || status.is_corrupt
        || status.changed_tracked_files > 0
        || status.missing_tracked_files > 0;
    if !force && !needs_repair {
        println!("Index does not need repair: {}", status.path.display());
        return Ok(());
    }

    println!("Repairing index: {}", status.path.display());
    let root_arg = root.to_string_lossy().to_string();
    handle_index_build(Some(&root_arg), options, config)
}

fn handle_index_bench(
    directory: Option<&String>,
    include_build: bool,
    limit: usize,
    query: &str,
    options: &[String],
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let mut rows = Vec::new();

    let start = Instant::now();
    let _status = fcs::index::status(&root)?;
    rows.push(("index_status", start.elapsed().as_millis()));

    if include_build {
        let root_arg = root.to_string_lossy().to_string();
        let ignore_path = resolve_ignore_file(Some(&root_arg));
        let start = Instant::now();
        let _report = fcs::index::build(&root, options, &config.search.ignore, &ignore_path)?;
        rows.push(("index_build", start.elapsed().as_millis()));
    }

    let start = Instant::now();
    let _index = fcs::index::load(&root)?;
    rows.push(("index_load", start.elapsed().as_millis()));

    let start = Instant::now();
    let _files = fcs::index::list(&root, fcs::index::IndexListKind::Files, limit)?;
    rows.push(("index_list_files", start.elapsed().as_millis()));

    let start = Instant::now();
    let _symbols = fcs::index::list(&root, fcs::index::IndexListKind::Symbols, limit)?;
    rows.push(("index_list_symbols", start.elapsed().as_millis()));

    let start = Instant::now();
    let _query = fcs::index::query(&root, fcs::index::IndexListKind::Symbols, query, limit)?;
    rows.push(("index_query_symbols", start.elapsed().as_millis()));

    for (name, elapsed_ms) in &rows {
        println!("{name}\t{elapsed_ms}");
    }
    write_latency_smoke(&root, &rows)?;
    Ok(())
}

struct QueryRequest<'a> {
    expression: &'a str,
    directory: Option<&'a String>,
    source: &'a str,
    limit: usize,
    format: &'a str,
    explain: bool,
    timing: bool,
    warn_ms: Option<u128>,
}

fn handle_query(request: QueryRequest<'_>, config: &fcs::config::Config) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(request.directory)?;
    let source = fcs::query::QuerySource::parse(request.source)?;
    if request.explain {
        let explanation = fcs::query::explain(request.expression, source);
        print!("{}", fcs::query::format_explanation(&explanation, request.format)?);
        return Ok(());
    }
    let start = Instant::now();
    let matches = fcs::query::run_with_config(&root, request.expression, source, request.limit, Some(&config.lsp))?;
    let elapsed_ms = start.elapsed().as_millis();
    print!("{}", fcs::query::format_matches(&matches, request.format)?);
    if request.timing {
        println!("timing_ms: {elapsed_ms}");
    }
    if request.warn_ms.is_some_and(|threshold| elapsed_ms > threshold) {
        eprintln!("warning: query took {elapsed_ms}ms");
    }
    Ok(())
}

fn handle_service_start(
    directory: Option<&String>,
    interval_ms: u64,
    max_cycles: Option<usize>,
    foreground: bool,
    options: &[String],
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let root_arg = root.to_string_lossy().to_string();
    let ignore_path = resolve_ignore_file(Some(&root_arg));
    let report = fcs::service::run_daemon(
        &root,
        options,
        &config.search.ignore,
        &ignore_path,
        config,
        fcs::service::ServiceOptions {
            interval_ms,
            max_cycles,
        },
    )?;

    println!("Service heartbeat: {}", report.heartbeat_path.display());
    println!("Service snapshot: {}", report.snapshot_path.display());
    println!("cycles: {}", report.cycles.last().map_or(0, |cycle| cycle.cycle));
    if foreground {
        for cycle in &report.cycles {
            println!(
                "cycle {}: index_rebuilt={} reason={} elapsed_ms={}",
                cycle.cycle, cycle.index_rebuilt, cycle.index_reason, cycle.elapsed_ms
            );
        }
    }
    Ok(())
}

fn handle_service_status(directory: Option<&String>) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let status = fcs::service::status(&root)?;
    println!("Heartbeat: {}", status.heartbeat_path.display());
    println!("Snapshot: {}", status.snapshot_path.display());
    println!("Stop file: {}", status.stop_path.display());
    println!("exists: {}", status.heartbeat.is_some());
    println!("stale: {}", status.stale);
    println!("stop_requested: {}", status.stop_requested);
    if let Some(heartbeat) = status.heartbeat {
        println!("root: {}", heartbeat.root);
        println!("pid: {}", heartbeat.pid);
        println!("started_at_unix: {}", heartbeat.started_at_unix);
        println!("updated_at_unix: {}", heartbeat.updated_at_unix);
        println!("interval_ms: {}", heartbeat.interval_ms);
        println!("cycles: {}", heartbeat.cycles);
        println!("last_index_rebuilt: {}", heartbeat.last_index_rebuilt);
        println!("last_index_reason: {}", heartbeat.last_index_reason);
    }
    if let Some(snapshot) = status.snapshot {
        println!("snapshot_index_files: {}", snapshot.index.files);
        println!("snapshot_index_symbols: {}", snapshot.index.symbols);
        println!("snapshot_trace_entries: {}", snapshot.trace.entries);
    }
    Ok(())
}

fn handle_service_snapshot(
    directory: Option<&String>,
    format: &str,
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let snapshot = fcs::service::snapshot(&root, config)?;
    print!("{}", fcs::service::format_snapshot(&snapshot, format)?);
    Ok(())
}

fn handle_service_stop(directory: Option<&String>) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let path = fcs::service::request_stop(&root)?;
    println!("Requested service stop: {}", path.display());
    Ok(())
}

fn handle_bench_report(
    mut report: fcs::bench::BenchReport,
    format: &str,
    warn_ms: Option<u128>,
) -> Result<(), AppError> {
    report.push_warning_rows(warn_ms);
    print!("{}", fcs::bench::format_report(&report, format)?);
    if !report.warnings.is_empty() {
        eprintln!(
            "warning: {} benchmark probe(s) exceeded threshold",
            report.warnings.len()
        );
    }
    Ok(())
}

fn handle_bench_all(
    directory: Option<&String>,
    format: &str,
    warn_ms: Option<u128>,
    limit: usize,
    query: &str,
    options: &[String],
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let root_arg = root.to_string_lossy().to_string();
    let ignore_path = resolve_ignore_file(Some(&root_arg));
    let report = fcs::bench::run_all(&root, query, limit, options, &config.search.ignore, &ignore_path)?;
    handle_bench_report(report, format, warn_ms)
}

fn handle_bench_search(
    pattern: &str,
    directory: Option<&String>,
    format: &str,
    warn_ms: Option<u128>,
    options: &[String],
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let root_arg = root.to_string_lossy().to_string();
    let ignore_path = resolve_ignore_file(Some(&root_arg));
    let report = fcs::bench::run_search(&root, pattern, options, &config.search.ignore, &ignore_path)?;
    handle_bench_report(report, format, warn_ms)
}

struct BenchIndexInput<'a> {
    directory: Option<&'a String>,
    format: &'a str,
    warn_ms: Option<u128>,
    build: bool,
    limit: usize,
    query: &'a str,
    options: &'a [String],
    config: &'a fcs::config::Config,
}

fn handle_bench_index(input: BenchIndexInput<'_>) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(input.directory)?;
    let root_arg = root.to_string_lossy().to_string();
    let ignore_path = resolve_ignore_file(Some(&root_arg));
    let report = fcs::bench::run_index(
        &root,
        input.build,
        input.limit,
        input.query,
        input.options,
        &input.config.search.ignore,
        &ignore_path,
    )?;
    handle_bench_report(report, input.format, input.warn_ms)
}

fn handle_bench_trace(format: &str, warn_ms: Option<u128>) -> Result<(), AppError> {
    let report = fcs::bench::run_trace()?;
    handle_bench_report(report, format, warn_ms)
}

fn handle_bench_preview(target: &str, format: &str, warn_ms: Option<u128>) -> Result<(), AppError> {
    let (path, _line, _height) = parse_preview_arg(target)?;
    let report = fcs::bench::run_preview_read(Path::new(&path))?;
    handle_bench_report(report, format, warn_ms)
}

fn handle_definition(target: &str, directory: Option<&String>, config: &fcs::config::Config) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let location = resolve_location_for_root(parse_location_arg(target)?, &root);
    let mut client = fcs::lsp::LspClient::start_for_path(location.path(), &root, &config.lsp)?;
    let items = client.definition(&location)?;

    if items.is_empty() {
        println!("No definition found");
        return Ok(());
    }

    if items.len() == 1 {
        fcs::trace::record_code_item(&items[0], "definition")?;
        fcs::editor::open_location(&items[0].location, config.editor.command.as_deref())?;
        return Ok(());
    }

    run_code_item_picker(&items, None, config)
}

fn handle_references(target: &str, directory: Option<&String>, config: &fcs::config::Config) -> Result<(), AppError> {
    handle_lsp_location_query(target, directory, config, "references", fcs::lsp::LspClient::references)
}

fn handle_type_definition(
    target: &str,
    directory: Option<&String>,
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    handle_lsp_location_query(
        target,
        directory,
        config,
        "type definitions",
        fcs::lsp::LspClient::type_definition,
    )
}

fn handle_implementation(
    target: &str,
    directory: Option<&String>,
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    handle_lsp_location_query(
        target,
        directory,
        config,
        "implementations",
        fcs::lsp::LspClient::implementation,
    )
}

fn handle_incoming_calls(
    target: &str,
    directory: Option<&String>,
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    handle_lsp_location_query(
        target,
        directory,
        config,
        "incoming calls",
        fcs::lsp::LspClient::incoming_calls,
    )
}

fn handle_outgoing_calls(
    target: &str,
    directory: Option<&String>,
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    handle_lsp_location_query(
        target,
        directory,
        config,
        "outgoing calls",
        fcs::lsp::LspClient::outgoing_calls,
    )
}

fn handle_lsp_location_query(
    target: &str,
    directory: Option<&String>,
    config: &fcs::config::Config,
    label: &str,
    query: fn(&mut fcs::lsp::LspClient, &Location) -> Result<Vec<CodeItem>, AppError>,
) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let location = resolve_location_for_root(parse_location_arg(target)?, &root);
    let mut client = fcs::lsp::LspClient::start_for_path(location.path(), &root, &config.lsp)?;
    let items = query(&mut client, &location)?;

    if items.is_empty() {
        println!("No {label} found");
        return Ok(());
    }

    run_code_item_picker(&items, None, config)
}

fn handle_document_symbols(
    target: &str,
    directory: Option<&String>,
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let path = resolve_path_for_root(parse_file_arg(target), &root);
    let mut client = fcs::lsp::LspClient::start_for_path(&path, &root, &config.lsp)?;
    let items = client.document_symbols(&path)?;

    if items.is_empty() {
        println!("No document symbols found");
        return Ok(());
    }

    run_code_item_picker(&items, None, config)
}

fn handle_diagnostics(target: &str, directory: Option<&String>, config: &fcs::config::Config) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let path = resolve_path_for_root(parse_file_arg(target), &root);
    let mut client = fcs::lsp::LspClient::start_for_path(&path, &root, &config.lsp)?;
    let items = client.diagnostics(&path)?;

    if items.is_empty() {
        println!("No diagnostics found");
        return Ok(());
    }

    run_code_item_picker(&items, None, config)
}

fn handle_hover(target: &str, directory: Option<&String>, config: &fcs::config::Config) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let location = resolve_location_for_root(parse_location_arg(target)?, &root);
    let mut client = fcs::lsp::LspClient::start_for_path(location.path(), &root, &config.lsp)?;
    println!("{}", client.hover(&location)?);
    Ok(())
}

fn handle_workspace_symbols(
    query: &str,
    directory: Option<&String>,
    limit: usize,
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let mut client = fcs::lsp::LspClient::start_for_workspace(&root, &config.lsp)?;
    let mut items = client.workspace_symbols(query)?;
    if items.is_empty() {
        println!("No workspace symbols found");
        return Ok(());
    }
    items.truncate(limit.max(1));
    let query = query.to_string();
    run_code_item_picker(&items, Some(&query), config)
}

fn handle_lsp_health(
    directory: Option<&String>,
    file: Option<&String>,
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let provider = match file {
        Some(file) => {
            let path = resolve_path_for_root(parse_file_arg(file), &root);
            fcs::lsp::provider_for_path(&path, &config.lsp)?
        }
        None => fcs::lsp::provider_for_workspace(&root, &config.lsp),
    };
    let health = fcs::lsp::provider_health(&provider);
    println!("Root: {}", root.display());
    println!("provider: {}", provider.name());
    println!("command: {}", health.command);
    println!("status: {}", format_lsp_provider_status(health.status));
    println!("version: {}", health.version.as_deref().unwrap_or("-"));
    println!("message: {}", health.message);
    Ok(())
}

fn handle_lsp_highlights(
    target: &str,
    directory: Option<&String>,
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    let (mut client, location) = lsp_client_for_location(target, directory, config)?;
    let items = client.document_highlights(&location)?;
    print_code_item_list("No document highlights found", &items);
    Ok(())
}

fn handle_lsp_grouped_refs(
    target: &str,
    directory: Option<&String>,
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    let (mut client, location) = lsp_client_for_location(target, directory, config)?;
    print!("{}", client.references_grouped(&location)?);
    Ok(())
}

fn handle_lsp_rename(
    target: &str,
    new_name: &str,
    directory: Option<&String>,
    apply: bool,
    dry_run: bool,
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    let (mut client, location) = lsp_client_for_location(target, directory, config)?;
    if apply {
        let report = client.apply_rename(&location, new_name, dry_run)?;
        print_workspace_edit_report(&report)?;
    } else {
        print!("{}", client.rename_preview(&location, new_name)?);
    }
    Ok(())
}

fn handle_lsp_code_actions(
    target: &str,
    directory: Option<&String>,
    format: &str,
    apply: Option<usize>,
    dry_run: bool,
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    let (mut client, location) = lsp_client_for_location(target, directory, config)?;
    if let Some(index) = apply {
        let report = client.apply_code_action(&location, index, dry_run)?;
        print_workspace_edit_report(&report)?;
        return Ok(());
    }

    let actions = client.code_action_candidates(&location)?;
    print_code_actions(&actions, format)?;
    Ok(())
}

fn handle_lsp_organize_imports(
    target: &str,
    directory: Option<&String>,
    apply: bool,
    dry_run: bool,
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let path = resolve_path_for_root(parse_file_arg(target), &root);
    let mut client = fcs::lsp::LspClient::start_for_path(&path, &root, &config.lsp)?;
    let actions = client.organize_imports_candidates(&path)?;
    if !apply {
        print_code_actions(&actions, "text")?;
        return Ok(());
    }

    let action = actions
        .first()
        .ok_or_else(|| AppError::General("No organize-imports action found".to_string()))?;
    let report = fcs::lsp::apply_workspace_edit(&action.edit, dry_run)?;
    print_workspace_edit_report(&report)?;
    Ok(())
}

fn handle_lsp_outline(
    target: &str,
    directory: Option<&String>,
    format: &str,
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let path = resolve_path_for_root(parse_file_arg(target), &root);
    let mut client = fcs::lsp::LspClient::start_for_path(&path, &root, &config.lsp)?;
    let outline = client.document_outline(&path)?;
    match format {
        "tree" | "text" => print!("{}", fcs::lsp::format_outline_text(&outline)),
        "json" => println!(
            "{}",
            serde_json::to_string_pretty(&outline).map_err(|err| AppError::General(err.to_string()))?
        ),
        other => return Err(AppError::General(format!("Unsupported outline format: {other}"))),
    }
    Ok(())
}

fn handle_lsp_breadcrumbs(
    target: &str,
    directory: Option<&String>,
    format: &str,
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    let (mut client, location) = lsp_client_for_location(target, directory, config)?;
    let breadcrumbs = client.breadcrumbs(&location)?;
    match format {
        "text" => print!("{}", fcs::lsp::format_breadcrumbs_text(&breadcrumbs)),
        "json" => println!(
            "{}",
            serde_json::to_string_pretty(&breadcrumbs).map_err(|err| AppError::General(err.to_string()))?
        ),
        other => return Err(AppError::General(format!("Unsupported breadcrumbs format: {other}"))),
    }
    Ok(())
}

fn handle_lsp_semantic_tokens(
    target: &str,
    directory: Option<&String>,
    line: Option<usize>,
    format: &str,
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let path = resolve_path_for_root(parse_file_arg(target), &root);
    let mut client = fcs::lsp::LspClient::start_for_path(&path, &root, &config.lsp)?;
    let tokens = client.semantic_tokens(&path, line)?;
    match format {
        "text" => print!("{}", fcs::lsp::format_semantic_tokens_text(&tokens)),
        "json" => println!(
            "{}",
            serde_json::to_string_pretty(&tokens).map_err(|err| AppError::General(err.to_string()))?
        ),
        other => return Err(AppError::General(format!("Unsupported semantic token format: {other}"))),
    }
    Ok(())
}

fn handle_lsp_call_tree(
    target: &str,
    directory: Option<&String>,
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    let (mut client, location) = lsp_client_for_location(target, directory, config)?;
    print!("{}", client.call_tree(&location)?);
    Ok(())
}

fn lsp_client_for_location(
    target: &str,
    directory: Option<&String>,
    config: &fcs::config::Config,
) -> Result<(fcs::lsp::LspClient, Location), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let location = resolve_location_for_root(parse_location_arg(target)?, &root);
    let client = fcs::lsp::LspClient::start_for_path(location.path(), &root, &config.lsp)?;
    Ok((client, location))
}

fn print_code_item_list(empty_message: &str, items: &[CodeItem]) {
    if items.is_empty() {
        println!("{empty_message}");
        return;
    }

    for item in items {
        println!("{}", item.display_text());
    }
}

fn print_code_actions(actions: &[fcs::lsp::CodeActionCandidate], format: &str) -> Result<(), AppError> {
    if actions.is_empty() {
        println!("No code actions found");
        return Ok(());
    }

    match format {
        "text" => {
            for (index, action) in actions.iter().enumerate() {
                let edit_count = action.edit.edits.len();
                let command = action.command.as_deref().unwrap_or("-");
                println!(
                    "{}. {} [{}] edits={} command={}",
                    index + 1,
                    action.title,
                    action.kind,
                    edit_count,
                    command
                );
            }
        }
        "json" => println!(
            "{}",
            serde_json::to_string_pretty(actions).map_err(|err| AppError::General(err.to_string()))?
        ),
        other => return Err(AppError::General(format!("Unsupported code action format: {other}"))),
    }
    Ok(())
}

fn print_workspace_edit_report(report: &fcs::lsp::WorkspaceEditApplyReport) -> Result<(), AppError> {
    print!("{}", report.preview);
    println!("dry_run: {}", report.dry_run);
    println!("edit_count: {}", report.edit_count);
    println!("changed_files: {}", display_paths(&report.changed_files));
    if !report.unsupported_operations.is_empty() {
        println!("unsupported_operations: {}", report.unsupported_operations.join(", "));
    }
    Ok(())
}

struct GraphSemanticInput<'a> {
    target: &'a str,
    relation: &'a str,
    format: &'a str,
    depth: usize,
    fanout: usize,
    exclude: &'a [String],
    directory: Option<&'a String>,
    config: &'a fcs::config::Config,
}

fn handle_graph_semantic(input: GraphSemanticInput<'_>) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(input.directory)?;
    let location = resolve_location_for_root(parse_location_arg(input.target)?, &root);
    let mut client = fcs::lsp::LspClient::start_for_path(location.path(), &root, &input.config.lsp)?;
    let items = match input.relation {
        "references" | "refs" => client.references(&location)?,
        "definition" | "def" => client.definition(&location)?,
        "type" | "type-definition" | "type-def" => client.type_definition(&location)?,
        "implementation" | "impl" => client.implementation(&location)?,
        "incoming" | "incoming-calls" => client.incoming_calls(&location)?,
        "outgoing" | "outgoing-calls" => client.outgoing_calls(&location)?,
        other => {
            return Err(AppError::General(format!(
                "Unsupported semantic graph relation: {other}"
            )));
        }
    };
    let options = fcs::graph::GraphOptions {
        limit: usize::MAX,
        depth: input.depth,
        fanout: input.fanout,
        exclude: input.exclude.to_vec(),
    };
    let edges = fcs::graph::apply_options(&fcs::graph::lsp_edges(&location, input.relation, &items), &options);
    let format = fcs::graph::GraphFormat::parse(input.format)?;
    print!("{}", fcs::graph::format_edges(&edges, format)?);
    Ok(())
}

fn handle_graph_imports(
    directory: Option<&String>,
    limit: usize,
    format: &str,
    depth: usize,
    fanout: usize,
    exclude: &[String],
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    handle_graph_dependencies(GraphDependencyInput {
        directory,
        limit,
        format,
        depth,
        fanout,
        exclude,
        config,
        graph_kind: "imports",
    })
}

fn handle_graph_modules(
    directory: Option<&String>,
    limit: usize,
    format: &str,
    depth: usize,
    fanout: usize,
    exclude: &[String],
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    handle_graph_dependencies(GraphDependencyInput {
        directory,
        limit,
        format,
        depth,
        fanout,
        exclude,
        config,
        graph_kind: "modules",
    })
}

fn handle_graph_calls(
    directory: Option<&String>,
    limit: usize,
    format: &str,
    depth: usize,
    fanout: usize,
    exclude: &[String],
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    handle_graph_dependencies(GraphDependencyInput {
        directory,
        limit,
        format,
        depth,
        fanout,
        exclude,
        config,
        graph_kind: "calls",
    })
}

struct GraphDependencyInput<'a> {
    directory: Option<&'a String>,
    limit: usize,
    format: &'a str,
    depth: usize,
    fanout: usize,
    exclude: &'a [String],
    config: &'a fcs::config::Config,
    graph_kind: &'a str,
}

fn handle_graph_dependencies(input: GraphDependencyInput<'_>) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(input.directory)?;
    let root_arg = root.to_string_lossy().to_string();
    let ignore_path = resolve_ignore_file(Some(&root_arg));
    let files = fcs::files::find_files(Some(&root_arg), &[], &input.config.search.ignore, &ignore_path)?;
    let options = fcs::graph::GraphOptions {
        limit: input.limit,
        depth: input.depth,
        fanout: input.fanout,
        exclude: input.exclude.to_vec(),
    };
    let edges = match input.graph_kind {
        "imports" => fcs::graph::import_edges(&root, &files, &options)?,
        "modules" => fcs::graph::module_edges(&root, &files, &options)?,
        "calls" => fcs::graph::call_edges(&root, &files, &options)?,
        other => return Err(AppError::General(format!("Unsupported graph kind: {other}"))),
    };
    let format = fcs::graph::GraphFormat::parse(input.format)?;
    print!("{}", fcs::graph::format_edges(&edges, format)?);
    Ok(())
}

fn handle_trace_add(
    target: &str,
    label: Option<&String>,
    kind: &str,
    session: Option<String>,
    parent: Option<String>,
    branch: Option<String>,
    tags: Vec<String>,
) -> Result<(), AppError> {
    let location = parse_location_arg(target).unwrap_or_else(|_| fcs::trace::location_from_path(target, Some(1), None));
    let label = label
        .cloned()
        .unwrap_or_else(|| format!("{}:{}", location.path.display(), location.line.unwrap_or(1)));
    let metadata = fcs::trace::TraceMetadata {
        session,
        parent,
        branch,
        tags,
        note: None,
        status: None,
        priority: None,
    };
    fcs::trace::record_location_with_metadata(&location, &label, kind, metadata)?;
    println!("Added trace entry: {label}");
    Ok(())
}

struct TraceListFilter<'a> {
    session: Option<&'a String>,
    tag: Option<&'a String>,
    kind: Option<&'a String>,
    status: Option<&'a String>,
    priority: Option<&'a String>,
}

fn handle_trace_list(filter: TraceListFilter<'_>) -> Result<(), AppError> {
    let entries = fcs::trace::list()?
        .into_iter()
        .filter(|entry| trace_entry_matches(entry, &filter))
        .collect::<Vec<fcs::trace::TraceEntry>>();
    if entries.is_empty() {
        println!("Trace history is empty");
        return Ok(());
    }

    for entry in entries {
        println!("{}", fcs::trace::format_entry(&entry));
    }
    Ok(())
}

fn trace_entry_matches(entry: &fcs::trace::TraceEntry, filter: &TraceListFilter<'_>) -> bool {
    if let Some(session) = filter.session {
        let entry_session = entry.session.as_deref().unwrap_or("default");
        if entry_session != session.as_str() {
            return false;
        }
    }
    if let Some(tag) = filter.tag {
        if !entry.tags.iter().any(|entry_tag| entry_tag == tag) {
            return false;
        }
    }
    if let Some(kind) = filter.kind {
        if entry.kind != *kind {
            return false;
        }
    }
    if let Some(status) = filter.status {
        if entry.status.as_deref() != Some(status.as_str()) {
            return false;
        }
    }
    if let Some(priority) = filter.priority {
        if entry.priority.as_deref() != Some(priority.as_str()) {
            return false;
        }
    }
    true
}

fn handle_trace_entry_change(selector: &str, change: fcs::trace::TraceEntryChange, field: &str) {
    match change {
        fcs::trace::TraceEntryChange::Changed => println!("Updated trace entry {selector}: {field}"),
        fcs::trace::TraceEntryChange::NotFound => println!("Trace entry not found: {selector}"),
    }
}

fn handle_trace_open(config: &fcs::config::Config) -> Result<(), AppError> {
    let entries = fcs::trace::list()?;
    if entries.is_empty() {
        println!("Trace history is empty");
        return Ok(());
    }

    let items = fcs::trace::entries_to_items(&entries);
    run_code_item_picker(&items, None, config)
}

fn handle_trace_sessions(include_archived: bool) -> Result<(), AppError> {
    let sessions = fcs::trace::list_sessions(include_archived)?;
    if sessions.is_empty() {
        println!("No trace sessions");
        return Ok(());
    }

    for session in sessions {
        println!("{}", fcs::trace::format_session(&session));
    }
    Ok(())
}

fn handle_trace_archive(session: &str) -> Result<(), AppError> {
    match fcs::trace::archive_session(session)? {
        fcs::trace::TraceSessionChange::Changed => println!("Archived trace session: {session}"),
        fcs::trace::TraceSessionChange::Unchanged => println!("Trace session already archived: {session}"),
        fcs::trace::TraceSessionChange::NotFound => println!("Trace session not found: {session}"),
    }
    Ok(())
}

fn handle_trace_unarchive(session: &str) -> Result<(), AppError> {
    match fcs::trace::unarchive_session(session)? {
        fcs::trace::TraceSessionChange::Changed => println!("Unarchived trace session: {session}"),
        fcs::trace::TraceSessionChange::Unchanged => println!("Trace session already active: {session}"),
        fcs::trace::TraceSessionChange::NotFound => println!("Trace session not found: {session}"),
    }
    Ok(())
}

fn handle_trace_report(session: &str, directory: Option<&String>, format: &str) -> Result<(), AppError> {
    let root = match directory {
        Some(directory) => Some(fcs::workspace::resolve_root(Some(directory))?),
        None => None,
    };
    match format {
        "markdown" | "md" => print!("{}", fcs::trace::export_session_markdown(session, root.as_deref())?),
        "json" => print!("{}", fcs::trace::export_session_json(session, root.as_deref())?),
        other => {
            return Err(AppError::General(format!(
                "Unsupported trace session report format: {other}"
            )));
        }
    }
    Ok(())
}

fn handle_trace_timeline(session: &str, directory: Option<&String>, format: &str) -> Result<(), AppError> {
    let root = match directory {
        Some(directory) => Some(fcs::workspace::resolve_root(Some(directory))?),
        None => None,
    };
    match format {
        "markdown" | "md" => print!(
            "{}",
            fcs::trace::export_session_timeline_markdown(session, root.as_deref())?
        ),
        "json" => print!(
            "{}",
            fcs::trace::export_session_timeline_json(session, root.as_deref())?
        ),
        other => {
            return Err(AppError::General(format!("Unsupported trace timeline format: {other}")));
        }
    }
    Ok(())
}

fn handle_trace_replay(session: &str, directory: Option<&String>, format: &str) -> Result<(), AppError> {
    let root = match directory {
        Some(directory) => Some(fcs::workspace::resolve_root(Some(directory))?),
        None => None,
    };
    match format {
        "markdown" | "md" => print!(
            "{}",
            fcs::trace::export_session_replay_markdown(session, root.as_deref())?
        ),
        "json" => print!("{}", fcs::trace::export_session_replay_json(session, root.as_deref())?),
        other => return Err(AppError::General(format!("Unsupported trace replay format: {other}"))),
    }
    Ok(())
}

fn handle_trace_replay_plan(
    session: &str,
    directory: Option<&String>,
    program: Option<&String>,
    name: Option<&String>,
    format: &str,
) -> Result<(), AppError> {
    let root = match directory {
        Some(directory) => Some(fcs::workspace::resolve_root(Some(directory))?),
        None => None,
    };
    match format {
        "markdown" | "md" => print!(
            "{}",
            fcs::trace::export_session_replay_plan_markdown(
                session,
                root.as_deref(),
                program.map(String::as_str),
                name.map(String::as_str),
            )?
        ),
        "json" => print!(
            "{}",
            fcs::trace::export_session_replay_plan_json(
                session,
                root.as_deref(),
                program.map(String::as_str),
                name.map(String::as_str),
            )?
        ),
        other => {
            return Err(AppError::General(format!(
                "Unsupported trace replay-plan format: {other}"
            )))
        }
    }
    Ok(())
}

fn handle_trace_structured(session: &str, directory: Option<&String>, format: &str) -> Result<(), AppError> {
    let root = match directory {
        Some(directory) => Some(fcs::workspace::resolve_root(Some(directory))?),
        None => None,
    };
    match format {
        "markdown" | "md" => print!(
            "{}",
            fcs::trace::export_session_structured_markdown(session, root.as_deref())?
        ),
        "json" => print!(
            "{}",
            fcs::trace::export_session_structured_json(session, root.as_deref())?
        ),
        other => {
            return Err(AppError::General(format!(
                "Unsupported trace structured report format: {other}"
            )));
        }
    }
    Ok(())
}

fn handle_trace_insights(session: &str, directory: Option<&String>, format: &str) -> Result<(), AppError> {
    let root = match directory {
        Some(directory) => Some(fcs::workspace::resolve_root(Some(directory))?),
        None => None,
    };
    match format {
        "markdown" | "md" => print!(
            "{}",
            fcs::trace::export_session_insights_markdown(session, root.as_deref())?
        ),
        "json" => print!(
            "{}",
            fcs::trace::export_session_insights_json(session, root.as_deref())?
        ),
        other => {
            return Err(AppError::General(format!(
                "Unsupported trace insights report format: {other}"
            )));
        }
    }
    Ok(())
}

fn handle_trace_diff(
    left_session: &str,
    right_session: &str,
    directory: Option<&String>,
    format: &str,
) -> Result<(), AppError> {
    let root = match directory {
        Some(directory) => Some(fcs::workspace::resolve_root(Some(directory))?),
        None => None,
    };
    match format {
        "markdown" | "md" => print!(
            "{}",
            fcs::trace::export_session_diff_markdown(left_session, right_session, root.as_deref())?
        ),
        "json" => print!(
            "{}",
            fcs::trace::export_session_diff_json(left_session, right_session, root.as_deref())?
        ),
        other => {
            return Err(AppError::General(format!("Unsupported trace diff format: {other}")));
        }
    }
    Ok(())
}

fn handle_history_list() -> Result<(), AppError> {
    let entries = fcs::history::list()?;
    if entries.is_empty() {
        println!("Query history is empty");
        return Ok(());
    }

    for entry in entries {
        println!("{}", fcs::history::format_entry(&entry));
    }
    Ok(())
}

fn handle_man(stdout: bool, out_dir: Option<&String>) -> Result<(), AppError> {
    let contents = man_page_contents();
    if stdout || out_dir.is_none() {
        print!("{contents}");
        return Ok(());
    }

    let out_dir = PathBuf::from(out_dir.expect("checked is_some"));
    fs::create_dir_all(&out_dir)?;
    let path = out_dir.join("fcs.1");
    fs::write(&path, contents)?;
    println!("Wrote man page: {}", path.display());
    Ok(())
}

fn man_page_contents() -> String {
    let help = Cli::command().render_long_help().to_string();
    format!(
        ".TH FCS 1 \"\" \"fcs {}\" \"User Commands\"\n.SH NAME\nfcs \\- fuzzy code search and tracing workbench\n.SH SYNOPSIS\n.B fcs\n<COMMAND> [OPTIONS]\n.SH DESCRIPTION\n.nf\n{}\n.fi\n",
        env!("CARGO_PKG_VERSION"),
        roff_escape(&help)
    )
}

fn roff_escape(value: &str) -> String {
    value
        .lines()
        .map(|line| {
            let escaped = line.replace('\\', "\\\\");
            if escaped.starts_with('.') || escaped.starts_with('\'') {
                format!("\\&{escaped}")
            } else {
                escaped
            }
        })
        .collect::<Vec<String>>()
        .join("\n")
}

fn handle_debug_command(
    binary: &str,
    debugger: &str,
    breakpoints: &[String],
    args: &[String],
    cwd: Option<&String>,
    env: &[String],
    run: bool,
) -> Result<(), AppError> {
    let debugger = fcs::debugger::DebuggerKind::parse(debugger)?;
    let locations = breakpoints
        .iter()
        .map(|breakpoint| parse_location_arg(breakpoint))
        .collect::<Result<Vec<Location>, AppError>>()?;
    let session = fcs::debugger::DebugSession {
        debugger,
        binary: PathBuf::from(binary),
        cwd: cwd.map(PathBuf::from),
        env: parse_debug_env(env)?,
        breakpoints: locations,
        args: args.to_vec(),
    };

    run_or_print_debug_session(&session, run)
}

fn handle_debug_last(
    binary: &str,
    debugger: &str,
    args: &[String],
    cwd: Option<&String>,
    env: &[String],
    run: bool,
) -> Result<(), AppError> {
    let debugger = fcs::debugger::DebuggerKind::parse(debugger)?;
    let entries = fcs::trace::list()?;
    let entry = entries
        .first()
        .ok_or_else(|| AppError::General("Trace history is empty".to_string()))?;
    let session = fcs::debugger::DebugSession {
        debugger,
        binary: PathBuf::from(binary),
        cwd: cwd.map(PathBuf::from),
        env: parse_debug_env(env)?,
        breakpoints: vec![Location::new(&entry.path, entry.line, entry.column)],
        args: args.to_vec(),
    };

    run_or_print_debug_session(&session, run)
}

struct DebugSaveProfileInput<'a> {
    name: &'a str,
    binary: &'a str,
    debugger: &'a str,
    breakpoints: &'a [String],
    directory: Option<&'a String>,
    args: &'a [String],
    cwd: Option<&'a String>,
    env: &'a [String],
}

struct DebugFromTraceInput<'a> {
    session: &'a str,
    binary: &'a str,
    name: Option<&'a String>,
    debugger: &'a str,
    directory: Option<&'a String>,
    args: &'a [String],
    cwd: Option<&'a String>,
    env: &'a [String],
    run: bool,
}

fn handle_debug_save_profile(input: DebugSaveProfileInput<'_>) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(input.directory)?;
    let debugger = fcs::debugger::DebuggerKind::parse(input.debugger)?;
    let breakpoints = input
        .breakpoints
        .iter()
        .map(|breakpoint| parse_location_arg(breakpoint))
        .collect::<Result<Vec<Location>, AppError>>()?;
    let session = fcs::debugger::DebugSession {
        debugger,
        binary: PathBuf::from(input.binary),
        cwd: input.cwd.map(PathBuf::from),
        env: parse_debug_env(input.env)?,
        breakpoints,
        args: input.args.to_vec(),
    };
    fcs::debugger::save_profile(&root, session.to_profile(input.name))?;
    println!("Saved debug profile: {}", input.name);
    Ok(())
}

fn handle_debug_from_trace(input: DebugFromTraceInput<'_>) -> Result<(), AppError> {
    let (root, breakpoints) = trace_session_breakpoints(input.session, input.directory)?;
    let debugger = fcs::debugger::DebuggerKind::parse(input.debugger)?;
    let session = fcs::debugger::DebugSession {
        debugger,
        binary: PathBuf::from(input.binary),
        cwd: input.cwd.map(PathBuf::from),
        env: parse_debug_env(input.env)?,
        breakpoints,
        args: input.args.to_vec(),
    };
    let profile_name = input
        .name
        .cloned()
        .unwrap_or_else(|| format!("{}-debug", input.session));
    fcs::debugger::save_profile(&root, session.to_profile(&profile_name))?;
    println!(
        "Saved debug profile from trace session '{}': {} ({} breakpoint(s))",
        input.session,
        profile_name,
        session.breakpoints.len()
    );
    if input.run {
        session.run()?;
    } else {
        println!("{}", session.command_preview());
    }
    Ok(())
}

fn handle_debug_profiles(directory: Option<&String>) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let profiles = fcs::debugger::list_profiles(&root)?;
    if profiles.is_empty() {
        println!("No debug profiles");
        return Ok(());
    }

    for profile in profiles {
        let session = fcs::debugger::DebugSession::from_profile(&profile);
        let disabled = profile
            .breakpoints
            .iter()
            .filter(|breakpoint| !breakpoint.enabled)
            .count();
        if disabled == 0 {
            println!("{}: {}", profile.name, session.command_preview());
        } else {
            println!(
                "{}: {} ({} disabled breakpoint(s))",
                profile.name,
                session.command_preview(),
                disabled
            );
        }
    }
    Ok(())
}

fn handle_debug_delete_profile(name: &str, directory: Option<&String>) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    if fcs::debugger::delete_profile(&root, name)? {
        println!("Deleted debug profile: {name}");
    } else {
        println!("Debug profile not found: {name}");
    }
    Ok(())
}

fn handle_debug_set_breakpoint_enabled(
    name: &str,
    index: usize,
    directory: Option<&String>,
    enabled: bool,
) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    fcs::debugger::set_breakpoint_enabled(&root, name, index, enabled)?;
    let state = if enabled { "Enabled" } else { "Disabled" };
    println!("{state} breakpoint {index} in debug profile: {name}");
    Ok(())
}

fn handle_debug_run_profile(name: &str, directory: Option<&String>, run: bool) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let profile = fcs::debugger::load_profile(&root, name)?;
    let session = fcs::debugger::DebugSession::from_profile(&profile);
    run_or_print_debug_session(&session, run)
}

struct DapProfileInput<'a> {
    name: Option<&'a String>,
    program: &'a str,
    adapter: &'a str,
    breakpoints: &'a [String],
    cwd: Option<&'a String>,
    env: &'a [String],
    stop_on_entry: bool,
    args: &'a [String],
}

struct DapFromTraceInput<'a> {
    session: &'a str,
    program: &'a str,
    name: Option<&'a String>,
    adapter: &'a str,
    directory: Option<&'a String>,
    cwd: Option<&'a String>,
    env: &'a [String],
    stop_on_entry: bool,
    args: &'a [String],
}

fn handle_dap_launch(input: DapProfileInput<'_>, bundle: bool) -> Result<(), AppError> {
    let profile = build_dap_profile(input)?;
    print_dap_request(&profile, bundle)
}

fn handle_dap_save_profile(input: DapProfileInput<'_>, directory: Option<&String>) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let profile = build_dap_profile(input)?;
    let profile_name = profile.name.clone();
    fcs::dap::save_profile(&root, profile)?;
    println!("Saved DAP profile: {profile_name}");
    Ok(())
}

fn handle_dap_from_trace(input: DapFromTraceInput<'_>) -> Result<(), AppError> {
    let (root, locations) = trace_session_breakpoints(input.session, input.directory)?;
    let env = input
        .env
        .iter()
        .map(|value| fcs::dap::parse_env_var(value))
        .collect::<Result<Vec<fcs::dap::DapEnvVar>, AppError>>()?;
    let profile_name = input.name.cloned().unwrap_or_else(|| format!("{}-dap", input.session));
    let profile = fcs::dap::DapLaunchProfile {
        name: profile_name.clone(),
        adapter: input.adapter.to_string(),
        program: PathBuf::from(input.program),
        cwd: input.cwd.map(PathBuf::from),
        args: input.args.to_vec(),
        env,
        breakpoints: locations.iter().map(fcs::dap::DapBreakpoint::from_location).collect(),
        stop_on_entry: input.stop_on_entry,
    };
    let breakpoint_count = profile.breakpoints.len();
    fcs::dap::save_profile(&root, profile)?;
    println!(
        "Saved DAP profile from trace session '{}': {} ({} breakpoint(s))",
        input.session, profile_name, breakpoint_count
    );
    Ok(())
}

fn handle_dap_profiles(directory: Option<&String>) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let profiles = fcs::dap::list_profiles(&root)?;
    if profiles.is_empty() {
        println!("No DAP profiles");
        return Ok(());
    }

    for profile in profiles {
        let enabled_breakpoints = profile
            .breakpoints
            .iter()
            .filter(|breakpoint| breakpoint.enabled)
            .count();
        println!(
            "{} [{}] program={} args={} breakpoints={}",
            profile.name,
            profile.adapter,
            profile.program.display(),
            profile.args.len(),
            enabled_breakpoints
        );
    }
    Ok(())
}

fn handle_dap_adapters(format: &str) -> Result<(), AppError> {
    let adapters = fcs::dap::discover_adapters();
    match format {
        "text" => {
            if adapters.is_empty() {
                println!("No DAP adapter candidates");
                return Ok(());
            }
            for adapter in adapters {
                let state = if adapter.available { "available" } else { "missing" };
                println!(
                    "{}\t{}\t{}\t{}",
                    adapter.adapter,
                    state,
                    adapter.command_line(),
                    adapter.detail
                );
            }
            Ok(())
        }
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&adapters).map_err(|err| AppError::General(err.to_string()))?
            );
            Ok(())
        }
        other => Err(AppError::General(format!("Unsupported DAP adapter format: {other}"))),
    }
}

fn handle_dap_request_profile(name: &str, directory: Option<&String>, bundle: bool) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let profile = fcs::dap::load_profile(&root, name)?;
    print_dap_request(&profile, bundle)
}

fn handle_dap_session_smoke(input: DapProfileInput<'_>) -> Result<(), AppError> {
    let profile = build_dap_profile(input)?;
    let report = fcs::dap::run_mock_session_smoke(&profile)?;

    println!("DAP mock session smoke passed");
    println!("requests: {}", report.request_count);
    println!("responses: {}", report.response_count);
    println!("commands: {}", report.commands.join(", "));
    if report.events.is_empty() {
        println!("events: none");
    } else {
        println!("events: {}", report.events.join(", "));
    }
    Ok(())
}

fn handle_dap_adapter_session(
    adapter_command: &str,
    adapter_env: &[String],
    input: DapProfileInput<'_>,
) -> Result<(), AppError> {
    let profile = build_dap_profile(input)?;
    let adapter_env = adapter_env
        .iter()
        .map(|value| fcs::dap::parse_env_var(value))
        .collect::<Result<Vec<fcs::dap::DapEnvVar>, AppError>>()?;
    let mut spec = if adapter_command == "auto" {
        fcs::dap::best_adapter_for_profile(&profile)
            .ok_or_else(|| AppError::General("No available DAP adapter was discovered".to_string()))?
            .spec(profile.cwd.clone())
    } else {
        fcs::dap::DapAdapterProcessSpec {
            command: PathBuf::from(adapter_command),
            args: Vec::new(),
            cwd: profile.cwd.clone(),
            env: Vec::new(),
        }
    };
    spec.env = adapter_env;
    let transport = fcs::dap::DapProcessTransport::spawn(&spec)?;
    let mut client = fcs::dap::DapClient::new(transport);
    let report = fcs::dap::run_launch_session(&mut client, &profile)?;

    println!("DAP adapter session completed");
    println!("adapter_command: {}", spec.command.display());
    println!("requests: {}", report.request_count);
    println!("responses: {}", report.response_count);
    println!("breakpoint_responses: {}", report.breakpoint_response_count);
    if report.breakpoint_results.is_empty() {
        println!("breakpoints: none");
    } else {
        for (index, breakpoint) in report.breakpoint_results.iter().enumerate() {
            let state = if breakpoint.verified { "verified" } else { "unverified" };
            let line = breakpoint
                .line
                .map(|line| line.to_string())
                .unwrap_or_else(|| "-".to_string());
            let message = breakpoint.message.as_deref().unwrap_or("-");
            println!("breakpoint {}: {} line={} {}", index + 1, state, line, message);
        }
    }
    println!("initialized: {}", report.initialized);
    println!("launch_completed: {}", report.launch_completed);
    println!("commands: {}", report.commands.join(", "));
    if report.events.is_empty() {
        println!("events: none");
    } else {
        println!("events: {}", report.events.join(", "));
    }
    Ok(())
}

fn build_dap_profile(input: DapProfileInput<'_>) -> Result<fcs::dap::DapLaunchProfile, AppError> {
    let program_path = PathBuf::from(input.program);
    let profile_name = input.name.cloned().unwrap_or_else(|| {
        program_path
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .unwrap_or("launch")
            .to_string()
    });
    let breakpoints = input
        .breakpoints
        .iter()
        .map(|breakpoint| {
            parse_location_arg(breakpoint).map(|location| fcs::dap::DapBreakpoint::from_location(&location))
        })
        .collect::<Result<Vec<fcs::dap::DapBreakpoint>, AppError>>()?;
    let env = input
        .env
        .iter()
        .map(|value| fcs::dap::parse_env_var(value))
        .collect::<Result<Vec<fcs::dap::DapEnvVar>, AppError>>()?;

    Ok(fcs::dap::DapLaunchProfile {
        name: profile_name,
        adapter: input.adapter.to_string(),
        program: program_path,
        cwd: input.cwd.map(PathBuf::from),
        args: input.args.to_vec(),
        env,
        breakpoints,
        stop_on_entry: input.stop_on_entry,
    })
}

fn print_dap_request(profile: &fcs::dap::DapLaunchProfile, bundle: bool) -> Result<(), AppError> {
    if bundle {
        print!("{}", fcs::dap::request_bundle_json(profile)?);
    } else {
        print!("{}", fcs::dap::launch_request_json(profile)?);
    }
    Ok(())
}

fn trace_session_breakpoints(session: &str, directory: Option<&String>) -> Result<(PathBuf, Vec<Location>), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let entries = fcs::trace::session_entries(session, Some(&root))?;
    let mut seen = BTreeSet::new();
    let mut locations = Vec::new();

    for entry in entries {
        if entry.line.is_none() {
            continue;
        }

        let path = if entry.path.is_absolute() {
            entry.path
        } else {
            root.join(entry.path)
        };
        let key = (path.clone(), entry.line, entry.column);
        if seen.insert(key) {
            locations.push(Location::new(path, entry.line, entry.column));
        }
    }

    if locations.is_empty() {
        return Err(AppError::General(format!(
            "Trace session has no line locations in workspace: {session}"
        )));
    }

    Ok((root, locations))
}

fn parse_debug_env(values: &[String]) -> Result<Vec<fcs::debugger::DebugEnvVar>, AppError> {
    values
        .iter()
        .map(|value| fcs::debugger::parse_env_var(value))
        .collect::<Result<Vec<fcs::debugger::DebugEnvVar>, AppError>>()
}

fn run_or_print_debug_session(session: &fcs::debugger::DebugSession, run: bool) -> Result<(), AppError> {
    if run {
        session.run()
    } else {
        println!("{}", session.command_preview());
        Ok(())
    }
}

fn print_workspace_status(status: &fcs::workspace::WorkspaceStatus) {
    println!("Root: {}", status.root.display());
    println!("Cache: {}", status.cache_dir.display());
    println!("clangd: {}", status.clangd_version.as_deref().unwrap_or("not found"));
    println!(
        "rust-analyzer: {}",
        status.rust_analyzer_version.as_deref().unwrap_or("not found")
    );
    println!("compile_commands.json: {}", status.has_compile_commands);
    println!("compile_flags.txt: {}", status.has_compile_flags);
    println!("Cargo.toml: {}", status.has_cargo_toml);
    println!("semantic_ready: {}", status.is_semantic_ready());
}

fn print_workspace_advice(report: &fcs::workspace::WorkspaceAdviceReport) {
    println!("Root: {}", report.root.display());
    println!("Project type: {}", report.project_type);
    println!("Config: {}", report.config_path.display());
    println!("Build systems: {}", display_list(&report.build_systems));
    println!("Languages: {}", display_list(&report.languages));
    if report.debug_targets.is_empty() {
        println!("Debug targets: none");
    } else {
        println!("Debug targets:");
        for target in &report.debug_targets {
            println!("  {}", target.display());
        }
    }
    println!("Health checks:");
    if report.cache_checks.is_empty() {
        println!("  none");
    } else {
        for check in &report.cache_checks {
            let state = if check.ok { "ok" } else { "warn" };
            println!("  [{state}] {}: {}", check.name, check.detail);
        }
    }
    println!("Advice:");
    if report.advice.is_empty() {
        println!("  none");
        return;
    }

    for advice in &report.advice {
        println!("  [{}] {}", advice.level.as_str(), advice.message);
        if let Some(action) = &advice.action {
            println!("    action: {action}");
        }
    }
}

fn display_list(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
    }
}

fn display_paths(values: &[PathBuf]) -> String {
    if values.is_empty() {
        return "none".to_string();
    }
    values
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<String>>()
        .join(", ")
}

fn format_index_counts(values: &[fcs::index::IndexCount]) -> String {
    if values.is_empty() {
        return "none".to_string();
    }

    values
        .iter()
        .map(|value| format!("{}={}", value.name, value.count))
        .collect::<Vec<String>>()
        .join(", ")
}

fn format_index_schema_status(status: fcs::index::IndexSchemaStatus) -> &'static str {
    match status {
        fcs::index::IndexSchemaStatus::Missing => "missing",
        fcs::index::IndexSchemaStatus::Current => "current",
        fcs::index::IndexSchemaStatus::Migrated => "migrated",
        fcs::index::IndexSchemaStatus::Future => "future",
        fcs::index::IndexSchemaStatus::Corrupt => "corrupt",
    }
}

fn format_lsp_provider_status(status: fcs::lsp::LspProviderHealthStatus) -> &'static str {
    match status {
        fcs::lsp::LspProviderHealthStatus::Available => "available",
        fcs::lsp::LspProviderHealthStatus::Unavailable => "unavailable",
    }
}

fn write_latency_smoke(root: &Path, rows: &[(&str, u128)]) -> Result<(), AppError> {
    let cache_dir = fcs::workspace::cache_dir_for_root(root)?;
    fs::create_dir_all(&cache_dir)?;
    let path = cache_dir.join("latency-smoke.tsv");
    let mut contents = String::from("command\tlatency_ms\n");
    for (name, elapsed_ms) in rows {
        contents.push_str(&format!("{name}\t{elapsed_ms}\n"));
    }
    fs::write(path, contents)?;
    Ok(())
}

fn handle_project_actions_list(directory: Option<&String>, config: &fcs::config::Config) -> Result<(), AppError> {
    let actions = fcs::project_actions::list_actions(config, directory)?;
    if actions.is_empty() {
        println!("No project actions configured");
        return Ok(());
    }

    for action in actions {
        println!("{}", fcs::project_actions::format_action(&action));
    }
    Ok(())
}

struct ProjectActionRunInput<'a> {
    name: &'a str,
    directory: Option<&'a String>,
    file: Option<&'a String>,
    line: Option<usize>,
    symbol: Option<&'a String>,
    dry_run: bool,
    args: &'a [String],
    config: &'a fcs::config::Config,
}

fn handle_project_actions_run(input: ProjectActionRunInput<'_>) -> Result<(), AppError> {
    let action = fcs::project_actions::expand_action(
        input.config,
        input.directory,
        input.name,
        input.file,
        input.line,
        input.symbol,
        input.args,
    )?;
    println!("{}", fcs::project_actions::format_command_line(&action));
    if input.dry_run {
        return Ok(());
    }

    fcs::project_actions::run_expanded_action(&action)?;
    Ok(())
}

fn handle_project_action_templates() {
    for template in fcs::project_actions::builtin_templates() {
        println!("{}", fcs::project_actions::format_template(&template));
    }
}

fn handle_project_actions_init(
    template: &str,
    directory: Option<&String>,
    force: bool,
    dry_run: bool,
) -> Result<(), AppError> {
    if dry_run {
        print!("{}", fcs::project_actions::template_config_toml(directory, template)?);
        return Ok(());
    }

    let path = fcs::project_actions::write_template_config(directory, template, force)?;
    println!("Wrote project action template: {}", path.display());
    Ok(())
}

fn handle_project_actions_doctor(directory: Option<&String>, config: &fcs::config::Config) -> Result<(), AppError> {
    let diagnostics = fcs::project_actions::doctor_actions(config, directory)?;
    for diagnostic in diagnostics {
        let state = if diagnostic.ok { "ok" } else { "warn" };
        println!("[{state}] {}: {}", diagnostic.name, diagnostic.detail);
    }
    Ok(())
}

fn handle_plugin_list(directory: Option<&String>) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let manifests = fcs::plugins::discover(Some(&root))?;
    if manifests.is_empty() {
        println!("No plugins discovered");
        return Ok(());
    }

    for manifest in manifests {
        println!("{}", fcs::plugins::format_manifest(&manifest));
    }
    Ok(())
}

fn handle_plugin_show(name: &str, directory: Option<&String>) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let manifest = fcs::plugins::find_manifest(Some(&root), name)?;
    println!("{}", fcs::plugins::format_manifest(&manifest));
    if !manifest.commands.is_empty() {
        println!("Commands:");
        for command in &manifest.commands {
            println!("  {}", fcs::plugins::format_command(&manifest.plugin.name, command));
        }
    }
    if !manifest.templates.is_empty() {
        println!("Templates:");
        for template in &manifest.templates {
            println!("  {}", fcs::plugins::format_template(&manifest.plugin.name, template));
        }
    }
    Ok(())
}

fn handle_plugin_doctor(directory: Option<&String>, strict: bool) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let diagnostics = fcs::plugins::doctor(Some(&root))?;
    let mut has_warning = false;
    for diagnostic in diagnostics {
        let state = if diagnostic.ok { "ok" } else { "warn" };
        has_warning |= !diagnostic.ok;
        println!("[{state}] {}: {}", diagnostic.name, diagnostic.detail);
    }
    if strict && has_warning {
        return Err(AppError::General("Plugin doctor found warning(s)".to_string()));
    }
    Ok(())
}

fn handle_plugin_schema(format: &str) -> Result<(), AppError> {
    print!("{}", fcs::plugins::manifest_schema(format)?);
    Ok(())
}

fn handle_plugin_templates(directory: Option<&String>) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    for manifest in fcs::plugins::discover(Some(&root))? {
        for template in &manifest.templates {
            println!("{}", fcs::plugins::format_template(&manifest.plugin.name, template));
        }
    }
    Ok(())
}

fn handle_plugin_commands(directory: Option<&String>) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    for manifest in fcs::plugins::discover(Some(&root))? {
        for command in &manifest.commands {
            println!("{}", fcs::plugins::format_command(&manifest.plugin.name, command));
        }
    }
    Ok(())
}

fn handle_plugin_init(template: &str, directory: Option<&String>, force: bool, dry_run: bool) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(directory)?;
    let report = fcs::plugins::init_template(&root, template, force, dry_run)?;
    if dry_run {
        print!("{}", report.contents);
    } else {
        println!(
            "Initialized project config from plugin template {}: {} ({} action(s))",
            report.template,
            report.path.display(),
            report.action_count
        );
    }
    Ok(())
}

struct PluginRunInput<'a> {
    name: &'a str,
    directory: Option<&'a String>,
    file: Option<&'a String>,
    line: Option<usize>,
    symbol: Option<&'a String>,
    dry_run: bool,
    vars: &'a [String],
    args: &'a [String],
}

fn handle_plugin_run(input: PluginRunInput<'_>) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(input.directory)?;
    let command = fcs::plugins::expand_command_with_vars(
        &root,
        input.name,
        input.file,
        input.line,
        input.symbol,
        input.args,
        input.vars,
    )?;
    if input.dry_run {
        println!("{}", fcs::plugins::format_execution_plan(&command));
        return Ok(());
    }

    let code = fcs::plugins::run_expanded_command(&command)?;
    println!("Plugin command exited with status {code}");
    Ok(())
}

fn handle_plugin_plan(input: PluginRunInput<'_>) -> Result<(), AppError> {
    let root = fcs::workspace::resolve_root(input.directory)?;
    let command = fcs::plugins::expand_command_with_vars(
        &root,
        input.name,
        input.file,
        input.line,
        input.symbol,
        input.args,
        input.vars,
    )?;
    println!("{}", fcs::plugins::format_execution_plan(&command));
    Ok(())
}

pub(super) fn execute(command: Commands, config: fcs::config::Config) -> Result<(), AppError> {
    match command {
        Commands::Ignore { action, directory } => {
            let ignore_path = resolve_ignore_file(directory.as_ref());
            let ignore_file = IgnoreFile::new(ignore_path.clone());
            match action {
                IgnoreAction::Init => {
                    ignore_file.init(true)?;
                    println!("Initialized ignore file at: {}", ignore_path.display());
                }
                IgnoreAction::Add { patterns } => {
                    if patterns.is_empty() {
                        return Err(AppError::General("No patterns specified to add".to_string()));
                    }
                    ignore_file.add(&patterns)?;
                    println!("Added patterns to ignore file at: {}", ignore_path.display());
                }
                IgnoreAction::Remove { patterns } => {
                    if patterns.is_empty() {
                        return Err(AppError::General("No patterns specified to remove".to_string()));
                    }
                    ignore_file.remove(&patterns)?;
                    println!("Removed patterns from ignore file at: {}", ignore_path.display());
                }
                IgnoreAction::List => {
                    let patterns = ignore_file.list()?;
                    if patterns.is_empty() {
                        println!("No ignore patterns in: {}", ignore_path.display());
                    } else {
                        for p in &patterns {
                            println!("{p}");
                        }
                    }
                }
            }
        }
        Commands::Preview { target } => {
            let (path, line, height) = parse_preview_arg(&target)?;
            let result = make_result(&path, line, "");
            fcs::preview::preview(&result, height)?;
        }
        Commands::Tui {
            directory,
            mode,
            query,
            debug_binary,
        } => {
            fcs::tui::run(config, directory, mode, query, debug_binary)?;
        }
        Commands::Workspace { action } => match action {
            WorkspaceAction::Status { directory } => {
                handle_workspace_status(directory.as_ref(), &config)?;
            }
            WorkspaceAction::Init { directory } => {
                handle_workspace_init(directory.as_ref(), &config)?;
            }
            WorkspaceAction::Config { directory, force } => {
                let path = fcs::workspace::write_project_config(directory.as_ref(), force)?;
                println!("Wrote project config: {}", path.display());
            }
            WorkspaceAction::Profile { action } => {
                handle_workspace_profile(action)?;
            }
            WorkspaceAction::ConfigDoctor { directory, strict } => {
                handle_workspace_config_doctor(directory.as_ref(), strict)?;
            }
            WorkspaceAction::ConfigSchema { format } => {
                handle_workspace_config_schema(&format)?;
            }
            WorkspaceAction::Advise { directory } => {
                handle_workspace_advise(directory.as_ref(), &config)?;
            }
            WorkspaceAction::Plan { directory } => {
                handle_workspace_plan(directory.as_ref(), &config)?;
            }
            WorkspaceAction::Detect { directory } => {
                handle_workspace_detect(directory.as_ref())?;
            }
            WorkspaceAction::Doctor { directory } => {
                handle_workspace_advise(directory.as_ref(), &config)?;
            }
            WorkspaceAction::Workflows { directory, format } => {
                handle_workspace_workflows(directory.as_ref(), &format, &config)?;
            }
        },
        Commands::Service { action } => match action {
            ServiceAction::Start {
                directory,
                interval_ms,
                max_cycles,
                foreground,
                option,
            } => {
                handle_service_start(
                    directory.as_ref(),
                    interval_ms,
                    max_cycles,
                    foreground,
                    &option,
                    &config,
                )?;
            }
            ServiceAction::Status { directory } => {
                handle_service_status(directory.as_ref())?;
            }
            ServiceAction::Snapshot { directory, format } => {
                handle_service_snapshot(directory.as_ref(), &format, &config)?;
            }
            ServiceAction::Query {
                expression,
                directory,
                source,
                limit,
                format,
                explain,
                timing,
                warn_ms,
            } => {
                handle_query(
                    QueryRequest {
                        expression: &expression,
                        directory: directory.as_ref(),
                        source: &source,
                        limit,
                        format: &format,
                        explain,
                        timing,
                        warn_ms,
                    },
                    &config,
                )?;
            }
            ServiceAction::Stop { directory } => {
                handle_service_stop(directory.as_ref())?;
            }
        },
        Commands::Index { action } => match action {
            IndexAction::Status { directory } => {
                handle_index_status(directory.as_ref())?;
            }
            IndexAction::Stats { directory } => {
                handle_index_stats(directory.as_ref())?;
            }
            IndexAction::Build { directory, option } => {
                handle_index_build(directory.as_ref(), &option, &config)?;
            }
            IndexAction::List { directory, kind, limit } => {
                handle_index_list(directory.as_ref(), &kind, limit)?;
            }
            IndexAction::Query {
                query,
                directory,
                kind,
                limit,
                timing,
                warn_ms,
            } => {
                handle_index_query(directory.as_ref(), &kind, &query, limit, timing, warn_ms)?;
            }
            IndexAction::Compact { directory, dry_run } => {
                handle_index_compact(directory.as_ref(), dry_run)?;
            }
            IndexAction::Prewarm { directory } => {
                handle_index_prewarm(directory.as_ref())?;
            }
            IndexAction::Refresh { directory, option } => {
                handle_index_refresh(directory.as_ref(), &option, &config)?;
            }
            IndexAction::Daemon {
                directory,
                interval_ms,
                max_cycles,
                foreground,
                option,
            } => {
                handle_index_daemon(
                    directory.as_ref(),
                    interval_ms,
                    max_cycles,
                    foreground,
                    &option,
                    &config,
                )?;
            }
            IndexAction::DaemonStatus { directory } => {
                handle_index_daemon_status(directory.as_ref())?;
            }
            IndexAction::Doctor { directory } => {
                handle_index_doctor(directory.as_ref())?;
            }
            IndexAction::Repair {
                directory,
                option,
                force,
            } => {
                handle_index_repair(directory.as_ref(), &option, force, &config)?;
            }
            IndexAction::Bench {
                directory,
                build,
                limit,
                query,
                option,
            } => {
                handle_index_bench(directory.as_ref(), build, limit, &query, &option, &config)?;
            }
        },
        Commands::Query {
            expression,
            directory,
            source,
            limit,
            format,
            explain,
            timing,
            warn_ms,
        } => {
            handle_query(
                QueryRequest {
                    expression: &expression,
                    directory: directory.as_ref(),
                    source: &source,
                    limit,
                    format: &format,
                    explain,
                    timing,
                    warn_ms,
                },
                &config,
            )?;
        }
        Commands::Bench { action } => match action {
            BenchAction::All {
                directory,
                format,
                warn_ms,
                limit,
                query,
                option,
            } => {
                handle_bench_all(directory.as_ref(), &format, warn_ms, limit, &query, &option, &config)?;
            }
            BenchAction::Search {
                pattern,
                directory,
                format,
                warn_ms,
                option,
            } => {
                handle_bench_search(&pattern, directory.as_ref(), &format, warn_ms, &option, &config)?;
            }
            BenchAction::Index {
                directory,
                format,
                warn_ms,
                build,
                limit,
                query,
                option,
            } => {
                handle_bench_index(BenchIndexInput {
                    directory: directory.as_ref(),
                    format: &format,
                    warn_ms,
                    build,
                    limit,
                    query: &query,
                    options: &option,
                    config: &config,
                })?;
            }
            BenchAction::Trace { format, warn_ms } => {
                handle_bench_trace(&format, warn_ms)?;
            }
            BenchAction::Preview {
                target,
                format,
                warn_ms,
            } => {
                handle_bench_preview(&target, &format, warn_ms)?;
            }
        },
        Commands::Graph { action } => match action {
            GraphAction::Semantic {
                target,
                relation,
                format,
                depth,
                fanout,
                exclude,
                directory,
            } => {
                handle_graph_semantic(GraphSemanticInput {
                    target: &target,
                    relation: &relation,
                    format: &format,
                    depth,
                    fanout,
                    exclude: &exclude,
                    directory: directory.as_ref(),
                    config: &config,
                })?;
            }
            GraphAction::Imports {
                directory,
                limit,
                format,
                depth,
                fanout,
                exclude,
            } => {
                handle_graph_imports(directory.as_ref(), limit, &format, depth, fanout, &exclude, &config)?;
            }
            GraphAction::Modules {
                directory,
                limit,
                format,
                depth,
                fanout,
                exclude,
            } => {
                handle_graph_modules(directory.as_ref(), limit, &format, depth, fanout, &exclude, &config)?;
            }
            GraphAction::Calls {
                directory,
                limit,
                format,
                depth,
                fanout,
                exclude,
            } => {
                handle_graph_calls(directory.as_ref(), limit, &format, depth, fanout, &exclude, &config)?;
            }
        },
        Commands::Actions { action } => match action {
            ProjectAction::List { directory } => {
                handle_project_actions_list(directory.as_ref(), &config)?;
            }
            ProjectAction::Run {
                name,
                directory,
                file,
                line,
                symbol,
                dry_run,
                args,
            } => {
                handle_project_actions_run(ProjectActionRunInput {
                    name: &name,
                    directory: directory.as_ref(),
                    file: file.as_ref(),
                    line,
                    symbol: symbol.as_ref(),
                    dry_run,
                    args: &args,
                    config: &config,
                })?;
            }
            ProjectAction::Templates => {
                handle_project_action_templates();
            }
            ProjectAction::Init {
                template,
                directory,
                force,
                dry_run,
            } => {
                handle_project_actions_init(&template, directory.as_ref(), force, dry_run)?;
            }
            ProjectAction::Doctor { directory } => {
                handle_project_actions_doctor(directory.as_ref(), &config)?;
            }
        },
        Commands::Plugin { action } => match action {
            PluginAction::List { directory } => {
                handle_plugin_list(directory.as_ref())?;
            }
            PluginAction::Show { name, directory } => {
                handle_plugin_show(&name, directory.as_ref())?;
            }
            PluginAction::Doctor { directory, strict } => {
                handle_plugin_doctor(directory.as_ref(), strict)?;
            }
            PluginAction::Schema { format } => {
                handle_plugin_schema(&format)?;
            }
            PluginAction::Templates { directory } => {
                handle_plugin_templates(directory.as_ref())?;
            }
            PluginAction::Commands { directory } => {
                handle_plugin_commands(directory.as_ref())?;
            }
            PluginAction::Init {
                template,
                directory,
                force,
                dry_run,
            } => {
                handle_plugin_init(&template, directory.as_ref(), force, dry_run)?;
            }
            PluginAction::Run {
                name,
                directory,
                file,
                line,
                symbol,
                dry_run,
                vars,
                args,
            } => {
                handle_plugin_run(PluginRunInput {
                    name: &name,
                    directory: directory.as_ref(),
                    file: file.as_ref(),
                    line,
                    symbol: symbol.as_ref(),
                    dry_run,
                    vars: &vars,
                    args: &args,
                })?;
            }
            PluginAction::Plan {
                name,
                directory,
                file,
                line,
                symbol,
                vars,
                args,
            } => {
                handle_plugin_plan(PluginRunInput {
                    name: &name,
                    directory: directory.as_ref(),
                    file: file.as_ref(),
                    line,
                    symbol: symbol.as_ref(),
                    dry_run: true,
                    vars: &vars,
                    args: &args,
                })?;
            }
        },
        Commands::Def { target, directory } => {
            handle_definition(&target, directory.as_ref(), &config)?;
        }
        Commands::Refs { target, directory } => {
            handle_references(&target, directory.as_ref(), &config)?;
        }
        Commands::TypeDef { target, directory } => {
            handle_type_definition(&target, directory.as_ref(), &config)?;
        }
        Commands::Implementation { target, directory } => {
            handle_implementation(&target, directory.as_ref(), &config)?;
        }
        Commands::DocSymbols { target, directory } => {
            handle_document_symbols(&target, directory.as_ref(), &config)?;
        }
        Commands::Incoming { target, directory } => {
            handle_incoming_calls(&target, directory.as_ref(), &config)?;
        }
        Commands::Outgoing { target, directory } => {
            handle_outgoing_calls(&target, directory.as_ref(), &config)?;
        }
        Commands::Diag { target, directory } => {
            handle_diagnostics(&target, directory.as_ref(), &config)?;
        }
        Commands::Hover { target, directory } => {
            handle_hover(&target, directory.as_ref(), &config)?;
        }
        Commands::WorkspaceSymbols {
            query,
            directory,
            limit,
        } => {
            handle_workspace_symbols(&query, directory.as_ref(), limit, &config)?;
        }
        Commands::Lsp { action } => match action {
            LspAction::Health { directory, file } => {
                handle_lsp_health(directory.as_ref(), file.as_ref(), &config)?;
            }
            LspAction::Highlights { target, directory } => {
                handle_lsp_highlights(&target, directory.as_ref(), &config)?;
            }
            LspAction::Refs { target, directory } => {
                handle_lsp_grouped_refs(&target, directory.as_ref(), &config)?;
            }
            LspAction::Rename {
                target,
                new_name,
                directory,
                apply,
                dry_run,
            } => {
                handle_lsp_rename(&target, &new_name, directory.as_ref(), apply, dry_run, &config)?;
            }
            LspAction::CodeActions {
                target,
                directory,
                format,
                apply,
                dry_run,
            } => {
                handle_lsp_code_actions(&target, directory.as_ref(), &format, apply, dry_run, &config)?;
            }
            LspAction::OrganizeImports {
                target,
                directory,
                apply,
                dry_run,
            } => {
                handle_lsp_organize_imports(&target, directory.as_ref(), apply, dry_run, &config)?;
            }
            LspAction::Outline {
                target,
                directory,
                format,
            } => {
                handle_lsp_outline(&target, directory.as_ref(), &format, &config)?;
            }
            LspAction::Breadcrumbs {
                target,
                directory,
                format,
            } => {
                handle_lsp_breadcrumbs(&target, directory.as_ref(), &format, &config)?;
            }
            LspAction::SemanticTokens {
                target,
                directory,
                line,
                format,
            } => {
                handle_lsp_semantic_tokens(&target, directory.as_ref(), line, &format, &config)?;
            }
            LspAction::CallTree { target, directory } => {
                handle_lsp_call_tree(&target, directory.as_ref(), &config)?;
            }
        },
        Commands::Trace { action } => match action {
            TraceAction::Add {
                target,
                label,
                kind,
                session,
                parent,
                branch,
                tags,
            } => {
                handle_trace_add(&target, label.as_ref(), &kind, session, parent, branch, tags)?;
            }
            TraceAction::List {
                session,
                tag,
                kind,
                status,
                priority,
            } => {
                handle_trace_list(TraceListFilter {
                    session: session.as_ref(),
                    tag: tag.as_ref(),
                    kind: kind.as_ref(),
                    status: status.as_ref(),
                    priority: priority.as_ref(),
                })?;
            }
            TraceAction::Note { id, note } => {
                let change = fcs::trace::update_entry_note(&id, &note)?;
                handle_trace_entry_change(&id, change, "note");
            }
            TraceAction::Status { id, status } => {
                let change = fcs::trace::update_entry_status(&id, &status)?;
                handle_trace_entry_change(&id, change, "status");
            }
            TraceAction::Priority { id, priority } => {
                let change = fcs::trace::update_entry_priority(&id, &priority)?;
                handle_trace_entry_change(&id, change, "priority");
            }
            TraceAction::Sessions { archived } => {
                handle_trace_sessions(archived)?;
            }
            TraceAction::Archive { session } => {
                handle_trace_archive(&session)?;
            }
            TraceAction::Unarchive { session } => {
                handle_trace_unarchive(&session)?;
            }
            TraceAction::Report {
                session,
                directory,
                format,
            } => {
                handle_trace_report(&session, directory.as_ref(), &format)?;
            }
            TraceAction::Timeline {
                session,
                directory,
                format,
            } => {
                handle_trace_timeline(&session, directory.as_ref(), &format)?;
            }
            TraceAction::Replay {
                session,
                directory,
                format,
            } => {
                handle_trace_replay(&session, directory.as_ref(), &format)?;
            }
            TraceAction::ReplayPlan {
                session,
                directory,
                program,
                name,
                format,
            } => {
                handle_trace_replay_plan(&session, directory.as_ref(), program.as_ref(), name.as_ref(), &format)?;
            }
            TraceAction::Structured {
                session,
                directory,
                format,
            } => {
                handle_trace_structured(&session, directory.as_ref(), &format)?;
            }
            TraceAction::Insights {
                session,
                directory,
                format,
            } => {
                handle_trace_insights(&session, directory.as_ref(), &format)?;
            }
            TraceAction::Diff {
                left,
                right,
                directory,
                format,
            } => {
                handle_trace_diff(&left, &right, directory.as_ref(), &format)?;
            }
            TraceAction::Open => {
                handle_trace_open(&config)?;
            }
            TraceAction::Clear => {
                fcs::trace::clear()?;
                println!("Trace history cleared");
            }
            TraceAction::Export { directory, format } => {
                let root = match directory.as_ref() {
                    Some(directory) => Some(fcs::workspace::resolve_root(Some(directory))?),
                    None => None,
                };
                match format.as_str() {
                    "markdown" | "md" => print!("{}", fcs::trace::export_markdown(root.as_deref())?),
                    "json" => print!("{}", fcs::trace::export_json(root.as_deref())?),
                    other => {
                        return Err(AppError::General(format!("Unsupported trace export format: {other}")));
                    }
                }
            }
            TraceAction::Graph { directory } => {
                let root = match directory.as_ref() {
                    Some(directory) => Some(fcs::workspace::resolve_root(Some(directory))?),
                    None => None,
                };
                print!("{}", fcs::trace::export_graph(root.as_deref())?);
            }
        },
        Commands::History { action } => match action {
            HistoryAction::List => {
                handle_history_list()?;
            }
            HistoryAction::Clear => {
                fcs::history::clear()?;
                println!("Query history cleared");
            }
        },
        Commands::Debug { action } => match action {
            DebugAction::Command {
                binary,
                debugger,
                breakpoints,
                args,
                cwd,
                env,
                run,
            } => {
                handle_debug_command(&binary, &debugger, &breakpoints, &args, cwd.as_ref(), &env, run)?;
            }
            DebugAction::Last {
                binary,
                debugger,
                args,
                cwd,
                env,
                run,
            } => {
                handle_debug_last(&binary, &debugger, &args, cwd.as_ref(), &env, run)?;
            }
            DebugAction::SaveProfile {
                name,
                binary,
                debugger,
                breakpoints,
                directory,
                args,
                cwd,
                env,
            } => {
                handle_debug_save_profile(DebugSaveProfileInput {
                    name: &name,
                    binary: &binary,
                    debugger: &debugger,
                    breakpoints: &breakpoints,
                    directory: directory.as_ref(),
                    args: &args,
                    cwd: cwd.as_ref(),
                    env: &env,
                })?;
            }
            DebugAction::Profiles { directory } => {
                handle_debug_profiles(directory.as_ref())?;
            }
            DebugAction::FromTrace {
                session,
                binary,
                name,
                debugger,
                directory,
                args,
                cwd,
                env,
                run,
            } => {
                handle_debug_from_trace(DebugFromTraceInput {
                    session: &session,
                    binary: &binary,
                    name: name.as_ref(),
                    debugger: &debugger,
                    directory: directory.as_ref(),
                    args: &args,
                    cwd: cwd.as_ref(),
                    env: &env,
                    run,
                })?;
            }
            DebugAction::DeleteProfile { name, directory } => {
                handle_debug_delete_profile(&name, directory.as_ref())?;
            }
            DebugAction::EnableBreakpoint { name, index, directory } => {
                handle_debug_set_breakpoint_enabled(&name, index, directory.as_ref(), true)?;
            }
            DebugAction::DisableBreakpoint { name, index, directory } => {
                handle_debug_set_breakpoint_enabled(&name, index, directory.as_ref(), false)?;
            }
            DebugAction::RunProfile { name, directory, run } => {
                handle_debug_run_profile(&name, directory.as_ref(), run)?;
            }
        },
        Commands::Dap { action } => match action {
            DapAction::Adapters { format } => {
                handle_dap_adapters(&format)?;
            }
            DapAction::Launch {
                program,
                adapter,
                name,
                breakpoints,
                cwd,
                env,
                stop_on_entry,
                bundle,
                args,
            } => {
                handle_dap_launch(
                    DapProfileInput {
                        name: name.as_ref(),
                        program: &program,
                        adapter: &adapter,
                        breakpoints: &breakpoints,
                        cwd: cwd.as_ref(),
                        env: &env,
                        stop_on_entry,
                        args: &args,
                    },
                    bundle,
                )?;
            }
            DapAction::SaveProfile {
                name,
                program,
                adapter,
                breakpoints,
                directory,
                cwd,
                env,
                stop_on_entry,
                args,
            } => {
                handle_dap_save_profile(
                    DapProfileInput {
                        name: Some(&name),
                        program: &program,
                        adapter: &adapter,
                        breakpoints: &breakpoints,
                        cwd: cwd.as_ref(),
                        env: &env,
                        stop_on_entry,
                        args: &args,
                    },
                    directory.as_ref(),
                )?;
            }
            DapAction::Profiles { directory } => {
                handle_dap_profiles(directory.as_ref())?;
            }
            DapAction::FromTrace {
                session,
                program,
                name,
                adapter,
                directory,
                cwd,
                env,
                stop_on_entry,
                args,
            } => {
                handle_dap_from_trace(DapFromTraceInput {
                    session: &session,
                    program: &program,
                    name: name.as_ref(),
                    adapter: &adapter,
                    directory: directory.as_ref(),
                    cwd: cwd.as_ref(),
                    env: &env,
                    stop_on_entry,
                    args: &args,
                })?;
            }
            DapAction::RequestProfile {
                name,
                directory,
                bundle,
            } => {
                handle_dap_request_profile(&name, directory.as_ref(), bundle)?;
            }
            DapAction::SessionSmoke {
                program,
                adapter,
                name,
                breakpoints,
                cwd,
                env,
                stop_on_entry,
                args,
            } => {
                handle_dap_session_smoke(DapProfileInput {
                    name: name.as_ref(),
                    program: &program,
                    adapter: &adapter,
                    breakpoints: &breakpoints,
                    cwd: cwd.as_ref(),
                    env: &env,
                    stop_on_entry,
                    args: &args,
                })?;
            }
            DapAction::AdapterSession {
                adapter_command,
                program,
                adapter,
                name,
                breakpoints,
                cwd,
                adapter_env,
                env,
                stop_on_entry,
                args,
            } => {
                handle_dap_adapter_session(
                    &adapter_command,
                    &adapter_env,
                    DapProfileInput {
                        name: name.as_ref(),
                        program: &program,
                        adapter: &adapter,
                        breakpoints: &breakpoints,
                        cwd: cwd.as_ref(),
                        env: &env,
                        stop_on_entry,
                        args: &args,
                    },
                )?;
            }
        },
        Commands::Files {
            directory,
            query,
            option,
        } => {
            handle_files(directory.as_ref(), query.as_ref(), &option, &config)?;
        }
        Commands::Symbol {
            directory,
            query,
            option,
        } => {
            handle_symbols(directory.as_ref(), query.as_ref(), &option, &config)?;
        }
        Commands::Search {
            pattern,
            directory,
            option,
        } => {
            handle_search(&pattern, directory.as_ref(), &option, &config)?;
        }
        Commands::Complete { shell } => {
            let mut cmd = Cli::command();
            let name = cmd.get_name().to_string();
            clap_complete::generate(shell, &mut cmd, name, &mut std::io::stdout());
        }
        Commands::Man { stdout, out_dir } => {
            handle_man(stdout, out_dir.as_ref())?;
        }
    }

    Ok(())
}
