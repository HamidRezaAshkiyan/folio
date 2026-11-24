use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RtlMode {
    /// Detect RTL per entry/line automatically (great for mixed RTL + LTR notes)
    #[default]
    Auto,
    /// Force Left-to-Right layout and alignment
    Ltr,
    /// Full mirrored Right-to-Left UI layout and alignment
    Rtl,
}

impl RtlMode {
    pub fn label(&self) -> &'static str {
        match self {
            RtlMode::Auto => "AUTO",
            RtlMode::Ltr => "LTR",
            RtlMode::Rtl => "RTL",
        }
    }
}

fn default_gui_theme() -> String {
    "dark".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolioConfig {
    pub vault_path: PathBuf,
    pub notebook_page: u32,
    pub default_spaces: Vec<String>,
    #[serde(default)]
    pub rtl_mode: RtlMode,
    #[serde(default = "default_gui_theme")]
    pub gui_theme: String,
}

impl Default for FolioConfig {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        Self {
            vault_path: home.join("Obsidian").join("BuJo-Vault"),
            notebook_page: 1,
            default_spaces: vec![
                "Studio & Work".to_string(),
                "Reading & Study".to_string(),
                "Personal".to_string(),
            ],
            rtl_mode: RtlMode::Auto,
            gui_theme: default_gui_theme(),
        }
    }
}

impl FolioConfig {
    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("folio")
            .join("config.toml")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if let Ok(raw) = fs::read_to_string(&path)
            && let Ok(cfg) = toml::from_str::<FolioConfig>(&raw)
        {
            return cfg;
        }
        Self::default()
    }

    #[allow(dead_code)]
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create config dir {}", parent.display()))?;
        }
        let raw = toml::to_string_pretty(self)?;
        fs::write(&path, raw)
            .with_context(|| format!("failed to write config {}", path.display()))?;
        Ok(())
    }

    pub fn resolve_vault_path(&self, cli_override: Option<PathBuf>) -> PathBuf {
        if let Some(p) = cli_override {
            return expand_tilde(p);
        }
        if let Ok(env_val) = std::env::var("FOLIO_VAULT")
            && !env_val.trim().is_empty()
        {
            return expand_tilde(PathBuf::from(env_val));
        }
        expand_tilde(self.vault_path.clone())
    }
}

pub fn expand_tilde(path: PathBuf) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(stripped) = s.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(stripped);
    }
    if s == "~"
        && let Some(home) = dirs::home_dir()
    {
        return home;
    }
    path
}
