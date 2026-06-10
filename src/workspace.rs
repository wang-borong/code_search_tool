use std::collections::{hash_map::DefaultHasher, BTreeSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::ActionConfig;
use crate::errors::{AppError, Result};

const DEFAULT_LATENCY_WARN_MS: u64 = 500;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectConfig {
    pub project_type: String,
    pub languages: Vec<String>,
    pub build_systems: Vec<String>,
    pub clangd_command: String,
    pub rust_analyzer_command: String,
    pub default_debug_binary: String,
    pub debug_targets: Vec<String>,
    pub index_roots: Vec<String>,
    pub search_ignore: Vec<String>,
    pub log_dir: String,
    pub latency_warn_ms: u64,
    #[serde(default)]
    pub actions: Vec<ActionConfig>,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            project_type: "generic".to_string(),
            languages: Vec::new(),
            build_systems: Vec::new(),
            clangd_command: "clangd".to_string(),
            rust_analyzer_command: crate::lsp::RUST_ANALYZER_COMMAND.to_string(),
            default_debug_binary: "target/debug/app".to_string(),
            debug_targets: Vec::new(),
            index_roots: vec![".".to_string()],
            search_ignore: vec![".git/".to_string(), "target/".to_string(), "node_modules/".to_string()],
            log_dir: ".fcs/logs".to_string(),
            latency_warn_ms: DEFAULT_LATENCY_WARN_MS,
            actions: Vec::new(),
        }
    }
}

impl ProjectConfig {
    pub fn for_workspace(root: &Path) -> Result<Self> {
        let detection = detect_project(root)?;
        let mut config = Self::default();
        config.project_type = detection.project_type;
        config.languages = detection.languages;
        config.build_systems = detection.build_systems;
        config.default_debug_binary = detection
            .debug_targets
            .first()
            .map(|target| path_for_config(root, target))
            .unwrap_or_else(|| default_debug_binary_for_project(&config.project_type));
        config.debug_targets = detection
            .debug_targets
            .iter()
            .map(|target| path_for_config(root, target))
            .collect();
        config.index_roots = detection.index_roots;
        config.search_ignore = search_ignores_for_project(&config.project_type);
        config.actions = detection.suggested_actions;
        Ok(config)
    }
}

#[derive(Debug, Clone)]
pub struct WorkspaceStatus {
    pub root: PathBuf,
    pub cache_dir: PathBuf,
    pub has_compile_commands: bool,
    pub has_compile_flags: bool,
    pub clangd_available: bool,
    pub clangd_version: Option<String>,
    pub rust_analyzer_available: bool,
    pub rust_analyzer_version: Option<String>,
    pub has_cargo_toml: bool,
}

impl WorkspaceStatus {
    pub fn is_semantic_ready(&self) -> bool {
        self.is_cpp_semantic_ready() || self.is_rust_semantic_ready()
    }

    pub fn is_cpp_semantic_ready(&self) -> bool {
        self.clangd_available && (self.has_compile_commands || self.has_compile_flags)
    }

    pub fn is_rust_semantic_ready(&self) -> bool {
        self.has_cargo_toml && self.rust_analyzer_available
    }

    pub fn semantic_status_label(&self) -> &'static str {
        if self.is_semantic_ready() {
            "semantic: ready"
        } else if self.has_cargo_toml && !self.rust_analyzer_available {
            "semantic: rust-analyzer missing"
        } else if self.clangd_available {
            "semantic: provider partial"
        } else {
            "semantic: off"
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceAdviceReport {
    pub root: PathBuf,
    pub project_type: String,
    pub config_path: PathBuf,
    pub build_systems: Vec<String>,
    pub languages: Vec<String>,
    pub debug_targets: Vec<PathBuf>,
    pub cache_checks: Vec<WorkspaceHealthCheck>,
    pub advice: Vec<WorkspaceAdvice>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceHealthCheck {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceAdvice {
    pub level: AdviceLevel,
    pub message: String,
    pub action: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdviceLevel {
    Info,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDetection {
    pub project_type: String,
    pub build_systems: Vec<String>,
    pub languages: Vec<String>,
    pub debug_targets: Vec<PathBuf>,
    pub index_roots: Vec<String>,
    pub suggested_actions: Vec<ActionConfig>,
}

impl AdviceLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            AdviceLevel::Info => "info",
            AdviceLevel::Warning => "warning",
        }
    }
}

pub fn status(directory: Option<&String>, clangd_command: &str) -> Result<WorkspaceStatus> {
    status_with_lsp_commands(directory, clangd_command, crate::lsp::RUST_ANALYZER_COMMAND)
}

pub fn status_with_lsp_commands(
    directory: Option<&String>,
    clangd_command: &str,
    rust_analyzer_command: &str,
) -> Result<WorkspaceStatus> {
    let root = resolve_root(directory)?;
    let cache_dir = workspace_cache_dir(&root)?;
    let clangd_version = crate::lsp::provider_version(clangd_command);
    let rust_analyzer_version = crate::lsp::provider_version(rust_analyzer_command);

    Ok(WorkspaceStatus {
        has_compile_commands: root.join("compile_commands.json").exists()
            || root.join("build").join("compile_commands.json").exists(),
        has_compile_flags: root.join("compile_flags.txt").exists(),
        clangd_available: clangd_version.is_some(),
        clangd_version,
        rust_analyzer_available: rust_analyzer_version.is_some(),
        rust_analyzer_version,
        has_cargo_toml: root.join("Cargo.toml").exists(),
        cache_dir,
        root,
    })
}

pub fn init(directory: Option<&String>, clangd_command: &str) -> Result<WorkspaceStatus> {
    let status = status(directory, clangd_command)?;
    fs::create_dir_all(&status.cache_dir)?;

    let metadata = format!(
        "root = \"{}\"\nclangd_available = {}\nsemantic_ready = {}\n",
        status.root.display(),
        status.clangd_available,
        status.is_semantic_ready()
    );
    fs::write(status.cache_dir.join("workspace.toml"), metadata)?;

    Ok(status)
}

pub fn advise(directory: Option<&String>, clangd_command: &str) -> Result<WorkspaceAdviceReport> {
    advise_with_lsp_commands(directory, clangd_command, crate::lsp::RUST_ANALYZER_COMMAND)
}

pub fn advise_with_lsp_commands(
    directory: Option<&String>,
    clangd_command: &str,
    rust_analyzer_command: &str,
) -> Result<WorkspaceAdviceReport> {
    let status = status_with_lsp_commands(directory, clangd_command, rust_analyzer_command)?;
    let detection = detect_project(&status.root)?;
    let config_path = status.root.join(".fcs.toml");
    let project_config = read_project_config(&status.root)?;
    let cache_checks = detect_cache_checks(&status, project_config.as_ref());
    let build_systems = detection.build_systems.clone();
    let languages = detection.languages.clone();
    let debug_targets = detection.debug_targets.clone();
    let mut advice = Vec::new();
    let has_cpp = languages.iter().any(|language| language == "C/C++");
    let has_rust = languages.iter().any(|language| language == "Rust");

    advice.push(WorkspaceAdvice {
        level: AdviceLevel::Info,
        message: format!("Detected project profile: {}", detection.project_type),
        action: None,
    });

    if has_cpp && !status.clangd_available {
        advice.push(WorkspaceAdvice {
            level: AdviceLevel::Warning,
            message: format!("clangd command is not available: {clangd_command}"),
            action: Some("Install clangd or set lsp.clangd_command in fcs.toml".to_string()),
        });
    }

    if has_cpp && !status.has_compile_commands && !status.has_compile_flags {
        advice.push(WorkspaceAdvice {
            level: AdviceLevel::Warning,
            message: "C/C++ files detected, but clangd has no compile database".to_string(),
            action: Some("Generate compile_commands.json or add compile_flags.txt".to_string()),
        });
    }

    if has_rust && !status.rust_analyzer_available {
        advice.push(WorkspaceAdvice {
            level: AdviceLevel::Warning,
            message: format!("rust-analyzer command is not available: {rust_analyzer_command}"),
            action: Some("Install rust-analyzer and ensure it is available on PATH".to_string()),
        });
    }

    if build_systems.iter().any(|system| system == "CMake") && !status.has_compile_commands {
        advice.push(WorkspaceAdvice {
            level: AdviceLevel::Info,
            message: "CMake project detected without compile_commands.json at the workspace root".to_string(),
            action: Some("Run: cmake -S . -B build -DCMAKE_EXPORT_COMPILE_COMMANDS=ON".to_string()),
        });
    }

    if !status.cache_dir.exists() {
        advice.push(WorkspaceAdvice {
            level: AdviceLevel::Info,
            message: "Workspace cache has not been initialized".to_string(),
            action: Some("Run: rtk cargo run -- workspace init".to_string()),
        });
    }

    if !status.cache_dir.join("workspace.toml").exists() {
        advice.push(WorkspaceAdvice {
            level: AdviceLevel::Info,
            message: "Workspace metadata cache is missing".to_string(),
            action: Some("Run: rtk cargo run -- workspace init".to_string()),
        });
    }

    if !config_path.exists() {
        advice.push(WorkspaceAdvice {
            level: AdviceLevel::Info,
            message: "Project-level fcs config is missing".to_string(),
            action: Some("Run: rtk cargo run -- workspace config".to_string()),
        });
    } else if let Some(project_config) = project_config.as_ref() {
        if project_config.actions.is_empty() && !detection.suggested_actions.is_empty() {
            advice.push(WorkspaceAdvice {
                level: AdviceLevel::Info,
                message: "Project config has no actions for the detected project type".to_string(),
                action: Some("Run: rtk cargo run -- workspace config --force".to_string()),
            });
        }
        if project_config.project_type == "generic" && detection.project_type != "generic" {
            advice.push(WorkspaceAdvice {
                level: AdviceLevel::Info,
                message: "Project config was generated before project auto-detection metadata".to_string(),
                action: Some("Run: rtk cargo run -- workspace config --force".to_string()),
            });
        }
    }

    if !status.cache_dir.join("code_index.toml").exists() {
        advice.push(WorkspaceAdvice {
            level: AdviceLevel::Info,
            message: "Code index cache is missing".to_string(),
            action: Some("Run: rtk cargo run -- index build".to_string()),
        });
    }

    if !status.cache_dir.join("latency-smoke.tsv").exists() {
        advice.push(WorkspaceAdvice {
            level: AdviceLevel::Info,
            message: "Latency smoke data has not been recorded for this workspace".to_string(),
            action: Some("Run: rtk scripts/smoke.sh".to_string()),
        });
    }

    for check in cache_checks.iter().filter(|check| !check.ok) {
        advice.push(WorkspaceAdvice {
            level: AdviceLevel::Info,
            message: format!("Health check '{}' needs attention: {}", check.name, check.detail),
            action: health_check_action(check),
        });
    }

    if cache_checks.iter().any(|check| !check.ok) {
        advice.push(WorkspaceAdvice {
            level: AdviceLevel::Warning,
            message: "Workspace health checks reported setup gaps".to_string(),
            action: Some("Review cache/log/latency advice above before release".to_string()),
        });
    }

    if build_systems.is_empty() {
        advice.push(WorkspaceAdvice {
            level: AdviceLevel::Info,
            message: "No known build system marker was detected".to_string(),
            action: Some(
                "Add a project marker such as Cargo.toml, CMakeLists.txt, Makefile, or compile_commands.json"
                    .to_string(),
            ),
        });
    }

    if debug_targets.is_empty() {
        advice.push(WorkspaceAdvice {
            level: AdviceLevel::Info,
            message: "No obvious debug binary was detected".to_string(),
            action: Some("Save one with: fcs debug save-profile <name> <binary>".to_string()),
        });
    }

    if has_cpp && status.is_cpp_semantic_ready() {
        advice.push(WorkspaceAdvice {
            level: AdviceLevel::Info,
            message: "clangd semantic navigation is ready".to_string(),
            action: None,
        });
    }

    if has_rust && status.is_rust_semantic_ready() {
        advice.push(WorkspaceAdvice {
            level: AdviceLevel::Info,
            message: "rust-analyzer semantic navigation is ready".to_string(),
            action: None,
        });
    }

    Ok(WorkspaceAdviceReport {
        root: status.root,
        project_type: detection.project_type,
        config_path,
        build_systems,
        languages,
        debug_targets,
        cache_checks,
        advice,
    })
}

pub fn write_project_config(directory: Option<&String>, force: bool) -> Result<PathBuf> {
    let root = resolve_root(directory)?;
    let path = root.join(".fcs.toml");
    if path.exists() && !force {
        return Err(AppError::General(format!(
            "Project config already exists: {}",
            path.display()
        )));
    }

    let contents =
        toml::to_string_pretty(&ProjectConfig::for_workspace(&root)?).map_err(|e| AppError::General(e.to_string()))?;
    fs::write(&path, contents)?;
    Ok(path)
}

pub fn read_project_config(root: &Path) -> Result<Option<ProjectConfig>> {
    let path = root.join(".fcs.toml");
    if !path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(path)?;
    toml::from_str(&contents)
        .map(Some)
        .map_err(|e| AppError::General(e.to_string()))
}

pub fn cache_dir_for_root(root: &Path) -> Result<PathBuf> {
    workspace_cache_dir(root)
}

pub fn resolve_root(directory: Option<&String>) -> Result<PathBuf> {
    let start = match directory {
        Some(directory) => Path::new(directory).to_path_buf(),
        None => std::env::current_dir()?,
    };

    let mut current = start.canonicalize().unwrap_or(start);
    if current.is_file() {
        current = current
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| AppError::General("Could not resolve workspace root".to_string()))?;
    }

    let mut cursor = current.as_path();
    loop {
        if is_workspace_marker(cursor) {
            return Ok(cursor.to_path_buf());
        }

        if let Some(parent) = cursor.parent() {
            cursor = parent;
        } else {
            return Ok(current);
        }
    }
}

fn is_workspace_marker(path: &Path) -> bool {
    path.join(".git").exists()
        || path.join("compile_commands.json").exists()
        || path.join("compile_flags.txt").exists()
        || path.join("Cargo.toml").exists()
        || path.join("CMakeLists.txt").exists()
}

pub fn detect_project(root: &Path) -> Result<ProjectDetection> {
    let build_systems = detect_build_systems(root);
    let languages = detect_languages(root)?;
    let debug_targets = detect_debug_targets(root);
    let project_type = project_type_for(&build_systems, &languages);
    let index_roots = detect_index_roots(root, &languages);
    let suggested_actions = suggested_actions_for(&build_systems, &languages);

    Ok(ProjectDetection {
        project_type,
        build_systems,
        languages,
        debug_targets,
        index_roots,
        suggested_actions,
    })
}

fn detect_build_systems(root: &Path) -> Vec<String> {
    let markers = [
        ("Cargo.toml", "Cargo"),
        ("CMakeLists.txt", "CMake"),
        ("Makefile", "Make"),
        ("compile_commands.json", "clang compile database"),
        ("compile_flags.txt", "clang compile flags"),
        ("package.json", "Node"),
        ("pyproject.toml", "Python"),
        ("go.mod", "Go"),
    ];

    markers
        .iter()
        .filter(|(marker, _)| root.join(marker).exists())
        .map(|(_, label)| (*label).to_string())
        .collect()
}

fn project_type_for(build_systems: &[String], languages: &[String]) -> String {
    if build_systems.iter().any(|system| system == "Cargo") || languages.iter().any(|language| language == "Rust") {
        return "rust".to_string();
    }
    if build_systems
        .iter()
        .any(|system| system == "CMake" || system == "Make" || system.starts_with("clang"))
        || languages.iter().any(|language| language == "C/C++")
    {
        return "c-cpp".to_string();
    }
    if build_systems.iter().any(|system| system == "Go") || languages.iter().any(|language| language == "Go") {
        return "go".to_string();
    }
    if build_systems.iter().any(|system| system == "Python") || languages.iter().any(|language| language == "Python") {
        return "python".to_string();
    }
    if build_systems.iter().any(|system| system == "Node")
        || languages.iter().any(|language| language == "JavaScript/TypeScript")
    {
        return "node".to_string();
    }
    "generic".to_string()
}

fn detect_index_roots(root: &Path, languages: &[String]) -> Vec<String> {
    let mut roots = BTreeSet::new();
    for candidate in ["src", "include", "lib", "crates", "cmd", "pkg", "app"] {
        if root.join(candidate).is_dir() {
            roots.insert(candidate.to_string());
        }
    }

    if languages.iter().any(|language| language == "Python") && root.join("tests").is_dir() {
        roots.insert("tests".to_string());
    }

    if roots.is_empty() {
        roots.insert(".".to_string());
    }

    roots.into_iter().collect()
}

fn suggested_actions_for(build_systems: &[String], languages: &[String]) -> Vec<ActionConfig> {
    let mut actions = Vec::new();
    if build_systems.iter().any(|system| system == "Cargo") {
        actions.push(action("cargo-check", "Run cargo check", "cargo", &["check"]));
        actions.push(action("cargo-test", "Run cargo tests", "cargo", &["test"]));
        actions.push(action("cargo-run", "Run cargo binary", "cargo", &["run"]));
    }
    if build_systems.iter().any(|system| system == "CMake") {
        actions.push(action(
            "cmake-configure",
            "Configure CMake with compile_commands.json",
            "cmake",
            &[
                "-S",
                "{workspace}",
                "-B",
                "{workspace}/build",
                "-DCMAKE_EXPORT_COMPILE_COMMANDS=ON",
            ],
        ));
        actions.push(action(
            "cmake-build",
            "Build CMake project",
            "cmake",
            &["--build", "{workspace}/build"],
        ));
        actions.push(action(
            "ctest",
            "Run CTest from the CMake build directory",
            "ctest",
            &["--test-dir", "{workspace}/build", "--output-on-failure"],
        ));
    } else if build_systems.iter().any(|system| system == "Make") {
        actions.push(action("make-build", "Run make", "make", &[]));
        actions.push(action("make-test", "Run make test", "make", &["test"]));
    }
    if build_systems.iter().any(|system| system == "Go") || languages.iter().any(|language| language == "Go") {
        actions.push(action("go-test", "Run Go tests", "go", &["test", "./..."]));
        actions.push(action("go-run", "Run Go module", "go", &["run", "."]));
    }
    if build_systems.iter().any(|system| system == "Python") || languages.iter().any(|language| language == "Python") {
        actions.push(action("pytest", "Run pytest", "python", &["-m", "pytest"]));
    }
    if build_systems.iter().any(|system| system == "Node")
        || languages.iter().any(|language| language == "JavaScript/TypeScript")
    {
        actions.push(action("npm-test", "Run npm test", "npm", &["test"]));
    }
    actions
}

fn action(name: &str, description: &str, command: &str, args: &[&str]) -> ActionConfig {
    ActionConfig {
        name: name.to_string(),
        description: Some(description.to_string()),
        command: command.to_string(),
        args: args.iter().map(|arg| (*arg).to_string()).collect(),
        cwd: Some("{workspace}".to_string()),
    }
}

fn search_ignores_for_project(project_type: &str) -> Vec<String> {
    let mut ignores = vec![
        ".git/".to_string(),
        ".fcs/".to_string(),
        ".fcs-cache/".to_string(),
        "target/".to_string(),
        "node_modules/".to_string(),
        "*.tmp".to_string(),
        "*.log".to_string(),
    ];
    match project_type {
        "c-cpp" => {
            ignores.push("build/".to_string());
            ignores.push("cmake-build-*/".to_string());
        }
        "python" => {
            ignores.push(".venv/".to_string());
            ignores.push("__pycache__/".to_string());
        }
        "go" => {
            ignores.push("bin/".to_string());
        }
        _ => {}
    }
    ignores.sort();
    ignores.dedup();
    ignores
}

fn default_debug_binary_for_project(project_type: &str) -> String {
    match project_type {
        "rust" => "target/debug/app".to_string(),
        "c-cpp" => "build/app".to_string(),
        "go" => "app".to_string(),
        _ => "".to_string(),
    }
}

fn path_for_config(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn detect_languages(root: &Path) -> Result<Vec<String>> {
    let mut languages = BTreeSet::new();
    let mut scanned = 0usize;
    let walker = ignore::WalkBuilder::new(root)
        .standard_filters(true)
        .hidden(false)
        .max_depth(Some(6))
        .build();

    for entry in walker {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        if !entry.file_type().is_some_and(|file_type| file_type.is_file()) {
            continue;
        }

        scanned += 1;
        if scanned > 5000 {
            break;
        }

        if let Some(language) = language_for_path(entry.path()) {
            languages.insert(language.to_string());
        }
    }

    Ok(languages.into_iter().collect())
}

fn language_for_path(path: &Path) -> Option<&'static str> {
    let extension = path.extension().and_then(|extension| extension.to_str()).unwrap_or("");
    match extension {
        "c" | "cc" | "cpp" | "cxx" | "h" | "hh" | "hpp" | "hxx" => Some("C/C++"),
        "rs" => Some("Rust"),
        "py" => Some("Python"),
        "js" | "jsx" | "ts" | "tsx" => Some("JavaScript/TypeScript"),
        "go" => Some("Go"),
        _ => None,
    }
}

fn detect_debug_targets(root: &Path) -> Vec<PathBuf> {
    let mut targets = Vec::new();
    let project_name = root.file_name().and_then(|name| name.to_str()).unwrap_or("app");
    let candidates = [
        root.join("target").join("debug").join(project_name),
        root.join("build").join(project_name),
        root.join(project_name),
    ];

    for candidate in candidates {
        if candidate.is_file() {
            targets.push(candidate);
        }
    }

    targets
}

fn detect_cache_checks(status: &WorkspaceStatus, project_config: Option<&ProjectConfig>) -> Vec<WorkspaceHealthCheck> {
    let mut checks = Vec::new();
    checks.push(WorkspaceHealthCheck {
        name: "cache-dir".to_string(),
        ok: status.cache_dir.is_dir(),
        detail: status.cache_dir.display().to_string(),
    });
    checks.push(cache_write_check(&status.cache_dir));

    let log_dir = project_config
        .map(|config| config.log_dir.as_str())
        .filter(|log_dir| !log_dir.trim().is_empty())
        .unwrap_or(".fcs/logs");
    let log_path = status.root.join(log_dir);
    checks.push(WorkspaceHealthCheck {
        name: "log-dir".to_string(),
        ok: log_path.is_dir(),
        detail: log_path.display().to_string(),
    });

    let latency_path = status.cache_dir.join("latency-smoke.tsv");
    checks.push(WorkspaceHealthCheck {
        name: "latency-smoke".to_string(),
        ok: latency_path.is_file(),
        detail: latency_path.display().to_string(),
    });
    checks
}

fn cache_write_check(cache_dir: &Path) -> WorkspaceHealthCheck {
    let probe = cache_dir.join(format!(".fcs-health-probe-{}", std::process::id()));
    match fs::write(&probe, b"ok") {
        Ok(()) => {
            let _ = fs::remove_file(&probe);
            WorkspaceHealthCheck {
                name: "cache-write".to_string(),
                ok: true,
                detail: cache_dir.display().to_string(),
            }
        }
        Err(err) => WorkspaceHealthCheck {
            name: "cache-write".to_string(),
            ok: false,
            detail: format!("{}: {err}", cache_dir.display()),
        },
    }
}

fn health_check_action(check: &WorkspaceHealthCheck) -> Option<String> {
    match check.name.as_str() {
        "cache-dir" | "cache-write" => Some("Run: rtk cargo run -- workspace init".to_string()),
        "log-dir" => Some("Run: rtk mkdir -p .fcs/logs".to_string()),
        "latency-smoke" => Some("Run: rtk scripts/smoke.sh".to_string()),
        _ => None,
    }
}

fn workspace_cache_dir(root: &Path) -> Result<PathBuf> {
    let cache_root = crate::cache::workspace_cache_root(root)?;
    Ok(workspace_cache_path(root, &cache_root))
}

fn workspace_cache_path(root: &Path, cache_root: &Path) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    root.hash(&mut hasher);
    let hash = format!("{:016x}", hasher.finish());
    let name = root.file_name().and_then(|value| value.to_str()).unwrap_or("workspace");

    cache_root.join(format!("{name}-{hash}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_workspace_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("fcs_workspace_{name}_{}", std::process::id()))
    }

    #[test]
    fn resolves_workspace_root_from_marker() {
        let temp_dir = temp_workspace_dir("root_marker");
        let nested = temp_dir.join("src").join("module");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&nested).unwrap();
        fs::write(temp_dir.join("compile_commands.json"), "[]").unwrap();

        let dir = nested.to_string_lossy().to_string();
        let root = resolve_root(Some(&dir)).unwrap();
        assert_eq!(root, temp_dir);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn writes_and_reads_project_config() {
        let temp_dir = temp_workspace_dir("project_config");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("src")).unwrap();
        fs::write(temp_dir.join("Cargo.toml"), "[package]\nname = \"fixture\"\n").unwrap();
        fs::write(temp_dir.join("src").join("main.rs"), "fn main() {}\n").unwrap();
        let dir = temp_dir.to_string_lossy().to_string();

        let path = write_project_config(Some(&dir), false).unwrap();
        let config = read_project_config(&temp_dir).unwrap().unwrap();

        assert_eq!(path, temp_dir.join(".fcs.toml"));
        assert_eq!(config.clangd_command, "clangd");
        assert_eq!(config.project_type, "rust");
        assert!(config.languages.iter().any(|language| language == "Rust"));
        assert!(config.build_systems.iter().any(|system| system == "Cargo"));
        assert!(config.index_roots.iter().any(|root| root == "src"));
        assert!(config.search_ignore.iter().any(|pattern| pattern == "target/"));
        assert!(config.actions.iter().any(|action| action.name == "cargo-test"));
        assert!(write_project_config(Some(&dir), false).is_err());
        assert_eq!(write_project_config(Some(&dir), true).unwrap(), path);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn workspace_cache_path_is_stable_and_namespaced() {
        let root = PathBuf::from("/tmp/fcs-project");
        let cache_root = PathBuf::from("/tmp/fcs-cache/workspaces");

        let first = workspace_cache_path(&root, &cache_root);
        let second = workspace_cache_path(&root, &cache_root);

        assert_eq!(first, second);
        assert!(first.starts_with(&cache_root));
        assert!(first.file_name().unwrap().to_string_lossy().starts_with("fcs-project-"));
    }

    #[test]
    fn advice_detects_cmake_compile_database_gap() {
        let temp_dir = temp_workspace_dir("advice_cmake");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("src")).unwrap();
        fs::write(
            temp_dir.join("CMakeLists.txt"),
            "cmake_minimum_required(VERSION 3.20)\n",
        )
        .unwrap();
        fs::write(temp_dir.join("src").join("main.c"), "int main(void) { return 0; }\n").unwrap();
        let dir = temp_dir.to_string_lossy().to_string();

        let report = advise_with_lsp_commands(
            Some(&dir),
            "definitely-missing-clangd",
            "definitely-missing-rust-analyzer",
        )
        .unwrap();
        let messages = report
            .advice
            .iter()
            .map(|advice| advice.message.as_str())
            .collect::<Vec<&str>>();

        assert!(report.build_systems.iter().any(|system| system == "CMake"));
        assert!(report.languages.iter().any(|language| language == "C/C++"));
        assert!(messages
            .iter()
            .any(|message| message.contains("clangd command is not available")));
        assert!(messages.iter().any(|message| message.contains("compile database")));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn advice_detects_cargo_project() {
        let temp_dir = temp_workspace_dir("advice_cargo");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("src")).unwrap();
        fs::write(temp_dir.join("Cargo.toml"), "[package]\nname = \"fixture\"\n").unwrap();
        fs::write(temp_dir.join("src").join("main.rs"), "fn main() {}\n").unwrap();
        let dir = temp_dir.to_string_lossy().to_string();

        let report = advise_with_lsp_commands(
            Some(&dir),
            "definitely-missing-clangd",
            "definitely-missing-rust-analyzer",
        )
        .unwrap();
        let messages = report
            .advice
            .iter()
            .map(|advice| advice.message.as_str())
            .collect::<Vec<&str>>();

        assert!(report.build_systems.iter().any(|system| system == "Cargo"));
        assert!(report.languages.iter().any(|language| language == "Rust"));
        assert!(messages
            .iter()
            .any(|message| message.contains("rust-analyzer command is not available")));
        assert!(!messages
            .iter()
            .any(|message| message.contains("clangd command is not available")));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn detects_cmake_project_and_generates_actions() {
        let temp_dir = temp_workspace_dir("detect_cmake");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("src")).unwrap();
        fs::write(
            temp_dir.join("CMakeLists.txt"),
            "cmake_minimum_required(VERSION 3.20)\n",
        )
        .unwrap();
        fs::write(temp_dir.join("src").join("main.cpp"), "int main() { return 0; }\n").unwrap();

        let detection = detect_project(&temp_dir).unwrap();
        let config = ProjectConfig::for_workspace(&temp_dir).unwrap();

        assert_eq!(detection.project_type, "c-cpp");
        assert!(detection.build_systems.iter().any(|system| system == "CMake"));
        assert!(detection.languages.iter().any(|language| language == "C/C++"));
        assert!(config.actions.iter().any(|action| action.name == "cmake-configure"));
        assert!(config.actions.iter().any(|action| action.name == "cmake-build"));
        assert!(config.search_ignore.iter().any(|pattern| pattern == "build/"));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn cache_checks_report_log_and_latency_gaps() {
        let temp_dir = temp_workspace_dir("health_checks");
        let cache_dir = temp_workspace_dir("health_cache");
        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::remove_dir_all(&cache_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        fs::create_dir_all(&cache_dir).unwrap();
        let status = WorkspaceStatus {
            root: temp_dir.clone(),
            cache_dir: cache_dir.clone(),
            has_compile_commands: false,
            has_compile_flags: false,
            clangd_available: false,
            clangd_version: None,
            rust_analyzer_available: false,
            rust_analyzer_version: None,
            has_cargo_toml: false,
        };

        let checks = detect_cache_checks(&status, None);

        assert!(checks.iter().any(|check| check.name == "cache-write" && check.ok));
        assert!(checks.iter().any(|check| check.name == "log-dir" && !check.ok));
        assert!(checks.iter().any(|check| check.name == "latency-smoke" && !check.ok));

        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::remove_dir_all(&cache_dir);
    }
}
