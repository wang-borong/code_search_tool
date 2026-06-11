use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::errors::{AppError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorBundle {
    pub root: PathBuf,
    pub generated_at_unix: u64,
    pub workspace: DoctorWorkspaceSection,
    pub config_diagnostics: Vec<DoctorCheck>,
    pub index: DoctorIndexSection,
    pub service: Option<crate::service::ServiceSnapshot>,
    pub dap: DoctorDapSection,
    pub workflows: Vec<crate::workspace::DiagnosticWorkflow>,
    pub saved_queries: Vec<crate::query::SavedQuery>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorWorkspaceSection {
    pub project_type: String,
    pub languages: Vec<String>,
    pub build_systems: Vec<String>,
    pub active_profile: Option<String>,
    pub recommended_tasks: Vec<String>,
    pub blocking_warnings: Vec<String>,
    pub lsp_provider: String,
    pub lsp_available: bool,
    pub lsp_message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorCheck {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorIndexSection {
    pub status: DoctorIndexStatus,
    pub stats: Option<DoctorIndexStats>,
    pub shard_status: Option<crate::index::IndexShardStatus>,
    pub shard_report: Option<crate::index::IndexShardReport>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorIndexStatus {
    pub exists: bool,
    pub schema_status: String,
    pub stale: bool,
    pub corrupt: bool,
    pub files: usize,
    pub symbols: usize,
    pub changed_tracked_files: usize,
    pub missing_tracked_files: usize,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorIndexStats {
    pub source_size_bytes: u64,
    pub index_size_bytes: u64,
    pub languages: Vec<String>,
    pub symbol_kinds: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorDapSection {
    pub profiles: usize,
    pub adapters: Vec<crate::dap::DapAdapterDiscovery>,
    pub templates: Vec<crate::dap::DapAdapterTemplate>,
}

pub fn build_bundle(root: &Path, config: &crate::config::Config) -> Result<DoctorBundle> {
    let root_arg = root.display().to_string();
    let root = crate::workspace::resolve_root(Some(&root_arg))?;
    let plan = crate::workspace::startup_plan(&root, config)?;
    let config_diagnostics = crate::workspace::config_diagnostics(&root)?
        .into_iter()
        .map(|check| DoctorCheck {
            name: check.name,
            ok: check.ok,
            detail: check.detail,
        })
        .collect::<Vec<DoctorCheck>>();
    let index = index_section(&root)?;
    let service = crate::service::snapshot(&root, config).ok();
    let dap_profiles = crate::dap::list_profiles(&root).unwrap_or_default();
    let workflows = crate::workspace::diagnostic_workflows(&root, config)?;
    let saved_queries = crate::query::list_saved_queries(&root).unwrap_or_default();

    Ok(DoctorBundle {
        root,
        generated_at_unix: now_unix(),
        workspace: DoctorWorkspaceSection {
            project_type: plan.project_type,
            languages: plan.languages,
            build_systems: plan.build_systems,
            active_profile: plan.active_profile,
            recommended_tasks: plan.recommended_tasks,
            blocking_warnings: plan.blocking_warnings,
            lsp_provider: plan.lsp.provider,
            lsp_available: plan.lsp.available,
            lsp_message: plan.lsp.message,
        },
        config_diagnostics,
        index,
        service,
        dap: DoctorDapSection {
            profiles: dap_profiles.len(),
            adapters: crate::dap::discover_adapters(),
            templates: crate::dap::adapter_templates(),
        },
        workflows,
        saved_queries,
    })
}

pub fn format_bundle(bundle: &DoctorBundle, format: &str) -> Result<String> {
    match format {
        "json" => serde_json::to_string_pretty(bundle)
            .map(|mut json| {
                json.push('\n');
                json
            })
            .map_err(|err| AppError::General(err.to_string())),
        "text" | "markdown" | "md" => Ok(format_bundle_text(bundle)),
        other => Err(AppError::General(format!("Unsupported doctor bundle format: {other}"))),
    }
}

pub fn write_bundle(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    Ok(())
}

fn index_section(root: &Path) -> Result<DoctorIndexSection> {
    let status = crate::index::status(root)?;
    let stats = crate::index::stats(root).ok().map(|stats| DoctorIndexStats {
        source_size_bytes: stats.source_size_bytes,
        index_size_bytes: stats.index_size_bytes,
        languages: stats
            .languages
            .into_iter()
            .map(|entry| format!("{}={}", entry.name, entry.count))
            .collect(),
        symbol_kinds: stats
            .symbol_kinds
            .into_iter()
            .map(|entry| format!("{}={}", entry.name, entry.count))
            .collect(),
    });
    let shard_status = crate::index::shard_status(root).ok();
    let shard_report = crate::index::shard_report(root, 5000).ok();

    Ok(DoctorIndexSection {
        status: DoctorIndexStatus {
            exists: status.exists,
            schema_status: index_schema_status_label(status.schema_status).to_string(),
            stale: status.is_stale,
            corrupt: status.is_corrupt,
            files: status.file_count,
            symbols: status.symbol_count,
            changed_tracked_files: status.changed_tracked_files,
            missing_tracked_files: status.missing_tracked_files,
            message: status.message,
        },
        stats,
        shard_status,
        shard_report,
        error: None,
    })
}

fn format_bundle_text(bundle: &DoctorBundle) -> String {
    let mut output = String::new();
    output.push_str(&format!("root: {}\n", bundle.root.display()));
    output.push_str(&format!("generated_at_unix: {}\n", bundle.generated_at_unix));
    output.push_str(&format!("project_type: {}\n", bundle.workspace.project_type));
    output.push_str(&format!("languages: {}\n", display_list(&bundle.workspace.languages)));
    output.push_str(&format!(
        "build_systems: {}\n",
        display_list(&bundle.workspace.build_systems)
    ));
    output.push_str(&format!(
        "lsp: {} available={} {}\n",
        bundle.workspace.lsp_provider, bundle.workspace.lsp_available, bundle.workspace.lsp_message
    ));
    output.push_str(&format!(
        "index: exists={} stale={} corrupt={} files={} symbols={}\n",
        bundle.index.status.exists,
        bundle.index.status.stale,
        bundle.index.status.corrupt,
        bundle.index.status.files,
        bundle.index.status.symbols
    ));
    if let Some(shards) = &bundle.index.shard_status {
        output.push_str(&format!(
            "shards: exists={} stale={} count={} reason={}\n",
            shards.exists, shards.stale, shards.shard_count, shards.reason
        ));
    }
    output.push_str(&format!("dap_profiles: {}\n", bundle.dap.profiles));
    output.push_str(&format!("dap_adapters: {}\n", bundle.dap.adapters.len()));
    output.push_str(&format!("workflows: {}\n", bundle.workflows.len()));
    output.push_str(&format!("saved_queries: {}\n", bundle.saved_queries.len()));
    output.push_str("config_diagnostics:\n");
    for check in &bundle.config_diagnostics {
        let state = if check.ok { "ok" } else { "warn" };
        output.push_str(&format!("  [{state}] {}: {}\n", check.name, check.detail));
    }
    output
}

fn display_list(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
    }
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

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_doctor_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("fcs_doctor_{name}_{}", std::process::id()))
    }

    #[test]
    fn bundle_formats_without_index_or_project_config() {
        let temp_dir = temp_doctor_dir("missing_index");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("src")).unwrap();
        fs::write(temp_dir.join("Cargo.toml"), "[package]\nname = \"fixture\"\n").unwrap();
        fs::write(temp_dir.join("src").join("main.rs"), "fn main() {}\n").unwrap();
        let cache_dir = crate::workspace::cache_dir_for_root(&temp_dir).unwrap();
        let _ = fs::remove_dir_all(&cache_dir);
        crate::query::save_query(
            &temp_dir,
            "main",
            "kind:function name:main",
            crate::query::QuerySource::Index,
            crate::query::QueryMode::Exact,
        )
        .unwrap();

        let bundle = build_bundle(&temp_dir, &crate::config::Config::default()).unwrap();
        let text = format_bundle(&bundle, "text").unwrap();
        let json = format_bundle(&bundle, "json").unwrap();

        assert_eq!(bundle.root, temp_dir);
        assert!(!bundle.index.status.exists);
        assert_eq!(bundle.index.status.schema_status, "missing");
        assert!(bundle
            .dap
            .templates
            .iter()
            .any(|template| template.adapter == "codelldb"));
        assert!(bundle.saved_queries.iter().any(|query| query.name == "main"));
        assert!(text.contains("index: exists=false"));
        assert!(text.contains("saved_queries: 1"));
        assert!(json.contains("\"saved_queries\""));
        assert!(json.contains("\"templates\""));

        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::remove_dir_all(&cache_dir);
    }
}
