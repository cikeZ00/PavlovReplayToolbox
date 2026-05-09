use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    time::Instant,
};

use eframe::{egui, CreationContext};

use crate::core::{DownloadedReplayInfo, ModInfo, ReplayListState, Settings, UpdateInfo};
use crate::tools::replay_processor::{DownloadProgress, Progress};

type DownloadedReplaysSender = mpsc::Sender<String>;
type DownloadedReplaysReceiver = mpsc::Receiver<String>;
type UpdateInfoReceiver = mpsc::Receiver<UpdateInfo>;

#[derive(Clone)]
pub(crate) struct Notification {
    pub(crate) id: u64,
    pub(crate) message: String,
    pub(crate) created_at: Instant,
    pub(crate) duration_ms: u64,
    pub(crate) notification_type: NotificationType,
    pub(crate) position: f32,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum NotificationType {
    Info,
    Success,
    #[allow(dead_code)]
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Page {
    Main,
    ProcessLocal,
    Settings,
    Manage,
}

pub struct ReplayApp {
    pub progress: Arc<Mutex<Option<Progress>>>,
    pub status: Arc<Mutex<String>>,
    pub is_processing_local: bool,
    pub(crate) is_downloading: bool,
    pub selected_path: Option<PathBuf>,
    pub show_completion_dialog: bool,
    pub current_page: Page,
    pub replay_list: ReplayListState,
    pub(crate) profile_textures: HashMap<String, egui::TextureHandle>,
    pub(crate) loading_profiles: HashSet<String>,
    pub(crate) profile_tx: mpsc::Sender<(String, egui::ColorImage)>,
    pub(crate) profile_rx: mpsc::Receiver<(String, egui::ColorImage)>,
    pub(crate) download_progress: Arc<Mutex<Option<DownloadProgress>>>,
    pub downloading_replay_id: Option<String>,
    pub downloaded_replays: HashSet<String>,
    pub(crate) downloaded_tx: DownloadedReplaysSender,
    pub(crate) downloaded_rx: DownloadedReplaysReceiver,
    pub settings: Settings,
    pub(crate) last_refresh_time: Instant,
    pub(crate) downloaded_replay_cache: Vec<DownloadedReplayInfo>,
    pub(crate) downloaded_replay_cache_dirty: bool,
    pub(crate) notifications: Vec<Notification>,
    pub(crate) next_notification_id: u64,
    pub(crate) update_info: Option<UpdateInfo>,
    pub(crate) update_rx: UpdateInfoReceiver,
    pub mod_info_cache: HashMap<String, ModInfo>,
    pub mod_info_tx: mpsc::Sender<ModInfo>,
    pub mod_info_rx: mpsc::Receiver<ModInfo>,
    pub mod_thumbnail_textures: HashMap<String, egui::TextureHandle>,
    pub loading_thumbnails: HashSet<String>,
    pub mod_thumbnail_rx: mpsc::Receiver<(String, egui::ColorImage)>,
    pub mod_thumbnail_tx: mpsc::Sender<(String, egui::ColorImage)>,
}

impl ReplayApp {
    pub fn new(_cc: &CreationContext<'_>) -> Self {
        let (profile_tx, profile_rx) = mpsc::channel();
        let (downloaded_tx, downloaded_rx) = mpsc::channel();
        let (update_tx, update_rx) = mpsc::channel();
        let (mod_info_tx, mod_info_rx) = mpsc::channel();
        let (mod_thumbnail_tx, mod_thumbnail_rx) = mpsc::channel();

        let settings = Self::load_settings().unwrap_or_default();

        let mut app = Self {
            progress: Arc::new(Mutex::new(None)),
            status: Arc::new(Mutex::new("Loading replays...".to_string())),
            is_processing_local: false,
            is_downloading: false,
            selected_path: None,
            show_completion_dialog: false,
            current_page: Page::Main,
            replay_list: ReplayListState::default(),
            profile_textures: HashMap::new(),
            loading_profiles: HashSet::new(),
            profile_tx,
            profile_rx,
            download_progress: Arc::new(Mutex::new(None)),
            downloading_replay_id: None,
            downloaded_replays: HashSet::new(),
            downloaded_tx,
            downloaded_rx,
            settings,
            last_refresh_time: Instant::now(),
            downloaded_replay_cache: Vec::new(),
            downloaded_replay_cache_dirty: true,
            notifications: Vec::new(),
            next_notification_id: 0,
            update_info: None,
            update_rx,
            mod_info_cache: HashMap::new(),
            mod_info_tx,
            mod_info_rx,
            mod_thumbnail_textures: HashMap::new(),
            loading_thumbnails: HashSet::new(),
            mod_thumbnail_rx,
            mod_thumbnail_tx,
        };
        app.refresh_replays();
        app.check_downloaded_replays();

        crate::core::updates::spawn_update_check(update_tx.clone());

        app
    }

    pub(crate) fn reset_state(&mut self) {
        self.is_processing_local = false;
        self.is_downloading = false;
        self.show_completion_dialog = false;
        if let Ok(mut progress) = self.progress.lock() {
            *progress = None;
        }
        if let Ok(mut status) = self.status.lock() {
            *status = "Idle".to_string();
        }
    }
}
