use crate::core::{self, Settings};

use super::ReplayApp;

impl ReplayApp {
    pub(crate) fn load_settings() -> Result<Settings, Box<dyn std::error::Error>> {
        core::settings::load_settings()
    }

    pub fn save_settings(&self) -> Result<(), Box<dyn std::error::Error>> {
        core::settings::save_settings(&self.settings)
    }
}
