use crate::tools::replay_processor::ReplayItem;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformFilter {
    All,
    Quest,
    PC,
}

#[derive(Debug, Clone)]
pub struct ReplayFilters {
    pub game_mode: String,
    pub map_name: String,
    pub workshop_mods: String,
    pub user_id: String,
    pub platform: PlatformFilter,
}

impl Default for ReplayFilters {
    fn default() -> Self {
        Self {
            game_mode: String::new(),
            map_name: String::new(),
            workshop_mods: String::new(),
            user_id: String::new(),
            platform: PlatformFilter::All,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReplayListState {
    pub replays: Vec<ReplayItem>,
    pub filtered_replays: Vec<ReplayItem>,
    pub current_page: usize,
    pub total_pages: usize,
    pub filters: ReplayFilters,
}

impl Default for ReplayListState {
    fn default() -> Self {
        Self {
            replays: Vec::new(),
            filtered_replays: Vec::new(),
            current_page: 0,
            total_pages: 0,
            filters: ReplayFilters::default(),
        }
    }
}

pub fn rebuild_filtered_replays(state: &mut ReplayListState) {
    let game_mode_filter = state.filters.game_mode.to_lowercase();
    let map_filter = state.filters.map_name.to_lowercase();
    let mods_filter = state.filters.workshop_mods.to_lowercase();
    let user_filter = state.filters.user_id.to_lowercase();
    let platform_filter = state.filters.platform;

    state.filtered_replays = state
        .replays
        .iter()
        .cloned()
        .filter(|replay| {
            let game_mode_ok = game_mode_filter.is_empty()
                || replay.game_mode.to_lowercase().contains(&game_mode_filter);
            let map_ok = map_filter.is_empty()
                || replay.map_name.to_lowercase().contains(&map_filter);
            let mods_ok = mods_filter.is_empty()
                || replay.workshop_mods.to_lowercase().contains(&mods_filter);
            let user_ok = user_filter.is_empty()
                || replay
                    .users
                    .iter()
                    .any(|user| user.to_lowercase().contains(&user_filter));
            let platform_ok = match platform_filter {
                PlatformFilter::All => true,
                PlatformFilter::Quest => replay.shack,
                PlatformFilter::PC => !replay.shack,
            };

            game_mode_ok && map_ok && mods_ok && user_ok && platform_ok
        })
        .collect();
}
