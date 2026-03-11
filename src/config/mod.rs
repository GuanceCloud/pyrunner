use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub cache_dir: PathBuf,
    pub default_interpreter: String,
    pub max_scripts: usize,
    pub max_age_days: u32,
    pub default_timeout_secs: u64,
    pub logging: LoggingConfig,
    pub matching: MatchingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub file: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchingConfig {
    pub similarity_threshold: f64,
    pub fts_weight: f64,
    pub tag_weight: f64,
    pub function_weight: f64,
    pub usage_weight: f64,
}

impl Config {
    pub fn load_or_default() -> Result<Self> {
        let path = Self::default_path()?;
        if path.exists() {
            let raw = fs::read_to_string(path)?;
            Ok(toml::from_str(&raw)?)
        } else {
            Ok(Self::default())
        }
    }

    pub fn default_path() -> Result<PathBuf> {
        Ok(Self::default().cache_dir.join("config.toml"))
    }

    pub fn database_path(&self) -> PathBuf {
        self.cache_dir.join("metadata.db")
    }
}

impl Default for Config {
    fn default() -> Self {
        let cache_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".pyrunner");

        Self {
            cache_dir: cache_dir.clone(),
            default_interpreter: "python3".to_string(),
            max_scripts: 1000,
            max_age_days: 90,
            default_timeout_secs: 60,
            logging: LoggingConfig {
                level: "info".to_string(),
                file: cache_dir.join("logs").join("pyrunner.log"),
            },
            matching: MatchingConfig {
                similarity_threshold: 0.85,
                fts_weight: 0.50,
                tag_weight: 0.20,
                function_weight: 0.20,
                usage_weight: 0.10,
            },
        }
    }
}
