use crate::errors::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub search: SearchConfig,
    pub skim: SkimConfig,
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

fn default_tab_width() -> usize {
    4
}

impl Default for Config {
    fn default() -> Self {
        Config {
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
        let config: Config = toml::from_str(&contents)
            .map_err(|e| AppError::General(format!("Failed to parse config at {}: {e}", config_path.display())))?;

        Ok(config)
    }
}

static CONFIG: std::sync::OnceLock<Config> = std::sync::OnceLock::new();

pub fn init_global(config: Config) {
    let _ = CONFIG.set(config);
}

pub fn get_global() -> &'static Config {
    CONFIG.get_or_init(|| {
        Config::load_or_create().unwrap_or_else(|_| Config::default())
    })
}
