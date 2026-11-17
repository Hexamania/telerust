//! Configuration management

use anyhow::Result;
use config::{Config, File, Environment};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub telegram: TelegramConfig,
    pub app: AppSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    pub api_id: i64,
    pub api_hash: String,
    pub phone_number: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub log_level: String,
    pub download_dir: PathBuf,
}

impl AppConfig {
    /// Load configuration from file and environment
    pub fn load() -> Result<Self> {
        let proj_dirs = ProjectDirs::from("com", "telerust", "telerust")
            .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;

        let config_dir = proj_dirs.config_dir();
        let config_file = config_dir.join("config.toml");

        let mut builder = Config::builder()
            .add_source(File::from(config_file).required(false))
            .add_source(Environment::with_prefix("TELERUST").separator("__"));

        // Set defaults
        builder = builder
            .set_default("telegram.api_id", 0i64)?
            .set_default("telegram.api_hash", "")?
            .set_default("telegram.phone_number", "")?
            .set_default("app.log_level", "info")?
            .set_default("app.download_dir", proj_dirs.data_dir().join("downloads").to_string_lossy().to_string())?;

        let config = builder.build()?;
        Ok(config.try_deserialize()?)
    }

    /// Get the session file path
    pub fn session_file(&self) -> PathBuf {
        let proj_dirs = ProjectDirs::from("com", "telerust", "telerust")
            .expect("Could not determine config directory");
        proj_dirs.data_dir().join("session.grammers")
    }
}