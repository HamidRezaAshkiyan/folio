use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolioConfig {
    pub vault_path: PathBuf,
    pub notebook_page: u32,
    pub default_spaces: Vec<String>,
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
