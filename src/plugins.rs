use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::config::ActionConfig;
use crate::errors::{AppError, Result};

const BUILTIN_PLUGIN: &str = r#"
[plugin]
name = "builtin-dev"
version = "1.0.0"
description = "Built-in development commands and project action templates"

[[commands]]
name = "cargo-check"
description = "Run cargo check in the workspace"
command = "cargo"
args = ["check"]
cwd = "{workspace}"

[[commands]]
name = "cargo-test"
description = "Run cargo test in the workspace"
command = "cargo"
args = ["test"]
cwd = "{workspace}"

[[templates]]
name = "rust-debug"
description = "Rust check/test/run actions"

[[templates.actions]]
name = "cargo-check"
description = "Run cargo check"
command = "cargo"
args = ["check"]
cwd = "{workspace}"

[[templates.actions]]
name = "cargo-test"
description = "Run cargo tests"
command = "cargo"
args = ["test"]
cwd = "{workspace}"
"#;

const TEMPLATE_VARIABLES: &[&str] = &["workspace", "file", "line", "symbol"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifest {
    pub plugin: PluginInfo,
    #[serde(default)]
    pub commands: Vec<PluginCommand>,
    #[serde(default)]
    pub templates: Vec<PluginTemplate>,
    #[serde(skip)]
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginCommand {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub pre: Vec<PluginHookCommand>,
    #[serde(default)]
    pub post: Vec<PluginHookCommand>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginHookCommand {
    #[serde(default)]
    pub name: Option<String>,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginTemplate {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub actions: Vec<ActionConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginDiagnostic {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandedPluginCommand {
    pub plugin: String,
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: BTreeMap<String, String>,
    pub pre_hooks: Vec<ExpandedPluginStep>,
    pub post_hooks: Vec<ExpandedPluginStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandedPluginStep {
    pub phase: String,
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginTemplateInitReport {
    pub path: PathBuf,
    pub template: String,
    pub action_count: usize,
    pub dry_run: bool,
    pub contents: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PluginContext {
    workspace: PathBuf,
    file: Option<String>,
    line: Option<usize>,
    symbol: Option<String>,
    vars: BTreeMap<String, String>,
}

pub fn discover(root: Option<&Path>) -> Result<Vec<PluginManifest>> {
    let mut manifests = vec![parse_manifest(BUILTIN_PLUGIN, "builtin")?];
    for path in manifest_paths(root) {
        let contents = fs::read_to_string(&path)?;
        manifests.push(parse_manifest(&contents, &path.display().to_string())?);
    }
    Ok(manifests)
}

pub fn doctor(root: Option<&Path>) -> Result<Vec<PluginDiagnostic>> {
    let manifests = discover(root)?;
    let mut diagnostics = Vec::new();
    let mut names = BTreeSet::new();

    for manifest in &manifests {
        let plugin_name = manifest.plugin.name.clone();
        let unique = names.insert(plugin_name.clone());
        diagnostics.push(PluginDiagnostic {
            name: plugin_name.clone(),
            ok: unique,
            detail: if unique {
                format!("source={}", manifest.source)
            } else {
                "duplicate plugin name".to_string()
            },
        });

        if manifest.commands.is_empty() && manifest.templates.is_empty() {
            diagnostics.push(PluginDiagnostic {
                name: plugin_name.clone(),
                ok: false,
                detail: "plugin has no commands or templates".to_string(),
            });
        }

        for command in &manifest.commands {
            diagnostics.extend(diagnostics_for_command(&plugin_name, command));
        }
        for template in &manifest.templates {
            diagnostics.extend(diagnostics_for_template(&plugin_name, template));
        }
    }

    Ok(diagnostics)
}

pub fn find_manifest(root: Option<&Path>, plugin_name: &str) -> Result<PluginManifest> {
    discover(root)?
        .into_iter()
        .find(|manifest| manifest.plugin.name == plugin_name)
        .ok_or_else(|| AppError::General(format!("Plugin not found: {plugin_name}")))
}

pub fn expand_command(
    root: &Path,
    selector: &str,
    file: Option<&String>,
    line: Option<usize>,
    symbol: Option<&String>,
    extra_args: &[String],
) -> Result<ExpandedPluginCommand> {
    expand_command_with_vars(root, selector, file, line, symbol, extra_args, &[])
}

pub fn expand_command_with_vars(
    root: &Path,
    selector: &str,
    file: Option<&String>,
    line: Option<usize>,
    symbol: Option<&String>,
    extra_args: &[String],
    vars: &[String],
) -> Result<ExpandedPluginCommand> {
    let (manifest, command) = find_command(Some(root), selector)?;
    let context = PluginContext {
        workspace: root.to_path_buf(),
        file: file.cloned(),
        line,
        symbol: symbol.cloned(),
        vars: parse_vars(vars)?,
    };
    Ok(expand_plugin_command(
        &manifest.plugin.name,
        &command,
        &context,
        extra_args,
    ))
}

pub fn run_expanded_command(command: &ExpandedPluginCommand) -> Result<i32> {
    for step in execution_steps(command) {
        run_expanded_step(&step)?;
    }
    Ok(0)
}

pub fn execution_steps(command: &ExpandedPluginCommand) -> Vec<ExpandedPluginStep> {
    let mut steps = Vec::new();
    steps.extend(command.pre_hooks.iter().cloned());
    steps.push(ExpandedPluginStep {
        phase: "main".to_string(),
        name: command.name.clone(),
        command: command.command.clone(),
        args: command.args.clone(),
        cwd: command.cwd.clone(),
        env: command.env.clone(),
    });
    steps.extend(command.post_hooks.iter().cloned());
    steps
}

pub fn format_execution_plan(command: &ExpandedPluginCommand) -> String {
    execution_steps(command)
        .iter()
        .map(format_expanded_step)
        .collect::<Vec<String>>()
        .join("\n")
}

pub fn manifest_schema(format: &str) -> Result<String> {
    match format {
        "toml" | "text" => Ok(plugin_schema_toml()),
        "json" => serde_json::to_string_pretty(
            &toml::from_str::<toml::Value>(&plugin_schema_toml()).map_err(|err| AppError::General(err.to_string()))?,
        )
        .map(|mut json| {
            json.push('\n');
            json
        })
        .map_err(|err| AppError::General(err.to_string())),
        other => Err(AppError::General(format!("Unsupported plugin schema format: {other}"))),
    }
}

pub fn parse_vars(values: &[String]) -> Result<BTreeMap<String, String>> {
    let mut vars = BTreeMap::new();
    for value in values {
        let Some((name, value)) = value.split_once('=') else {
            return Err(AppError::General(format!(
                "Invalid plugin variable assignment: {value}. Use KEY=VALUE"
            )));
        };
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError::General("Plugin variable name cannot be empty".to_string()));
        }
        vars.insert(name.to_string(), value.to_string());
    }
    Ok(vars)
}

fn run_expanded_step(step: &ExpandedPluginStep) -> Result<i32> {
    let status = Command::new(&step.command)
        .args(&step.args)
        .current_dir(&step.cwd)
        .envs(&step.env)
        .status()?;
    let code = status.code().unwrap_or(1);
    if !status.success() {
        return Err(AppError::General(format!(
            "Plugin command failed with status {code}: {}",
            format_expanded_step(step)
        )));
    }
    Ok(code)
}

pub fn init_template(root: &Path, selector: &str, force: bool, dry_run: bool) -> Result<PluginTemplateInitReport> {
    let (_manifest, template) = find_template(Some(root), selector)?;
    let path = root.join(".fcs.toml");
    if path.exists() && !force && !dry_run {
        return Err(AppError::General(format!(
            "Project config already exists: {}",
            path.display()
        )));
    }

    let mut project_config =
        crate::workspace::read_project_config(root)?.unwrap_or(crate::workspace::ProjectConfig::for_workspace(root)?);
    project_config.actions = template.actions.clone();
    let contents = toml::to_string_pretty(&project_config).map_err(|err| AppError::General(err.to_string()))?;
    if !dry_run {
        fs::write(&path, &contents)?;
    }

    Ok(PluginTemplateInitReport {
        path,
        template: template.name,
        action_count: project_config.actions.len(),
        dry_run,
        contents,
    })
}

pub fn format_manifest(manifest: &PluginManifest) -> String {
    let description = manifest.plugin.description.as_deref().unwrap_or("-");
    format!(
        "{} {} [{}] commands={} templates={} source={}",
        manifest.plugin.name,
        manifest.plugin.version,
        description,
        manifest.commands.len(),
        manifest.templates.len(),
        manifest.source
    )
}

pub fn format_command(plugin_name: &str, command: &PluginCommand) -> String {
    let description = command.description.as_deref().unwrap_or("-");
    let hook_count = command.pre.len() + command.post.len();
    let env_count = command.env.len();
    format!(
        "{}:{} - {} [env={} hooks={}]",
        plugin_name, command.name, description, env_count, hook_count
    )
}

pub fn format_template(plugin_name: &str, template: &PluginTemplate) -> String {
    let description = template.description.as_deref().unwrap_or("-");
    format!(
        "{}:{} - {} [{} action(s)]",
        plugin_name,
        template.name,
        description,
        template.actions.len()
    )
}

pub fn format_expanded_command(command: &ExpandedPluginCommand) -> String {
    format_execution_plan(command)
}

fn format_expanded_step(step: &ExpandedPluginStep) -> String {
    let mut parts = vec![step.command.clone()];
    parts.extend(step.args.iter().cloned());
    let env = if step.env.is_empty() {
        "-".to_string()
    } else {
        step.env
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<String>>()
            .join(",")
    };
    format!(
        "{}:{} cwd={} env={} {}",
        step.phase,
        step.name,
        step.cwd.display(),
        env,
        parts.join(" ")
    )
}

fn parse_manifest(contents: &str, source: &str) -> Result<PluginManifest> {
    let mut manifest: PluginManifest = toml::from_str(contents)
        .map_err(|err| AppError::General(format!("Failed to parse plugin manifest {source}: {err}")))?;
    validate_manifest(&manifest, source)?;
    manifest.source = source.to_string();
    Ok(manifest)
}

fn validate_manifest(manifest: &PluginManifest, source: &str) -> Result<()> {
    if manifest.plugin.name.trim().is_empty() {
        return Err(AppError::General(format!("Plugin name is empty: {source}")));
    }
    if manifest.plugin.version.trim().is_empty() {
        return Err(AppError::General(format!(
            "Plugin version is empty: {}",
            manifest.plugin.name
        )));
    }
    Ok(())
}

fn manifest_paths(root: Option<&Path>) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(config_dir) = dirs::config_dir() {
        paths.extend(toml_files_in_dir(&config_dir.join("fcs").join("plugins")));
    }
    if let Some(root) = root {
        paths.extend(toml_files_in_dir(&root.join(".fcs").join("plugins")));
    }
    paths
}

fn toml_files_in_dir(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut paths = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some("toml"))
        .collect::<Vec<PathBuf>>();
    paths.sort();
    paths
}

fn find_command(root: Option<&Path>, selector: &str) -> Result<(PluginManifest, PluginCommand)> {
    let manifests = discover(root)?;
    let mut matches = Vec::new();
    for manifest in manifests {
        for command in &manifest.commands {
            if selector_matches(&manifest.plugin.name, &command.name, selector) {
                matches.push((manifest.clone(), command.clone()));
            }
        }
    }
    single_match(matches, "plugin command", selector)
}

fn find_template(root: Option<&Path>, selector: &str) -> Result<(PluginManifest, PluginTemplate)> {
    let manifests = discover(root)?;
    let mut matches = Vec::new();
    for manifest in manifests {
        for template in &manifest.templates {
            if selector_matches(&manifest.plugin.name, &template.name, selector) {
                matches.push((manifest.clone(), template.clone()));
            }
        }
    }
    single_match(matches, "plugin template", selector)
}

fn single_match<T>(matches: Vec<T>, kind: &str, selector: &str) -> Result<T> {
    match matches.len() {
        0 => Err(AppError::General(format!("{kind} not found: {selector}"))),
        1 => Ok(matches.into_iter().next().expect("single match")),
        _ => Err(AppError::General(format!(
            "Ambiguous {kind}: {selector}. Use plugin:name"
        ))),
    }
}

fn selector_matches(plugin_name: &str, item_name: &str, selector: &str) -> bool {
    if let Some((plugin, item)) = selector.split_once(':') {
        return plugin == plugin_name && item == item_name;
    }
    selector == item_name
}

fn expand_plugin_command(
    plugin_name: &str,
    command: &PluginCommand,
    context: &PluginContext,
    extra_args: &[String],
) -> ExpandedPluginCommand {
    let mut args = command
        .args
        .iter()
        .map(|arg| expand_template(arg, context))
        .collect::<Vec<String>>();
    args.extend(extra_args.iter().map(|arg| expand_template(arg, context)));
    let cwd = command
        .cwd
        .as_deref()
        .map(|cwd| expand_template(cwd, context))
        .map(PathBuf::from)
        .unwrap_or_else(|| context.workspace.clone());
    let cwd = if cwd.is_absolute() {
        cwd
    } else {
        context.workspace.join(cwd)
    };

    ExpandedPluginCommand {
        plugin: plugin_name.to_string(),
        name: command.name.clone(),
        command: expand_template(&command.command, context),
        args,
        cwd,
        env: expand_env(&command.env, context),
        pre_hooks: command
            .pre
            .iter()
            .enumerate()
            .map(|(index, hook)| expand_hook("pre", index, hook, context))
            .collect(),
        post_hooks: command
            .post
            .iter()
            .enumerate()
            .map(|(index, hook)| expand_hook("post", index, hook, context))
            .collect(),
    }
}

fn expand_hook(phase: &str, index: usize, hook: &PluginHookCommand, context: &PluginContext) -> ExpandedPluginStep {
    let cwd = hook
        .cwd
        .as_deref()
        .map(|cwd| expand_template(cwd, context))
        .map(PathBuf::from)
        .unwrap_or_else(|| context.workspace.clone());
    let cwd = if cwd.is_absolute() {
        cwd
    } else {
        context.workspace.join(cwd)
    };

    ExpandedPluginStep {
        phase: phase.to_string(),
        name: hook.name.clone().unwrap_or_else(|| format!("{}-{}", phase, index + 1)),
        command: expand_template(&hook.command, context),
        args: hook.args.iter().map(|arg| expand_template(arg, context)).collect(),
        cwd,
        env: expand_env(&hook.env, context),
    }
}

fn diagnostics_for_command(plugin_name: &str, command: &PluginCommand) -> Vec<PluginDiagnostic> {
    let mut diagnostics = Vec::new();
    diagnostics.push(PluginDiagnostic {
        name: format!("{}:{}", plugin_name, command.name),
        ok: !command.name.trim().is_empty() && !command.command.trim().is_empty(),
        detail: format!("command={}", command.command),
    });
    for value in command_fields(command) {
        diagnostics.extend(unknown_variables(value).into_iter().map(|variable| PluginDiagnostic {
            name: format!("{}:{}", plugin_name, command.name),
            ok: false,
            detail: format!("unknown template variable: {variable}"),
        }));
    }
    for hook in command.pre.iter().chain(command.post.iter()) {
        diagnostics.push(PluginDiagnostic {
            name: format!(
                "{}:{}:{}",
                plugin_name,
                command.name,
                hook.name.as_deref().unwrap_or("hook")
            ),
            ok: !hook.command.trim().is_empty(),
            detail: format!("hook_command={}", hook.command),
        });
        for value in hook_fields(hook) {
            diagnostics.extend(unknown_variables(value).into_iter().map(|variable| PluginDiagnostic {
                name: format!(
                    "{}:{}:{}",
                    plugin_name,
                    command.name,
                    hook.name.as_deref().unwrap_or("hook")
                ),
                ok: false,
                detail: format!("unknown template variable: {variable}"),
            }));
        }
    }
    diagnostics
}

fn diagnostics_for_template(plugin_name: &str, template: &PluginTemplate) -> Vec<PluginDiagnostic> {
    let mut diagnostics = vec![PluginDiagnostic {
        name: format!("{}:{}", plugin_name, template.name),
        ok: !template.name.trim().is_empty() && !template.actions.is_empty(),
        detail: format!("actions={}", template.actions.len()),
    }];
    for action in &template.actions {
        for value in action_fields(action) {
            diagnostics.extend(unknown_variables(value).into_iter().map(|variable| PluginDiagnostic {
                name: format!("{}:{}:{}", plugin_name, template.name, action.name),
                ok: false,
                detail: format!("unknown template variable: {variable}"),
            }));
        }
    }
    diagnostics
}

fn command_fields(command: &PluginCommand) -> Vec<&str> {
    let mut fields = vec![command.command.as_str()];
    fields.extend(command.args.iter().map(|arg| arg.as_str()));
    if let Some(cwd) = command.cwd.as_deref() {
        fields.push(cwd);
    }
    fields.extend(command.env.values().map(String::as_str));
    fields
}

fn hook_fields(hook: &PluginHookCommand) -> Vec<&str> {
    let mut fields = vec![hook.command.as_str()];
    fields.extend(hook.args.iter().map(|arg| arg.as_str()));
    if let Some(cwd) = hook.cwd.as_deref() {
        fields.push(cwd);
    }
    fields.extend(hook.env.values().map(String::as_str));
    fields
}

fn action_fields(action: &ActionConfig) -> Vec<&str> {
    let mut fields = vec![action.command.as_str()];
    fields.extend(action.args.iter().map(|arg| arg.as_str()));
    if let Some(cwd) = action.cwd.as_deref() {
        fields.push(cwd);
    }
    fields
}

fn unknown_variables(value: &str) -> Vec<String> {
    let mut unknown = BTreeMap::<String, ()>::new();
    let mut rest = value;
    while let Some(start) = rest.find('{') {
        let after_start = &rest[start + 1..];
        let Some(end) = after_start.find('}') else {
            unknown.insert("unclosed".to_string(), ());
            break;
        };
        let variable = &after_start[..end];
        if !is_known_template_variable(variable) {
            unknown.insert(variable.to_string(), ());
        }
        rest = &after_start[end + 1..];
    }
    unknown.into_keys().collect()
}

fn expand_template(template: &str, context: &PluginContext) -> String {
    let mut output = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        output.push_str(&rest[..start]);
        let after_start = &rest[start + 1..];
        let Some(end) = after_start.find('}') else {
            output.push('{');
            output.push_str(after_start);
            return output;
        };
        let variable = &after_start[..end];
        match template_variable_value(variable, context) {
            Some(value) => output.push_str(&value),
            None => {
                output.push('{');
                output.push_str(variable);
                output.push('}');
            }
        }
        rest = &after_start[end + 1..];
    }
    output.push_str(rest);
    output
}

fn expand_env(env: &BTreeMap<String, String>, context: &PluginContext) -> BTreeMap<String, String> {
    env.iter()
        .map(|(name, value)| (name.clone(), expand_template(value, context)))
        .collect()
}

fn template_variable_value(variable: &str, context: &PluginContext) -> Option<String> {
    match variable {
        "workspace" => Some(context.workspace.to_string_lossy().to_string()),
        "file" => Some(context.file.clone().unwrap_or_default()),
        "line" => Some(context.line.map(|line| line.to_string()).unwrap_or_default()),
        "symbol" => Some(context.symbol.clone().unwrap_or_default()),
        _ if variable.starts_with("env.") => {
            let name = variable.trim_start_matches("env.");
            Some(std::env::var(name).unwrap_or_default())
        }
        _ if variable.starts_with("var.") => {
            let name = variable.trim_start_matches("var.");
            Some(context.vars.get(name).cloned().unwrap_or_default())
        }
        _ => None,
    }
}

fn is_known_template_variable(variable: &str) -> bool {
    TEMPLATE_VARIABLES.contains(&variable) || variable.starts_with("env.") || variable.starts_with("var.")
}

fn plugin_schema_toml() -> String {
    r#"[plugin]
name = "example"
version = "1.0.0"
description = "Optional plugin description"

[[commands]]
name = "command-name"
description = "Command shown in fcs plugin commands"
command = "echo"
args = ["{workspace}", "{file}:{line}", "{symbol}", "{env.HOME}", "{var.mode}"]
cwd = "{workspace}"
env = { FCS_PLUGIN_MODE = "{var.mode}", FCS_WORKSPACE = "{workspace}" }

[[commands.pre]]
name = "before"
command = "echo"
args = ["pre hook"]
cwd = "{workspace}"
env = { FCS_HOOK = "pre" }

[[commands.post]]
name = "after"
command = "echo"
args = ["post hook"]
cwd = "{workspace}"
env = { FCS_HOOK = "post" }

[[templates]]
name = "template-name"
description = "Template shown in fcs plugin templates"

[[templates.actions]]
name = "project-action"
description = "Action copied into .fcs.toml"
command = "echo"
args = ["{workspace}"]
cwd = "{workspace}"
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_plugin_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("fcs_plugins_{name}_{}", std::process::id()))
    }

    #[test]
    fn discovers_builtin_manifest() {
        let manifests = discover(None).unwrap();

        assert!(manifests.iter().any(|manifest| manifest.plugin.name == "builtin-dev"));
    }

    #[test]
    fn expands_plugin_command_templates() {
        let root = PathBuf::from("/tmp/project");
        let command = PluginCommand {
            name: "show".to_string(),
            description: None,
            command: "echo".to_string(),
            args: vec!["{file}:{line}".to_string(), "{symbol}".to_string()],
            cwd: Some("{workspace}".to_string()),
            env: BTreeMap::new(),
            pre: Vec::new(),
            post: Vec::new(),
        };
        let context = PluginContext {
            workspace: root.clone(),
            file: Some("src/main.rs".to_string()),
            line: Some(7),
            symbol: Some("main".to_string()),
            vars: BTreeMap::new(),
        };

        let expanded = expand_plugin_command("demo", &command, &context, &["--extra".to_string()]);

        assert_eq!(expanded.cwd, root);
        assert_eq!(expanded.args, vec!["src/main.rs:7", "main", "--extra"]);
    }

    #[test]
    fn expands_plugin_env_hooks_and_custom_vars() {
        let root = PathBuf::from("/tmp/project");
        let command = PluginCommand {
            name: "trace".to_string(),
            description: None,
            command: "echo".to_string(),
            args: vec!["{var.mode}".to_string()],
            cwd: Some("{workspace}".to_string()),
            env: BTreeMap::from([("MODE".to_string(), "{var.mode}".to_string())]),
            pre: vec![PluginHookCommand {
                name: Some("prepare".to_string()),
                command: "echo".to_string(),
                args: vec!["{file}".to_string()],
                cwd: Some("{workspace}".to_string()),
                env: BTreeMap::from([("HOOK".to_string(), "pre-{var.mode}".to_string())]),
            }],
            post: vec![PluginHookCommand {
                name: Some("cleanup".to_string()),
                command: "echo".to_string(),
                args: vec!["done".to_string()],
                cwd: None,
                env: BTreeMap::new(),
            }],
        };
        let context = PluginContext {
            workspace: root.clone(),
            file: Some("src/main.rs".to_string()),
            line: Some(7),
            symbol: Some("main".to_string()),
            vars: BTreeMap::from([("mode".to_string(), "debug".to_string())]),
        };

        let expanded = expand_plugin_command("demo", &command, &context, &[]);
        let plan = format_execution_plan(&expanded);

        assert_eq!(expanded.env.get("MODE").map(String::as_str), Some("debug"));
        assert_eq!(
            expanded.pre_hooks[0].env.get("HOOK").map(String::as_str),
            Some("pre-debug")
        );
        assert_eq!(execution_steps(&expanded).len(), 3);
        assert!(plan.contains("pre:prepare"));
        assert!(plan.contains("main:trace"));
        assert!(plan.contains("post:cleanup"));
    }

    #[test]
    fn init_template_supports_dry_run() {
        let root = temp_plugin_dir("init");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"demo\"\n").unwrap();
        fs::write(root.join("src").join("main.rs"), "fn main() {}\n").unwrap();

        let report = init_template(&root, "builtin-dev:rust-debug", false, true).unwrap();

        assert!(report.dry_run);
        assert_eq!(report.action_count, 2);
        assert!(report.contents.contains("cargo-check"));
        assert!(!report.path.exists());

        let _ = fs::remove_dir_all(&root);
    }
}
