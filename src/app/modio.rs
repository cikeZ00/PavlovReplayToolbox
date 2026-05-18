use crate::core::{self, ModInfo};

use super::ReplayApp;

impl ReplayApp {
    pub fn load_mod_info(&mut self, mod_id: String) {
        if self.mod_info_cache.contains_key(&mod_id) {
            return;
        }

        self.mod_info_cache.insert(mod_id.clone(), ModInfo {
            id: mod_id.clone(),
            name: "Loading...".to_string(),
            description: "".to_string(),
            thumbnail_url: None,
            is_loading: true,
            failed: false,
        });

        let mod_info_tx = self.mod_info_tx.clone();
        let api_url = self.settings.modio_api_url.clone();
        let api_token = self.settings.modio_api_token.clone();

        core::modio::spawn_mod_info_load(mod_id, api_url, api_token, mod_info_tx);
    }

    pub fn parse_mod_ids(&self, workshop_mods: &str) -> Vec<String> {
        core::modio::parse_mod_ids(workshop_mods)
    }

    pub fn load_mod_thumbnail(&mut self, mod_id: String, thumbnail_url: String) {
        if self.mod_thumbnail_textures.contains_key(&mod_id) || self.loading_thumbnails.contains(&mod_id) {
            return;
        }

        self.loading_thumbnails.insert(mod_id.clone());

        let thumbnail_tx = self.mod_thumbnail_tx.clone();
        core::modio::spawn_mod_thumbnail_load(mod_id, thumbnail_url, thumbnail_tx);
    }
}
