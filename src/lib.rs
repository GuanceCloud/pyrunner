pub mod cli;
pub mod config;
pub mod db;
pub mod models;
pub mod services;
pub mod utils;

use anyhow::Result;
use config::Config;
use db::Database;
use services::AppServices;

pub struct AppContext {
    pub config: Config,
    pub database: Database,
    pub services: AppServices,
}

impl AppContext {
    pub fn bootstrap() -> Result<Self> {
        let config = Config::load_or_default()?;
        let database = Database::new(&config.database_path())?;
        let services = AppServices::new(config.clone());

        Ok(Self {
            config,
            database,
            services,
        })
    }
}
