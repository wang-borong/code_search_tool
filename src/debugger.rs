use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::core::Location;
use crate::errors::{AppError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DebuggerKind {
    Gdb,
    Lldb,
}

impl DebuggerKind {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "gdb" => Ok(Self::Gdb),
            "lldb" => Ok(Self::Lldb),
            _ => Err(AppError::General(format!("Unsupported debugger: {value}"))),
        }
    }

    fn program(self) -> &'static str {
        match self {
            Self::Gdb => "gdb",
            Self::Lldb => "lldb",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugBreakpoint {
    pub path: PathBuf,
    pub line: Option<usize>,
    pub column: Option<usize>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl DebugBreakpoint {
    fn to_location(&self) -> Location {
        Location::new(&self.path, self.line, self.column)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DebugEnvVar {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugProfile {
    pub name: String,
    pub debugger: DebuggerKind,
    pub binary: PathBuf,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub env: Vec<DebugEnvVar>,
    pub args: Vec<String>,
    pub breakpoints: Vec<DebugBreakpoint>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct DebugProfileStore {
    profiles: Vec<DebugProfile>,
}

#[derive(Debug, Clone)]
pub struct DebugSession {
    pub debugger: DebuggerKind,
    pub binary: PathBuf,
    pub cwd: Option<PathBuf>,
    pub env: Vec<DebugEnvVar>,
    pub breakpoints: Vec<Location>,
    pub args: Vec<String>,
}

impl DebugSession {
    pub fn from_profile(profile: &DebugProfile) -> Self {
        Self {
            debugger: profile.debugger,
            binary: profile.binary.clone(),
            cwd: profile.cwd.clone(),
            env: profile.env.clone(),
            breakpoints: profile
                .breakpoints
                .iter()
                .filter(|breakpoint| breakpoint.enabled)
                .map(DebugBreakpoint::to_location)
                .collect(),
            args: profile.args.clone(),
        }
    }

    pub fn to_profile(&self, name: &str) -> DebugProfile {
        DebugProfile {
            name: name.to_string(),
            debugger: self.debugger,
            binary: self.binary.clone(),
            cwd: self.cwd.clone(),
            env: self.env.clone(),
            args: self.args.clone(),
            breakpoints: self
                .breakpoints
                .iter()
                .map(|location| DebugBreakpoint {
                    path: location.path.clone(),
                    line: location.line,
                    column: location.column,
                    enabled: true,
                })
                .collect(),
        }
    }

    pub fn command_preview(&self) -> String {
        let mut parts = Vec::new();
        if let Some(cwd) = &self.cwd {
            parts.push("cd".to_string());
            parts.push(shell_quote(&cwd.to_string_lossy()));
            parts.push("&&".to_string());
        }
        for env in &self.env {
            parts.push(format!("{}={}", env.name, shell_quote(&env.value)));
        }
        parts.push(self.debugger.program().to_string());

        match self.debugger {
            DebuggerKind::Gdb => {
                parts.push("--quiet".to_string());
                for breakpoint in &self.breakpoints {
                    parts.push("--ex".to_string());
                    parts.push(shell_quote(&format_breakpoint_gdb(breakpoint)));
                }
                parts.push("--args".to_string());
                parts.push(shell_quote(&self.binary.to_string_lossy()));
                parts.extend(self.args.iter().map(|arg| shell_quote(arg)));
            }
            DebuggerKind::Lldb => {
                for breakpoint in &self.breakpoints {
                    parts.push("--one-line".to_string());
                    parts.push(shell_quote(&format_breakpoint_lldb(breakpoint)));
                }
                parts.push("--".to_string());
                parts.push(shell_quote(&self.binary.to_string_lossy()));
                parts.extend(self.args.iter().map(|arg| shell_quote(arg)));
            }
        }

        parts.join(" ")
    }

    pub fn run(&self) -> Result<()> {
        let status = match self.debugger {
            DebuggerKind::Gdb => self.run_gdb()?,
            DebuggerKind::Lldb => self.run_lldb()?,
        };

        if !status.success() {
            return Err(AppError::General(format!(
                "Debugger exited with status {}",
                status
                    .code()
                    .map_or_else(|| "unknown".to_string(), |code| code.to_string())
            )));
        }

        Ok(())
    }

    fn run_gdb(&self) -> Result<std::process::ExitStatus> {
        let mut command = Command::new("gdb");
        self.configure_command(&mut command);
        command.arg("--quiet");
        for breakpoint in &self.breakpoints {
            command.arg("--ex").arg(format_breakpoint_gdb(breakpoint));
        }
        command.arg("--args").arg(&self.binary).args(&self.args);
        command.status().map_err(AppError::Io)
    }

    fn run_lldb(&self) -> Result<std::process::ExitStatus> {
        let mut command = Command::new("lldb");
        self.configure_command(&mut command);
        for breakpoint in &self.breakpoints {
            command.arg("--one-line").arg(format_breakpoint_lldb(breakpoint));
        }
        command.arg("--").arg(&self.binary).args(&self.args);
        command.status().map_err(AppError::Io)
    }

    fn configure_command(&self, command: &mut Command) {
        if let Some(cwd) = &self.cwd {
            command.current_dir(cwd);
        }
        for env in &self.env {
            command.env(&env.name, &env.value);
        }
    }
}

pub fn save_profile(root: &Path, profile: DebugProfile) -> Result<()> {
    let path = profile_path(root)?;
    save_profile_to_path(&path, profile)
}

pub fn list_profiles(root: &Path) -> Result<Vec<DebugProfile>> {
    let path = profile_path(root)?;
    list_profiles_from_path(&path)
}

pub fn load_profile(root: &Path, name: &str) -> Result<DebugProfile> {
    let path = profile_path(root)?;
    load_profile_from_path(&path, name)
}

pub fn delete_profile(root: &Path, name: &str) -> Result<bool> {
    let path = profile_path(root)?;
    delete_profile_from_path(&path, name)
}

pub fn set_breakpoint_enabled(root: &Path, profile_name: &str, index: usize, enabled: bool) -> Result<()> {
    if index == 0 {
        return Err(AppError::General("Breakpoint index starts at 1".to_string()));
    }

    let path = profile_path(root)?;
    let mut store = load_store_from_path(&path)?;
    let profile = store
        .profiles
        .iter_mut()
        .find(|profile| profile.name == profile_name)
        .ok_or_else(|| AppError::General(format!("Debug profile not found: {profile_name}")))?;
    let breakpoint = profile
        .breakpoints
        .get_mut(index - 1)
        .ok_or_else(|| AppError::General(format!("Breakpoint index out of range: {index}")))?;
    breakpoint.enabled = enabled;
    save_store_to_path(&path, &store)
}

pub fn parse_env_var(value: &str) -> Result<DebugEnvVar> {
    let Some((name, raw_value)) = value.split_once('=') else {
        return Err(AppError::General(format!("Invalid environment assignment: {value}")));
    };
    if name.is_empty() {
        return Err(AppError::General("Environment variable name is empty".to_string()));
    }

    Ok(DebugEnvVar {
        name: name.to_string(),
        value: raw_value.to_string(),
    })
}

fn load_store_from_path(path: &Path) -> Result<DebugProfileStore> {
    if !path.exists() {
        return Ok(DebugProfileStore::default());
    }

    let contents = fs::read_to_string(path)?;
    toml::from_str(&contents).map_err(|e| AppError::General(e.to_string()))
}

fn save_store_to_path(path: &Path, store: &DebugProfileStore) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let contents = toml::to_string_pretty(store).map_err(|e| AppError::General(e.to_string()))?;
    fs::write(path, contents)?;
    Ok(())
}

fn save_profile_to_path(path: &Path, profile: DebugProfile) -> Result<()> {
    let mut store = load_store_from_path(path)?;
    store.profiles.retain(|existing| existing.name != profile.name);
    store.profiles.push(profile);
    save_store_to_path(path, &store)
}

fn list_profiles_from_path(path: &Path) -> Result<Vec<DebugProfile>> {
    Ok(load_store_from_path(path)?.profiles)
}

fn load_profile_from_path(path: &Path, name: &str) -> Result<DebugProfile> {
    load_store_from_path(path)?
        .profiles
        .into_iter()
        .find(|profile| profile.name == name)
        .ok_or_else(|| AppError::General(format!("Debug profile not found: {name}")))
}

fn delete_profile_from_path(path: &Path, name: &str) -> Result<bool> {
    let mut store = load_store_from_path(path)?;
    let before = store.profiles.len();
    store.profiles.retain(|profile| profile.name != name);
    let deleted = store.profiles.len() != before;
    if deleted {
        save_store_to_path(path, &store)?;
    }
    Ok(deleted)
}

fn profile_path(root: &Path) -> Result<PathBuf> {
    Ok(crate::workspace::cache_dir_for_root(root)?.join("debug_profiles.toml"))
}

fn format_breakpoint_gdb(location: &Location) -> String {
    format!("break {}:{}", location.path.display(), location.line.unwrap_or(1))
}

fn format_breakpoint_lldb(location: &Location) -> String {
    format!(
        "breakpoint set --file {} --line {}",
        location.path.display(),
        location.line.unwrap_or(1)
    )
}

fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '-' | '_' | ':'))
    {
        return value.to_string();
    }

    format!("'{}'", value.replace('\'', "'\\''"))
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_profile_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("fcs_debug_{name}_{}", std::process::id()))
            .join("debug_profiles.toml")
    }

    fn profile(name: &str, args: Vec<String>) -> DebugProfile {
        DebugProfile {
            name: name.to_string(),
            debugger: DebuggerKind::Gdb,
            binary: PathBuf::from("target/debug/app"),
            cwd: Some(PathBuf::from("/tmp/project")),
            env: vec![DebugEnvVar {
                name: "RUST_LOG".to_string(),
                value: "debug".to_string(),
            }],
            args,
            breakpoints: vec![DebugBreakpoint {
                path: PathBuf::from("src/main.c"),
                line: Some(42),
                column: None,
                enabled: true,
            }],
        }
    }

    #[test]
    fn builds_gdb_command_preview() {
        let session = DebugSession {
            debugger: DebuggerKind::Gdb,
            binary: PathBuf::from("target/debug/app"),
            cwd: Some(PathBuf::from("/tmp/project")),
            env: vec![DebugEnvVar {
                name: "RUST_LOG".to_string(),
                value: "debug".to_string(),
            }],
            breakpoints: vec![Location::new("src/main.c", Some(42), None)],
            args: vec!["--flag".to_string()],
        };

        let command = session.command_preview();
        assert!(command.contains("cd /tmp/project && RUST_LOG=debug gdb --quiet"));
        assert!(command.contains("break src/main.c:42"));
        assert!(command.contains("--args target/debug/app --flag"));
    }

    #[test]
    fn profile_filters_disabled_breakpoints() {
        let profile = DebugProfile {
            name: "smoke".to_string(),
            debugger: DebuggerKind::Gdb,
            binary: PathBuf::from("target/debug/app"),
            cwd: None,
            env: Vec::new(),
            args: Vec::new(),
            breakpoints: vec![
                DebugBreakpoint {
                    path: PathBuf::from("src/main.c"),
                    line: Some(10),
                    column: None,
                    enabled: true,
                },
                DebugBreakpoint {
                    path: PathBuf::from("src/main.c"),
                    line: Some(20),
                    column: None,
                    enabled: false,
                },
            ],
        };

        let session = DebugSession::from_profile(&profile);

        assert_eq!(session.breakpoints.len(), 1);
        assert_eq!(session.breakpoints[0].line, Some(10));
    }

    #[test]
    fn old_profiles_default_breakpoints_to_enabled() {
        let contents = r#"
[[profiles]]
name = "legacy"
debugger = "Gdb"
binary = "target/debug/app"
args = []

[[profiles.breakpoints]]
path = "src/main.c"
line = 12
"#;

        let store: DebugProfileStore = toml::from_str(contents).unwrap();

        assert_eq!(store.profiles[0].cwd, None);
        assert!(store.profiles[0].env.is_empty());
        assert!(store.profiles[0].breakpoints[0].enabled);
    }

    #[test]
    fn parses_environment_assignment() {
        let env = parse_env_var("KEY=value=with-equals").unwrap();

        assert_eq!(env.name, "KEY");
        assert_eq!(env.value, "value=with-equals");
        assert!(parse_env_var("missing-equals").is_err());
    }

    #[test]
    fn persists_profiles_and_replaces_profiles_by_name() {
        let path = temp_profile_path("replace");
        let _ = fs::remove_file(&path);

        save_profile_to_path(&path, profile("smoke", vec!["--old".to_string()])).unwrap();
        save_profile_to_path(
            &path,
            profile("smoke", vec!["--new".to_string(), "input.txt".to_string()]),
        )
        .unwrap();

        let profiles = list_profiles_from_path(&path).unwrap();
        let loaded = load_profile_from_path(&path, "smoke").unwrap();

        assert_eq!(profiles.len(), 1);
        assert_eq!(loaded.args, vec!["--new".to_string(), "input.txt".to_string()]);
        assert_eq!(loaded.env[0].name, "RUST_LOG");
        assert!(load_profile_from_path(&path, "missing").is_err());

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn persists_breakpoint_toggle_and_profile_delete() {
        let path = temp_profile_path("toggle_delete");
        let _ = fs::remove_file(&path);
        save_profile_to_path(&path, profile("smoke", Vec::new())).unwrap();

        let mut store = load_store_from_path(&path).unwrap();
        store.profiles[0].breakpoints[0].enabled = false;
        save_store_to_path(&path, &store).unwrap();

        let loaded = load_profile_from_path(&path, "smoke").unwrap();
        assert!(!loaded.breakpoints[0].enabled);
        assert!(delete_profile_from_path(&path, "smoke").unwrap());
        assert!(!delete_profile_from_path(&path, "smoke").unwrap());
        assert!(list_profiles_from_path(&path).unwrap().is_empty());

        let _ = fs::remove_file(&path);
    }
}
