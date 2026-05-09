use crate::core::{self, DownloadedReplayInfo};

use super::ReplayApp;

impl ReplayApp {
    pub(crate) fn check_downloaded_replays(&mut self) {
        let replays = core::downloads::scan_downloaded_replays(&self.settings.download_dir);
        self.downloaded_replays = replays.iter().map(|replay| replay.id.clone()).collect();
        self.downloaded_replay_cache = replays;
        self.downloaded_replay_cache_dirty = false;
    }

    pub fn mark_downloaded_replay_cache_dirty(&mut self) {
        self.downloaded_replay_cache_dirty = true;
    }

    pub fn get_downloaded_replays(&mut self) -> Vec<DownloadedReplayInfo> {
        if self.downloaded_replay_cache_dirty {
            self.downloaded_replay_cache = core::downloads::scan_downloaded_replays(&self.settings.download_dir);
            self.downloaded_replay_cache_dirty = false;
        }

        self.downloaded_replay_cache.clone()
    }

    pub fn delete_replay_file(&mut self, replay_info: &DownloadedReplayInfo) -> Result<(), std::io::Error> {
        core::downloads::delete_replay_file(replay_info)?;

        self.downloaded_replays.remove(&replay_info.id);

        self.mark_downloaded_replay_cache_dirty();

        self.show_success(format!("Deleted replay: {}", replay_info.filename));

        Ok(())
    }
}
