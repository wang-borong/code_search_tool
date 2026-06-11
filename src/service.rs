use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::errors::{AppError, Result};

const SERVICE_HEARTBEAT_FILE: &str = "service-daemon.toml";
const SERVICE_SNAPSHOT_FILE: &str = "service-snapshot.json";
const SERVICE_STOP_FILE: &str = "service-stop";
const MAX_SERVICE_REPORT_CYCLES: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceOptions {
    pub interval_ms: u64,
    pub max_cycles: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceCycle {
    pub cycle: usize,
    pub timestamp_unix: u64,
    pub elapsed_ms: u128,
    pub index_rebuilt: bool,
    pub index_reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceDaemonReport {
    pub root: PathBuf,
    pub heartbeat_path: PathBuf,
    pub snapshot_path: PathBuf,
    pub cycles: Vec<ServiceCycle>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceHeartbeat {
    pub root: String,
    pub pid: u32,
    pub started_at_unix: u64,
    pub updated_at_unix: u64,
    pub interval_ms: u64,
    pub cycles: usize,
    pub last_index_rebuilt: bool,
    pub last_index_reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceSnapshot {
    pub root: PathBuf,
    pub cache_dir: PathBuf,
    pub timestamp_unix: u64,
    pub index: ServiceIndexSnapshot,
    pub lsp: ServiceLspSnapshot,
    pub trace: ServiceTraceSnapshot,
    pub plugins: ServicePluginSnapshot,
    pub workspace_profile: Option<ServiceWorkspaceProfileSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceIndexSnapshot {
    pub exists: bool,
    pub schema_status: String,
    pub stale: bool,
    pub corrupt: bool,
    pub files: usize,
    pub symbols: usize,
    pub daemon_exists: bool,
    pub daemon_stale: bool,
    pub daemon_cycles: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceLspSnapshot {
    pub provider: String,
    pub command: String,
    pub status: String,
    pub version: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceTraceSnapshot {
    pub entries: usize,
    pub sessions: usize,
    pub archived_sessions: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServicePluginSnapshot {
    pub plugins: usize,
    pub diagnostics: usize,
    pub failed_diagnostics: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceWorkspaceProfileSnapshot {
    pub name: String,
    pub root: PathBuf,
    pub project_type: String,
    pub index_roots: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceStatus {
    pub heartbeat_path: PathBuf,
    pub snapshot_path: PathBuf,
    pub stop_path: PathBuf,
    pub heartbeat: Option<ServiceHeartbeat>,
    pub snapshot: Option<ServiceSnapshot>,
    pub stale: bool,
    pub stop_requested: bool,
}

pub fn run_daemon(
    root: &Path,
    file_options: &[String],
    default_ignore: &[String],
    ignore_file: &Path,
    config: &crate::config::Config,
    options: ServiceOptions,
) -> Result<ServiceDaemonReport> {
    if options.max_cycles == Some(0) {
        return Err(AppError::General(
            "service start --max-cycles must be greater than zero".to_string(),
        ));
    }

    let root = normalize_root(root);
    let heartbeat_path = heartbeat_path(&root)?;
    let snapshot_path = snapshot_path(&root)?;
    let stop_path = stop_path(&root)?;
    let _ = fs::remove_file(&stop_path);
    let started_at_unix = now_unix();
    let pid = std::process::id();
    let mut report = ServiceDaemonReport {
        root: root.clone(),
        heartbeat_path: heartbeat_path.clone(),
        snapshot_path: snapshot_path.clone(),
        cycles: Vec::new(),
    };

    loop {
        let cycle_number = report.cycles.last().map_or(1, |cycle| cycle.cycle + 1);
        let started = Instant::now();
        let refresh = crate::index::refresh(&root, file_options, default_ignore, ignore_file)?;
        let snapshot = snapshot(&root, config)?;
        write_snapshot(&snapshot_path, &snapshot)?;
        let cycle = ServiceCycle {
            cycle: cycle_number,
            timestamp_unix: now_unix(),
            elapsed_ms: started.elapsed().as_millis(),
            index_rebuilt: refresh.rebuilt,
            index_reason: refresh.reason,
        };
        write_heartbeat(
            &heartbeat_path,
            &ServiceHeartbeat {
                root: root.display().to_string(),
                pid,
                started_at_unix,
                updated_at_unix: cycle.timestamp_unix,
                interval_ms: options.interval_ms,
                cycles: cycle.cycle,
                last_index_rebuilt: cycle.index_rebuilt,
                last_index_reason: cycle.index_reason.clone(),
            },
        )?;
        push_cycle(&mut report.cycles, cycle);

        if stop_path.exists() {
            break;
        }
        if options
            .max_cycles
            .is_some_and(|max_cycles| report.cycles.last().is_some_and(|cycle| cycle.cycle >= max_cycles))
        {
            break;
        }
        if options.interval_ms > 0 {
            thread::sleep(Duration::from_millis(options.interval_ms));
        }
    }

    Ok(report)
}

pub fn status(root: &Path) -> Result<ServiceStatus> {
    let root = normalize_root(root);
    let heartbeat_path = heartbeat_path(&root)?;
    let snapshot_path = snapshot_path(&root)?;
    let stop_path = stop_path(&root)?;
    let heartbeat = if heartbeat_path.exists() {
        Some(
            toml::from_str::<ServiceHeartbeat>(&fs::read_to_string(&heartbeat_path)?)
                .map_err(|err| AppError::General(format!("Corrupt service heartbeat: {err}")))?,
        )
    } else {
        None
    };
    let snapshot = if snapshot_path.exists() {
        Some(
            serde_json::from_str::<ServiceSnapshot>(&fs::read_to_string(&snapshot_path)?)
                .map_err(|err| AppError::General(format!("Corrupt service snapshot: {err}")))?,
        )
    } else {
        None
    };
    let stale = heartbeat.as_ref().is_some_and(|heartbeat| {
        let grace_secs = ((heartbeat.interval_ms / 1000).max(1) * 3).max(5);
        now_unix().saturating_sub(heartbeat.updated_at_unix) > grace_secs
    });

    Ok(ServiceStatus {
        heartbeat_path,
        snapshot_path,
        stop_path: stop_path.clone(),
        heartbeat,
        snapshot,
        stale,
        stop_requested: stop_path.exists(),
    })
}

pub fn snapshot(root: &Path, config: &crate::config::Config) -> Result<ServiceSnapshot> {
    let root = normalize_root(root);
    let cache_dir = crate::workspace::cache_dir_for_root(&root)?;
    let index = crate::index::status(&root)?;
    let daemon = crate::index::daemon_status(&root).ok();
    let provider = crate::lsp::provider_for_workspace(&root, &config.lsp);
    let lsp = crate::lsp::provider_health(&provider);
    let trace_entries = crate::trace::list()?;
    let trace_sessions = crate::trace::list_sessions(true)?;
    let plugins = crate::plugins::discover(Some(&root))?;
    let plugin_diagnostics = crate::plugins::doctor(Some(&root))?;
    let workspace_profile = crate::workspace::current_profile()?.map(|profile| ServiceWorkspaceProfileSnapshot {
        name: profile.name,
        root: profile.root,
        project_type: profile.project_type,
        index_roots: profile.index_roots,
    });

    Ok(ServiceSnapshot {
        root,
        cache_dir,
        timestamp_unix: now_unix(),
        index: ServiceIndexSnapshot {
            exists: index.exists,
            schema_status: index_schema_status_label(index.schema_status).to_string(),
            stale: index.is_stale,
            corrupt: index.is_corrupt,
            files: index.file_count,
            symbols: index.symbol_count,
            daemon_exists: daemon.as_ref().is_some_and(|daemon| daemon.exists),
            daemon_stale: daemon.as_ref().is_some_and(|daemon| daemon.stale),
            daemon_cycles: daemon
                .and_then(|daemon| daemon.heartbeat.map(|heartbeat| heartbeat.cycles))
                .unwrap_or(0),
        },
        lsp: ServiceLspSnapshot {
            provider: lsp.kind.as_str().to_string(),
            command: lsp.command,
            status: lsp_status_label(lsp.status).to_string(),
            version: lsp.version,
            message: lsp.message,
        },
        trace: ServiceTraceSnapshot {
            entries: trace_entries.len(),
            sessions: trace_sessions.len(),
            archived_sessions: trace_sessions.iter().filter(|session| session.is_archived()).count(),
        },
        plugins: ServicePluginSnapshot {
            plugins: plugins.len(),
            diagnostics: plugin_diagnostics.len(),
            failed_diagnostics: plugin_diagnostics.iter().filter(|diagnostic| !diagnostic.ok).count(),
        },
        workspace_profile,
    })
}

pub fn request_stop(root: &Path) -> Result<PathBuf> {
    let path = stop_path(&normalize_root(root))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, b"stop\n")?;
    Ok(path)
}

pub fn format_snapshot(snapshot: &ServiceSnapshot, format: &str) -> Result<String> {
    match format {
        "text" => {
            let mut output = String::new();
            output.push_str(&format!("Root: {}\n", snapshot.root.display()));
            output.push_str(&format!("Cache: {}\n", snapshot.cache_dir.display()));
            output.push_str(&format!(
                "Index: exists={} schema={} stale={} corrupt={} files={} symbols={}\n",
                snapshot.index.exists,
                snapshot.index.schema_status,
                snapshot.index.stale,
                snapshot.index.corrupt,
                snapshot.index.files,
                snapshot.index.symbols
            ));
            output.push_str(&format!(
                "Index daemon: exists={} stale={} cycles={}\n",
                snapshot.index.daemon_exists, snapshot.index.daemon_stale, snapshot.index.daemon_cycles
            ));
            output.push_str(&format!(
                "LSP: {} command={} status={} version={}\n",
                snapshot.lsp.provider,
                snapshot.lsp.command,
                snapshot.lsp.status,
                snapshot.lsp.version.as_deref().unwrap_or("-")
            ));
            output.push_str(&format!(
                "Trace: entries={} sessions={} archived={}\n",
                snapshot.trace.entries, snapshot.trace.sessions, snapshot.trace.archived_sessions
            ));
            output.push_str(&format!(
                "Plugins: plugins={} diagnostics={} failed={}\n",
                snapshot.plugins.plugins, snapshot.plugins.diagnostics, snapshot.plugins.failed_diagnostics
            ));
            if let Some(profile) = &snapshot.workspace_profile {
                output.push_str(&format!(
                    "Active profile: {} root={} project_type={}\n",
                    profile.name,
                    profile.root.display(),
                    profile.project_type
                ));
            }
            Ok(output)
        }
        "json" => serde_json::to_string_pretty(snapshot)
            .map(|mut json| {
                json.push('\n');
                json
            })
            .map_err(|err| AppError::General(err.to_string())),
        other => Err(AppError::General(format!(
            "Unsupported service snapshot format: {other}"
        ))),
    }
}

fn heartbeat_path(root: &Path) -> Result<PathBuf> {
    Ok(crate::workspace::cache_dir_for_root(root)?.join(SERVICE_HEARTBEAT_FILE))
}

fn snapshot_path(root: &Path) -> Result<PathBuf> {
    Ok(crate::workspace::cache_dir_for_root(root)?.join(SERVICE_SNAPSHOT_FILE))
}

fn stop_path(root: &Path) -> Result<PathBuf> {
    Ok(crate::workspace::cache_dir_for_root(root)?.join(SERVICE_STOP_FILE))
}

fn write_heartbeat(path: &Path, heartbeat: &ServiceHeartbeat) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let contents = toml::to_string_pretty(heartbeat).map_err(|err| AppError::General(err.to_string()))?;
    fs::write(path, contents)?;
    Ok(())
}

fn write_snapshot(path: &Path, snapshot: &ServiceSnapshot) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let contents = serde_json::to_string_pretty(snapshot).map_err(|err| AppError::General(err.to_string()))?;
    fs::write(path, contents)?;
    Ok(())
}

fn push_cycle(cycles: &mut Vec<ServiceCycle>, cycle: ServiceCycle) {
    if cycles.len() >= MAX_SERVICE_REPORT_CYCLES {
        cycles.remove(0);
    }
    cycles.push(cycle);
}

fn index_schema_status_label(status: crate::index::IndexSchemaStatus) -> &'static str {
    match status {
        crate::index::IndexSchemaStatus::Missing => "missing",
        crate::index::IndexSchemaStatus::Current => "current",
        crate::index::IndexSchemaStatus::Migrated => "migrated",
        crate::index::IndexSchemaStatus::Future => "future",
        crate::index::IndexSchemaStatus::Corrupt => "corrupt",
    }
}

fn lsp_status_label(status: crate::lsp::LspProviderHealthStatus) -> &'static str {
    match status {
        crate::lsp::LspProviderHealthStatus::Available => "available",
        crate::lsp::LspProviderHealthStatus::Unavailable => "unavailable",
    }
}

fn normalize_root(root: &Path) -> PathBuf {
    root.canonicalize().unwrap_or_else(|_| root.to_path_buf())
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_snapshot_text() {
        let snapshot = ServiceSnapshot {
            root: PathBuf::from("/tmp/fcs"),
            cache_dir: PathBuf::from("/tmp/fcs-cache"),
            timestamp_unix: 1,
            index: ServiceIndexSnapshot {
                exists: true,
                schema_status: "current".to_string(),
                stale: false,
                corrupt: false,
                files: 1,
                symbols: 2,
                daemon_exists: false,
                daemon_stale: false,
                daemon_cycles: 0,
            },
            lsp: ServiceLspSnapshot {
                provider: "clangd".to_string(),
                command: "clangd".to_string(),
                status: "available".to_string(),
                version: Some("clangd version".to_string()),
                message: "ok".to_string(),
            },
            trace: ServiceTraceSnapshot {
                entries: 0,
                sessions: 0,
                archived_sessions: 0,
            },
            plugins: ServicePluginSnapshot {
                plugins: 1,
                diagnostics: 1,
                failed_diagnostics: 0,
            },
            workspace_profile: None,
        };

        let output = format_snapshot(&snapshot, "text").unwrap();

        assert!(output.contains("Index: exists=true"));
        assert!(output.contains("LSP: clangd"));
    }
}
