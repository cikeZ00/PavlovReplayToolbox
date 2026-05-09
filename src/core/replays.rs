use crate::tools::replay_processor::ReplayItem;

#[derive(Clone, Default)]
pub struct ReplayFilters {
    pub game_mode: String,
    pub map_name: String,
    pub workshop_mods: String,
    pub platform: PlatformFilter,
    pub user_id: String,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PlatformFilter {
    All,
    Quest,
    PC,
}

impl Default for PlatformFilter {
    fn default() -> Self {
        Self::All
    }
}

#[derive(Clone, Default)]
pub struct ReplayListState {
    pub replays: Vec<ReplayItem>,
    pub filtered_replays: Vec<ReplayItem>,
    pub current_page: usize,
    pub total_pages: usize,
    pub filters: ReplayFilters,
}

pub fn rebuild_filtered_replays(state: &mut ReplayListState) {
    let filter_game_mode = state.filters.game_mode.to_lowercase();
    let filter_map_name = state.filters.map_name.to_lowercase();
    let filter_mods = state.filters.workshop_mods.to_lowercase();
    let filter_user_id = state.filters.user_id.to_lowercase();

    state.filtered_replays = state.replays.iter()
        .filter(|replay| {
            if !filter_game_mode.is_empty() &&
               !replay.game_mode.to_lowercase().contains(&filter_game_mode) {
                return false;
            }

            if !filter_map_name.is_empty() &&
               !replay.map_name.to_lowercase().contains(&filter_map_name) {
                return false;
            }

            if !filter_mods.is_empty() &&
               !replay.workshop_mods.to_lowercase().contains(&filter_mods) {
                return false;
            }

            if !filter_user_id.is_empty() &&
               !replay.users.iter().any(|user| user.to_lowercase().contains(&filter_user_id)) {
                return false;
            }

            true
        })
        .cloned()
        .collect();
}
