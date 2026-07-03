use crate::errors::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const CONFIG_SCHEMA_VERSION: u32 = 1;

const LEGACY_CONFIG_SCHEMA_VERSION: u32 = 0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "current_config_schema_version", alias = "version")]
    pub schema_version: u32,
    pub search: SearchConfig,
    pub skim: SkimConfig,
    #[serde(default)]
    pub editor: EditorConfig,
    #[serde(default)]
    pub lsp: LspConfig,
    #[serde(default)]
    pub tui: TuiConfig,
    #[serde(default)]
    pub runtime: RuntimeConfig,
    #[serde(default)]
    pub actions: Vec<ActionConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    pub rg_options: Vec<String>,
    pub ignore: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkimConfig {
    pub binds: Vec<String>,
    pub height: String,
    pub min_height: String,
    pub color: String,
    pub exact: bool,
    pub tac: bool,
    pub cycle: bool,
    pub preview_window: String,
    #[serde(default = "default_tab_width")]
    pub tab_width: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EditorConfig {
    pub command: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspConfig {
    pub clangd_command: String,
    pub request_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiConfig {
    #[serde(default)]
    pub keymap: TuiKeymapConfig,
    #[serde(default)]
    pub theme: TuiThemeConfig,
    #[serde(default = "default_true")]
    pub live_query: bool,
    #[serde(default)]
    pub trace_on_open: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub cache_probe: bool,
    pub log_dir: String,
    pub latency_warn_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiKeymapConfig {
    pub command_palette: String,
    pub query: String,
    pub open: String,
    pub refresh: String,
    pub trace: String,
    pub breakpoint: String,
    pub debug: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiThemeConfig {
    pub name: String,
    pub color: bool,
    pub syntax_highlight: bool,
    pub low_color: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionConfig {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSchemaStatus {
    Current,
    Legacy,
    Future,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigSchemaDiagnostic {
    pub configured_version: Option<u32>,
    pub effective_version: u32,
    pub supported_version: u32,
    pub status: ConfigSchemaStatus,
    pub message: String,
}

impl Default for LspConfig {
    fn default() -> Self {
        Self {
            clangd_command: "clangd".to_string(),
            request_timeout_ms: 3000,
        }
    }
}

impl Default for TuiKeymapConfig {
    fn default() -> Self {
        Self {
            command_palette: ":".to_string(),
            query: "/".to_string(),
            open: "o".to_string(),
            refresh: "r".to_string(),
            trace: "a".to_string(),
            breakpoint: "b".to_string(),
            debug: "D".to_string(),
        }
    }
}

impl Default for TuiThemeConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            color: true,
            syntax_highlight: true,
            low_color: false,
        }
    }
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            keymap: TuiKeymapConfig::default(),
            theme: TuiThemeConfig::default(),
            live_query: true,
            trace_on_open: false,
        }
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            cache_probe: true,
            log_dir: ".fcs/logs".to_string(),
            latency_warn_ms: 500,
        }
    }
}

fn default_tab_width() -> usize {
    4
}

fn current_config_schema_version() -> u32 {
    CONFIG_SCHEMA_VERSION
}

fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Config {
            schema_version: CONFIG_SCHEMA_VERSION,
            search: SearchConfig {
                rg_options: Vec::new(),
                ignore: vec![
                    ".git/".to_string(),
                    "target/".to_string(),
                    "node_modules/".to_string(),
                    "*.tmp".to_string(),
                    "*.log".to_string(),
                ],
            },
            skim: SkimConfig {
                binds: vec![
                    "ctrl-u:half-page-up".to_string(),
                    "ctrl-d:half-page-down".to_string(),
                    "ctrl-r:kill-line".to_string(),
                    "ctrl-v:toggle-preview".to_string(),
                    "alt-u:preview-page-up".to_string(),
                    "alt-d:preview-page-down".to_string(),
                    "alt-j:preview-down".to_string(),
                    "alt-k:preview-up".to_string(),
                ],
                height: "100%".to_string(),
                min_height: "20".to_string(),
                color:
                    "fg:-1,bg:-1,hl:33,fg+:254,bg+:235,hl+:33,info:136,prompt:136,pointer:230,marker:230,spinner:136"
                        .to_string(),
                exact: true,
                tac: true,
                cycle: true,
                preview_window: "right:59%".to_string(),
                tab_width: 4,
            },
            editor: EditorConfig::default(),
            lsp: LspConfig::default(),
            tui: TuiConfig::default(),
            runtime: RuntimeConfig::default(),
            actions: Vec::new(),
        }
    }
}

impl Config {
    pub fn load_or_create() -> Result<Self> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| AppError::General("Could not find configuration directory".to_string()))?;

        let fcs_dir = config_dir.join("fcs");
        let config_path = fcs_dir.join("fcs.toml");

        if !config_path.exists() {
            fs::create_dir_all(&fcs_dir)?;
            let default_config = Self::default();
            let toml_string = toml::to_string_pretty(&default_config)
                .map_err(|e| AppError::General(format!("Failed to serialize default config: {e}")))?;
            fs::write(&config_path, toml_string)?;
            return Ok(default_config);
        }

        let contents = fs::read_to_string(&config_path)?;
        Self::load_from_str(&contents, &config_path)
    }

    pub fn load_from_str(contents: &str, path: &Path) -> Result<Self> {
        let diagnostic = diagnose_config_schema(contents)?;
        if diagnostic.status == ConfigSchemaStatus::Future {
            return Err(AppError::General(format!(
                "Config schema_version {} at {} is newer than supported schema_version {}. \
                 Upgrade fcs, or restore a config written for this version before retrying.",
                diagnostic.effective_version,
                path.display(),
                diagnostic.supported_version
            )));
        }

        let mut config: Config = toml::from_str(contents)
            .map_err(|e| AppError::General(format!("Failed to parse config at {}: {e}", path.display())))?;
        config.schema_version = diagnostic.effective_version;

        Ok(config)
    }
}

pub fn diagnose_config_schema(contents: &str) -> Result<ConfigSchemaDiagnostic> {
    let value: toml::Value = toml::from_str(contents)
        .map_err(|e| AppError::General(format!("Failed to parse config schema header: {e}")))?;
    let configured_version = read_config_schema_version(&value)?;
    let raw_version = configured_version.unwrap_or(LEGACY_CONFIG_SCHEMA_VERSION);
    let status = match configured_version {
        None => ConfigSchemaStatus::Legacy,
        Some(version) if version == CONFIG_SCHEMA_VERSION => ConfigSchemaStatus::Current,
        Some(version) if version > CONFIG_SCHEMA_VERSION => ConfigSchemaStatus::Future,
        Some(_) => ConfigSchemaStatus::Legacy,
    };
    let effective_version = if raw_version > CONFIG_SCHEMA_VERSION {
        raw_version
    } else {
        CONFIG_SCHEMA_VERSION
    };
    let message = match status {
        ConfigSchemaStatus::Current => format!("config schema_version {CONFIG_SCHEMA_VERSION} is current"),
        ConfigSchemaStatus::Legacy => match configured_version {
            Some(version) => format!(
                "config schema_version {version} is older than supported schema_version {CONFIG_SCHEMA_VERSION}; \
                 fcs will read it with compatibility defaults"
            ),
            None => format!(
                "config has no schema_version; treating it as legacy schema_version {LEGACY_CONFIG_SCHEMA_VERSION} \
                 and reading it with compatibility defaults"
            ),
        },
        ConfigSchemaStatus::Future => format!(
            "config schema_version {effective_version} is newer than supported schema_version {CONFIG_SCHEMA_VERSION}"
        ),
    };

    Ok(ConfigSchemaDiagnostic {
        configured_version,
        effective_version,
        supported_version: CONFIG_SCHEMA_VERSION,
        status,
        message,
    })
}

fn read_config_schema_version(value: &toml::Value) -> Result<Option<u32>> {
    let Some(raw_version) = value.get("schema_version").or_else(|| value.get("version")) else {
        return Ok(None);
    };
    let Some(version) = raw_version.as_integer() else {
        return Err(AppError::General(
            "Config schema_version must be a non-negative integer".to_string(),
        ));
    };
    if version < 0 || version > u32::MAX as i64 {
        return Err(AppError::General(
            "Config schema_version must fit in an unsigned 32-bit integer".to_string(),
        ));
    }

    Ok(Some(version as u32))
}

static CONFIG: std::sync::OnceLock<Config> = std::sync::OnceLock::new();

pub fn init_global(config: Config) {
    let _ = CONFIG.set(config);
}

pub fn get_global() -> &'static Config {
    CONFIG.get_or_init(|| Config::load_or_create().unwrap_or_else(|_| Config::default()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn minimal_legacy_config() -> &'static str {
        r#"
[search]
rg_options = []
ignore = []

[skim]
binds = []
height = "100%"
min_height = "20"
color = "fg:-1"
exact = true
tac = true
cycle = true
preview_window = "right:50%"
"#
    }

    #[test]
    fn default_config_serializes_schema_version() {
        let contents = toml::to_string_pretty(&Config::default()).unwrap();
        assert!(contents.contains("schema_version = 1"));
    }

    #[test]
    fn tui_defaults_enable_live_query_without_trace_on_open() {
        let tui = TuiConfig::default();
        assert!(tui.live_query);
        assert!(!tui.trace_on_open);
    }

    #[test]
    fn missing_tui_behavior_fields_load_with_defaults() {
        let contents = r#"
[search]
rg_options = []
ignore = []

[skim]
binds = []
height = "100%"
min_height = "20"
color = "fg:-1"
exact = true
tac = true
cycle = true
preview_window = "right:50%"

[tui.keymap]
command_palette = ":"
query = "/"
open = "o"
refresh = "r"
trace = "a"
breakpoint = "b"
debug = "D"
"#;
        let config: Config = toml::from_str(contents).unwrap();
        assert!(config.tui.live_query);
        assert!(!config.tui.trace_on_open);
    }

    #[test]
    fn missing_schema_version_is_legacy_but_loads() {
        let diagnostic = diagnose_config_schema(minimal_legacy_config()).unwrap();
        assert_eq!(diagnostic.status, ConfigSchemaStatus::Legacy);
        assert_eq!(diagnostic.configured_version, None);
        assert_eq!(diagnostic.effective_version, CONFIG_SCHEMA_VERSION);

        let config = Config::load_from_str(minimal_legacy_config(), Path::new("legacy-fcs.toml")).unwrap();
        assert_eq!(config.schema_version, CONFIG_SCHEMA_VERSION);
        assert_eq!(config.skim.tab_width, default_tab_width());
    }

    #[test]
    fn version_alias_is_accepted_for_compatibility() {
        let contents = format!("version = 1\n{}", minimal_legacy_config());
        let diagnostic = diagnose_config_schema(&contents).unwrap();
        assert_eq!(diagnostic.status, ConfigSchemaStatus::Current);

        let config = Config::load_from_str(&contents, Path::new("alias-fcs.toml")).unwrap();
        assert_eq!(config.schema_version, CONFIG_SCHEMA_VERSION);
    }

    #[test]
    fn future_schema_version_is_rejected_with_guidance() {
        let contents = format!("schema_version = 999\n{}", minimal_legacy_config());
        let diagnostic = diagnose_config_schema(&contents).unwrap();
        assert_eq!(diagnostic.status, ConfigSchemaStatus::Future);

        let err = Config::load_from_str(&contents, Path::new("future-fcs.toml")).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("newer than supported"));
        assert!(message.contains("Upgrade fcs"));
    }
}
