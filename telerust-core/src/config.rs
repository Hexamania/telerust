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

        // On Windows, config_dir() = %APPDATA%\telerust\telerust\config
        // The config.toml is at %APPDATA%\telerust\telerust\config.toml (parent of config_dir)
        let config_dir = proj_dirs.config_dir().parent()
            .ok_or_else(|| anyhow::anyhow!("Could not determine config parent directory"))?;
        let config_file = config_dir.join("config.toml");

        eprintln!("DEBUG: Config dir: {:?}", config_dir);
        eprintln!("DEBUG: Config file: {:?}", config_file);
        eprintln!("DEBUG: Config file exists: {}", config_file.exists());

        let mut builder = Config::builder()
            .add_source(File::from(config_file.clone()).required(false))
            .add_source(Environment::with_prefix("TELERUST").separator("__"));

        // Set defaults
        builder = builder
            .set_default("telegram.api_id", 0i64)?
            .set_default("telegram.api_hash", "")?
            .set_default("telegram.phone_number", "")?
            .set_default("app.log_level", "info")?
            .set_default("app.download_dir", config_dir.join("downloads").to_string_lossy().to_string())?;

        let config = builder.build()?;
        let result: AppConfig = config.try_deserialize()?;
        eprintln!("DEBUG: Loaded api_id: {}, api_hash: {}", result.telegram.api_id, result.telegram.api_hash);
        Ok(result)
    }

    /// Get the session file path
    pub fn session_file(&self) -> PathBuf {
        let proj_dirs = ProjectDirs::from("com", "telerust", "telerust")
            .expect("Could not determine config directory");
        // Use the same parent directory as config.toml
        proj_dirs.config_dir().parent()
            .expect("Could not determine config parent directory")
            .join("session.grammers")
    }
}