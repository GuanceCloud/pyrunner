pub mod cleanup;
pub mod parser;
pub mod register;
pub mod runner;
pub mod search;

use crate::config::Config;

pub struct AppServices {
    pub register: register::RegisterService,
    pub search: search::SearchService,
    pub runner: runner::RunnerService,
    pub cleanup: cleanup::CleanupService,
}

impl AppServices {
    pub fn new(config: Config) -> Self {
        Self {
            register: register::RegisterService::new(config.clone()),
            search: search::SearchService::new(config.clone()),
            runner: runner::RunnerService::new(config.clone()),
            cleanup: cleanup::CleanupService::new(config),
        }
    }
}
