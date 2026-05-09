use std::sync::Arc;

use super::ReplayApp;

impl ReplayApp {
    pub(crate) fn load_profile(&mut self, user: String) {
        self.loading_profiles.insert(user.clone());
        let profile_tx = self.profile_tx.clone();
        let status_clone = Arc::clone(&self.status);

        crate::core::profiles::spawn_profile_load(user, profile_tx, Some(status_clone));
    }
}
