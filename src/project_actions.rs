use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::config::{ActionConfig, Config};
use crate::errors::{AppError, Result};
use crate::workspace;

const ALLOWED_TEMPLATE_VARIABLES: &[&str] = &["workspace", "file", "line", "symbol"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedAction {
    pub name: String,
    pub description: Option<String>,
    pub source: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionContext {
    pub workspace: PathBuf,
    pub file: Option<String>,
    pub line: Option<usize>,
    pub symbol: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandedAction {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionTemplate {
    pub name: String,
    pub description: String,
    pub actions: Vec<ActionConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionDiagnostic {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

pub fn list_actions(config: &Config, directory: Option<&String>) -> Result<Vec<ResolvedAction>> {
    let root = workspace::resolve_root(directory)?;
    let mut actions = BTreeMap::<String, ResolvedAction>::new();

    insert_actions(&mut actions, "global", &config.actions)?;
    if let Some(project_config) = workspace::read_project_config(&root)? {
        insert_actions(&mut actions, "project", &project_config.actions)?;
    }

    Ok(actions.into_values().collect())
}

pub fn builtin_templates() -> Vec<ActionTemplate> {
    vec![
        action_template(
            "rust-cargo-test",
            "Cargo check/test/run actions for Rust workspaces",
            vec![
                action("cargo-check", "Run cargo check", "cargo", &["check"]),
                action("cargo-test", "Run cargo tests", "cargo", &["test"]),
                action("cargo-run", "Run cargo binary", "cargo", &["run"]),
            ],
        ),
        action_template(
            "cpp-cmake-test",
            "CMake configure/build/test actions for C/C++ workspaces",
            vec![
                action(
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
                ),
                action(
                    "cmake-build",
                    "Build CMake project",
                    "cmake",
                    &["--build", "{workspace}/build"],
                ),
                action(
                    "ctest",
                    "Run CTest from the CMake build directory",
                    "ctest",
                    &["--test-dir", "{workspace}/build", "--output-on-failure"],
                ),
            ],
        ),
        action_template(
            "make-test",
            "Make build/test actions",
            vec![
                action("make-build", "Run make", "make", &[]),
                action("make-test", "Run make test", "make", &["test"]),
            ],
        ),
        action_template(
            "pytest",
            "Python pytest action",
            vec![action("pytest", "Run pytest", "python", &["-m", "pytest"])],
        ),
        action_template(
            "npm-test",
            "Node npm test action",
            vec![action("npm-test", "Run npm test", "npm", &["test"])],
        ),
    ]
}

pub fn template_config_toml(directory: Option<&String>, template_name: &str) -> Result<String> {
    let root = workspace::resolve_root(directory)?;
    let template = find_template(template_name)?;
    let mut project_config = workspace::ProjectConfig::for_workspace(&root)?;
    project_config.actions = template.actions;
    toml::to_string_pretty(&project_config).map_err(|err| AppError::General(err.to_string()))
}

pub fn write_template_config(directory: Option<&String>, template_name: &str, force: bool) -> Result<PathBuf> {
    let root = workspace::resolve_root(directory)?;
    let path = root.join(".fcs.toml");
    if path.exists() && !force {
        return Err(AppError::General(format!(
            "Project config already exists: {}",
            path.display()
        )));
    }

    let contents = template_config_toml(directory, template_name)?;
    std::fs::write(&path, contents)?;
    Ok(path)
}

pub fn doctor_actions(config: &Config, directory: Option<&String>) -> Result<Vec<ActionDiagnostic>> {
    let root = workspace::resolve_root(directory)?;
    let actions = list_actions(config, directory)?;
    if actions.is_empty() {
        return Ok(vec![ActionDiagnostic {
            name: "actions".to_string(),
            ok: false,
            detail: "no global or project actions configured".to_string(),
        }]);
    }

    let mut diagnostics = Vec::new();
    for action in actions {
        diagnostics.extend(diagnostics_for_action(&root, &action));
    }
    Ok(diagnostics)
}

pub fn expand_action(
    config: &Config,
    directory: Option<&String>,
    name: &str,
    file: Option<&String>,
    line: Option<usize>,
    symbol: Option<&String>,
    extra_args: &[String],
) -> Result<ExpandedAction> {
    let root = workspace::resolve_root(directory)?;
    let action = list_actions(config, directory)?
        .into_iter()
        .find(|action| action.name == name)
        .ok_or_else(|| AppError::General(format!("Project action not found: {name}")))?;
    let context = ActionContext {
        workspace: root.clone(),
        file: file.cloned(),
        line,
        symbol: symbol.cloned(),
    };
    Ok(expand_resolved_action(&action, &context, extra_args))
}

pub fn run_expanded_action(action: &ExpandedAction) -> Result<i32> {
    let status = Command::new(&action.command)
        .args(&action.args)
        .current_dir(&action.cwd)
        .status()?;
    let code = status.code().unwrap_or(1);
    if !status.success() {
        return Err(AppError::General(format!(
            "Project action failed with status {code}: {}",
            format_command_line(action)
        )));
    }
    Ok(code)
}

pub fn format_action(action: &ResolvedAction) -> String {
    let description = action.description.as_deref().unwrap_or("-");
    format!("{} [{}] {}", action.name, action.source, description)
}

pub fn format_template(template: &ActionTemplate) -> String {
    let action_names = template
        .actions
        .iter()
        .map(|action| action.name.as_str())
        .collect::<Vec<&str>>()
        .join(",");
    format!("{} - {} [{}]", template.name, template.description, action_names)
}

pub fn format_command_line(action: &ExpandedAction) -> String {
    let mut parts = vec![action.command.clone()];
    parts.extend(action.args.iter().cloned());
    format!("cwd={} {}", action.cwd.display(), parts.join(" "))
}

fn insert_actions(target: &mut BTreeMap<String, ResolvedAction>, source: &str, actions: &[ActionConfig]) -> Result<()> {
    for action in actions {
        validate_action(action)?;
        target.insert(
            action.name.clone(),
            ResolvedAction {
                name: action.name.clone(),
                description: action.description.clone(),
                source: source.to_string(),
                command: action.command.clone(),
                args: action.args.clone(),
                cwd: action.cwd.clone(),
            },
        );
    }
    Ok(())
}

fn validate_action(action: &ActionConfig) -> Result<()> {
    if action.name.trim().is_empty() {
        return Err(AppError::General("Project action has an empty name".to_string()));
    }
    if action.command.trim().is_empty() {
        return Err(AppError::General(format!(
            "Project action has an empty command: {}",
            action.name
        )));
    }
    Ok(())
}

fn action_template(name: &str, description: &str, actions: Vec<ActionConfig>) -> ActionTemplate {
    ActionTemplate {
        name: name.to_string(),
        description: description.to_string(),
        actions,
    }
}

fn find_template(name: &str) -> Result<ActionTemplate> {
    builtin_templates()
        .into_iter()
        .find(|template| template.name == name)
        .ok_or_else(|| AppError::General(format!("Action template not found: {name}")))
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

fn diagnostics_for_action(root: &Path, action: &ResolvedAction) -> Vec<ActionDiagnostic> {
    let mut diagnostics = Vec::new();
    for (field, value) in action_template_fields(action) {
        let unknown = unknown_template_variables(value);
        if !unknown.is_empty() {
            diagnostics.push(ActionDiagnostic {
                name: action.name.clone(),
                ok: false,
                detail: format!("{field} uses unknown template variable(s): {}", unknown.join(", ")),
            });
        }
    }

    let context = ActionContext {
        workspace: root.to_path_buf(),
        file: Some("src/main.rs".to_string()),
        line: Some(1),
        symbol: Some("main".to_string()),
    };
    let expanded = expand_resolved_action(action, &context, &[]);
    diagnostics.push(ActionDiagnostic {
        name: action.name.clone(),
        ok: expanded.cwd.exists(),
        detail: format!("cwd={} command={}", expanded.cwd.display(), expanded.command),
    });
    diagnostics
}

fn action_template_fields(action: &ResolvedAction) -> Vec<(&'static str, &str)> {
    let mut fields = vec![("command", action.command.as_str())];
    fields.extend(action.args.iter().map(|arg| ("args", arg.as_str())));
    if let Some(cwd) = action.cwd.as_deref() {
        fields.push(("cwd", cwd));
    }
    fields
}

fn unknown_template_variables(value: &str) -> Vec<String> {
    let mut unknown = Vec::new();
    let mut rest = value;
    while let Some(start) = rest.find('{') {
        let after_start = &rest[start + 1..];
        let Some(end) = after_start.find('}') else {
            unknown.push("unclosed".to_string());
            break;
        };
        let name = &after_start[..end];
        if !ALLOWED_TEMPLATE_VARIABLES.contains(&name) {
            unknown.push(name.to_string());
        }
        rest = &after_start[end + 1..];
    }
    unknown.sort();
    unknown.dedup();
    unknown
}

fn expand_resolved_action(action: &ResolvedAction, context: &ActionContext, extra_args: &[String]) -> ExpandedAction {
    let command = expand_template(&action.command, context);
    let mut args = action
        .args
        .iter()
        .map(|arg| expand_template(arg, context))
        .collect::<Vec<_>>();
    args.extend(extra_args.iter().map(|arg| expand_template(arg, context)));
    let cwd = action
        .cwd
        .as_deref()
        .map(|cwd| expand_template(cwd, context))
        .map(PathBuf::from)
        .unwrap_or_else(|| context.workspace.clone());

    ExpandedAction {
        name: action.name.clone(),
        command,
        args,
        cwd: absolutize_cwd(&context.workspace, &cwd),
    }
}

fn expand_template(template: &str, context: &ActionContext) -> String {
    template
        .replace("{workspace}", &context.workspace.to_string_lossy())
        .replace("{file}", context.file.as_deref().unwrap_or(""))
        .replace("{line}", &context.line.map(|line| line.to_string()).unwrap_or_default())
        .replace("{symbol}", context.symbol.as_deref().unwrap_or(""))
}

fn absolutize_cwd(workspace: &Path, cwd: &Path) -> PathBuf {
    if cwd.is_absolute() {
        cwd.to_path_buf()
    } else {
        workspace.join(cwd)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_action_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("fcs_actions_{name}_{}", std::process::id()))
    }

    #[test]
    fn expands_action_variables_and_extra_args() {
        let workspace = PathBuf::from("/tmp/project");
        let action = ResolvedAction {
            name: "test".to_string(),
            description: None,
            source: "global".to_string(),
            command: "cargo".to_string(),
            args: vec![
                "test".to_string(),
                "{symbol}".to_string(),
                "--manifest-path".to_string(),
                "{workspace}/Cargo.toml".to_string(),
            ],
            cwd: Some("{workspace}".to_string()),
        };
        let context = ActionContext {
            workspace: workspace.clone(),
            file: Some("src/lib.rs".to_string()),
            line: Some(42),
            symbol: Some("parse".to_string()),
        };

        let expanded = expand_resolved_action(&action, &context, &["--exact".to_string(), "{file}:{line}".to_string()]);

        assert_eq!(expanded.command, "cargo");
        assert_eq!(expanded.cwd, workspace);
        assert_eq!(expanded.args[1], "parse");
        assert_eq!(expanded.args[3], "/tmp/project/Cargo.toml");
        assert_eq!(expanded.args[5], "src/lib.rs:42");
    }

    #[test]
    fn project_actions_override_global_actions() {
        let root = temp_action_dir("override");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        std::fs::write(
            root.join(".fcs.toml"),
            r#"
[[actions]]
name = "test"
description = "project test"
command = "cargo"
args = ["test"]
"#,
        )
        .unwrap();
        let mut config = Config::default();
        config.actions.push(ActionConfig {
            name: "test".to_string(),
            description: Some("global test".to_string()),
            command: "echo".to_string(),
            args: Vec::new(),
            cwd: None,
        });

        let root_arg = root.to_string_lossy().to_string();
        let actions = list_actions(&config, Some(&root_arg)).unwrap();

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].source, "project");
        assert_eq!(actions[0].description.as_deref(), Some("project test"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn builtin_templates_generate_project_config() {
        let root = temp_action_dir("template_config");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        std::fs::write(root.join("src").join("main.rs"), "fn main() {}\n").unwrap();
        let root_arg = root.to_string_lossy().to_string();

        let templates = builtin_templates();
        let contents = template_config_toml(Some(&root_arg), "rust-cargo-test").unwrap();

        assert!(templates.iter().any(|template| template.name == "rust-cargo-test"));
        assert!(contents.contains("cargo-test"));
        assert!(contents.contains("cargo-check"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn doctor_reports_unknown_template_variables() {
        let root = temp_action_dir("doctor_unknown");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let root_arg = root.to_string_lossy().to_string();
        let mut config = Config::default();
        config.actions.push(ActionConfig {
            name: "bad".to_string(),
            description: None,
            command: "echo".to_string(),
            args: vec!["{unknown}".to_string()],
            cwd: Some("{workspace}".to_string()),
        });

        let diagnostics = doctor_actions(&config, Some(&root_arg)).unwrap();

        assert!(diagnostics
            .iter()
            .any(|diagnostic| !diagnostic.ok && diagnostic.detail.contains("unknown")));

        let _ = std::fs::remove_dir_all(&root);
    }
}
