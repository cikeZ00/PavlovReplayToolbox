use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Read,
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone)]
struct Notification {
    id: u64,
    message: String,
    created_at: Instant,
    duration_ms: u64,
    notification_type: NotificationType,
    position: f32,
}

#[derive(Clone, Copy, PartialEq)]
enum NotificationType {
    Info,
    Success,
    #[allow(dead_code)]
    Warning,
    Error,
}

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    name: String,
    body: Option<String>,
    published_at: String,
}

#[derive(Clone)]
struct UpdateInfo {
    current_version: String,
    latest_version: String,
    release_url: String,
    release_name: String,
    release_date: String,
    release_notes: String,
}

use eframe::egui::{self, CentralPanel, Context};
use eframe::{App, CreationContext};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

use crate::tools::replay_processor::{
    download_replay, process_replay, ApiResponse, Config, DownloadProgress,
    MetaData, Progress, ReplayItem, API_BASE_URL,
};

use crate::pages;

type DownloadedReplaysSender = std::sync::mpsc::Sender<String>;
type DownloadedReplaysReceiver = std::sync::mpsc::Receiver<String>;
type UpdateInfoReceiver = std::sync::mpsc::Receiver<UpdateInfo>;

#[derive(Clone, Serialize, Deserialize)]
pub struct Settings {
    pub download_dir: PathBuf,
    pub auto_refresh_enabled: bool,
    pub auto_refresh_interval_mins: u64,
    pub auto_download_enabled: bool,
    pub auto_download_trigger_user_id: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            download_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            auto_refresh_enabled: true,
            auto_refresh_interval_mins: 5,
            auto_download_enabled: false,
            auto_download_trigger_user_id: String::new(),
        }
    }
}

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
    pub current_page: usize,
    pub total_pages: usize,
    pub filters: ReplayFilters,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Page {
    Main,
    ProcessLocal,
    Settings,
}

pub struct ReplayApp {
    pub progress: Arc<Mutex<Option<Progress>>>,
    pub status: Arc<Mutex<String>>,
    pub is_processing_local: bool,
    is_downloading: bool,
    pub selected_path: Option<PathBuf>,
    pub show_completion_dialog: bool,
    current_page: Page,
    pub replay_list: ReplayListState,
    profile_textures: HashMap<String, egui::TextureHandle>,
    loading_profiles: HashSet<String>,
    profile_tx: std::sync::mpsc::Sender<(String, egui::ColorImage)>,
    profile_rx: std::sync::mpsc::Receiver<(String, egui::ColorImage)>,
    download_progress: Arc<Mutex<Option<DownloadProgress>>>,
    pub downloading_replay_id: Option<String>,
    pub downloaded_replays: HashSet<String>,
    downloaded_tx: DownloadedReplaysSender,
    downloaded_rx: DownloadedReplaysReceiver,
    pub settings: Settings,
    last_refresh_time: Instant,
    notifications: Vec<Notification>,
    next_notification_id: u64,
    update_info: Option<UpdateInfo>,
    update_rx: UpdateInfoReceiver,
}

impl ReplayApp {
    pub fn new(_cc: &CreationContext<'_>) -> Self {
        let (profile_tx, profile_rx) = std::sync::mpsc::channel();
        let (downloaded_tx, downloaded_rx) = std::sync::mpsc::channel();
        let (update_tx, update_rx) = std::sync::mpsc::channel();

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
            notifications: Vec::new(),
            next_notification_id: 0,
            update_info: None,
            update_rx,
        };
        app.refresh_replays();
        app.check_downloaded_replays();

        // Start update check
        let update_tx_clone = update_tx.clone();
        thread::spawn(move || {
            let current_version = env!("CARGO_PKG_VERSION");
            
            let client = match Client::builder()
                .timeout(Duration::from_secs(10))
                .build() {
                    Ok(client) => client,
                    Err(_) => return,
                };
                
            let url = "https://api.github.com/repos/cikeZ00/PavlovReplayToolbox/releases/latest";
            
            let response = match client.get(url)
                .header("User-Agent", "PavlovReplayToolbox")
                .send() {
                    Ok(resp) => {
                        if !resp.status().is_success() {
                            return;
                        }
                        resp
                    },
                    Err(_) => return,
                };
                
            let github_release: GitHubRelease = match response.json() {
                Ok(release) => release,
                Err(_) => return,
            };
            
            // Remove 'v' prefix if present
            let latest_version = github_release.tag_name.trim_start_matches('v').to_string();
            
            // Compare versions
            let current_segments: Vec<u32> = current_version
                .split('.')
                .filter_map(|s| s.parse().ok())
                .collect();
                
            let latest_segments: Vec<u32> = latest_version
                .split('.')
                .filter_map(|s| s.parse().ok())
                .collect();
                
            let update_available = if current_segments.len() == latest_segments.len() {
                let mut is_newer = false;
                for i in 0..current_segments.len() {
                    if latest_segments[i] > current_segments[i] {
                        is_newer = true;
                        break;
                    } else if latest_segments[i] < current_segments[i] {
                        break;
                    }
                }
                is_newer
            } else {
                // Simple fallback - just check if they're different
                current_version != latest_version
            };
            
            if update_available {
                let update_info = UpdateInfo {
                    current_version: current_version.to_string(),
                    latest_version,
                    release_url: github_release.html_url,
                    release_name: github_release.name,
                    release_date: github_release.published_at
                        .split('T')
                        .next()
                        .unwrap_or("")
                        .to_string(),
                    release_notes: github_release.body.unwrap_or_default()
                        .lines()
                        .take(10)
                        .collect::<Vec<&str>>()
                        .join("\n"),
                };
                let _ = update_tx_clone.send(update_info);
            }
        });

        app
    }

    fn load_profile(&mut self, user: String) {
        self.loading_profiles.insert(user.clone());
        let profile_tx = self.profile_tx.clone();
        let status_clone = Arc::clone(&self.status);
        
        thread::spawn(move || {
            let client = match Client::builder()
                .timeout(Some(Duration::from_secs(10)))
                .build() {
                    Ok(client) => client,
                    Err(e) => {
                        if let Ok(mut status) = status_clone.lock() {
                            *status = format!("Failed to initialize HTTP client for profile: {}", e);
                        }
                        return;
                    }
                };
                
            let url = format!("http://prod.cdn.pavlov-vr.com/avatar/{}.png", user);
            
            match client.get(&url).send() {
                Ok(response) => {
                    if !response.status().is_success() {
                        // Profile not found or server error, but we can silently fail
                        return;
                    }
                    
                    match response.bytes() {
                        Ok(bytes) => {
                            match image::load_from_memory(&bytes) {
                                Ok(img) => {
                                    let img = img.to_rgba8();
                                    let size = [img.width() as usize, img.height() as usize];
                                    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &img.into_raw());
                                    let _ = profile_tx.send((user, color_image));
                                },
                                Err(_) => {
                                    // Invalid image data, can silently fail
                                }
                            }
                        },
                        Err(_) => {
                            // Failed to get bytes, can silently fail
                        }
                    }
                },
                Err(e) => {
                    if e.is_timeout() || e.is_connect() {
                        // Connection issues, can silently fail
                        return;
                    }
                }
            }
        });
    }

    fn fetch_replays(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let client = match Client::builder()
            .timeout(Duration::from_secs(10))
            .build() {
                Ok(client) => client,
                Err(e) => return Err(format!("Failed to initialize HTTP client: {}", e).into())
            };

        let offset = self.replay_list.current_page * 100;
        
        let mut url = format!(
            "{}/find/?game=all&offset={}&live=false",
            API_BASE_URL, offset
        );
        
        match self.replay_list.filters.platform {
            PlatformFilter::Quest => url.push_str("&shack=true"),
            PlatformFilter::PC => url.push_str("&shack=false"),
            PlatformFilter::All => {} 
        }

        let response = match client.get(&url).send() {
            Ok(resp) => {
                if !resp.status().is_success() {
                    return Err(format!("Server returned error status: {} - {}", 
                        resp.status().as_u16(), 
                        resp.status().canonical_reason().unwrap_or("Unknown error")).into());
                }
                resp
            },
            Err(e) => {
                return if e.is_timeout() {
                    Err("Connection timed out. Server may be down or unreachable.".into())
                } else if e.is_connect() {
                    Err("Failed to connect to server. Please check your internet connection.".into())
                } else {
                    Err(format!("Network error: {}", e).into())
                }
            }
        };

        let api_response = match response.json::<ApiResponse>() {
            Ok(data) => data,
            Err(e) => return Err(format!("Failed to parse server response: {}. The API may have changed format.", e).into())
        };

        self.replay_list.total_pages = (api_response.total as f32 / 100.0).ceil() as usize;
        self.replay_list.replays = api_response
            .replays
            .into_iter()
            .map(|r| ReplayItem {
                id: r.id,
                game_mode: r.game_mode,
                map_name: r.map_name,
                created_date: r.created,
                time_since: r.time_since,
                shack: r.shack,
                modcount: r.modcount,
                competitive: r.competitive,
                workshop_mods: r.workshop_mods,
                live: r.live,
                users: r.users.unwrap_or_default(),
            })
            .collect();
        Ok(())
    }

    pub fn refresh_replays(&mut self) {
        if let Ok(mut status) = self.status.lock() {
            *status = "Loading replays...".to_string();
        }

        match self.fetch_replays() {
            Ok(_) => {
                if let Ok(mut status) = self.status.lock() {
                    *status = "Replays loaded successfully".to_string();
                }
                self.show_success("Replays loaded successfully");
                self.last_refresh_time = Instant::now();
                
                // Check for auto-download triggers after refreshing
                self.check_auto_download_triggers();
            }
            Err(e) => {
                let error_message = format!("Error loading replays: {}", e);
                if let Ok(mut status) = self.status.lock() {
                    *status = error_message.clone();
                }
                self.show_error(error_message);
            }
        }
    }
    
    fn check_auto_download_triggers(&mut self) {
        if !self.settings.auto_download_enabled || 
           self.settings.auto_download_trigger_user_id.is_empty() ||
           self.is_downloading {
            return;
        }
    
        let trigger_user_id = self.settings.auto_download_trigger_user_id.to_lowercase();
        
        let replay_to_download = self.replay_list.replays.iter()
            .find(|replay| {
                !self.downloaded_replays.contains(&replay.id) && 
                replay.users.iter().any(|user| user.to_lowercase().contains(&trigger_user_id))
            })
            .map(|replay| replay.id.clone());
        
        if let Some(replay_id) = replay_to_download {
            if let Ok(mut status) = self.status.lock() {
                *status = format!("Auto-downloading replay with user ID: {}", 
                                 self.settings.auto_download_trigger_user_id);
            }
            
            self.process_online_replay(&replay_id);
        }
    }

    fn reset_state(&mut self) {
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

    pub fn start_processing(&mut self) {
        if self.is_processing_local || self.selected_path.is_none() {
            return;
        }
        self.is_processing_local = true;

        let progress_clone = Arc::clone(&self.progress);
        let status_clone = Arc::clone(&self.status);
        let path_clone = self.selected_path.clone().unwrap();

        thread::spawn(move || {
            if let Err(e) = std::env::set_current_dir(&path_clone) {
                if let Ok(mut status) = status_clone.lock() {
                    *status = format!("Error: Failed to set working directory - {}", e);
                }
                return;
            }

            let config = Config {
                update_callback: Box::new(move |progress| {
                    if let Ok(mut lock) = progress_clone.lock() {
                        *lock = Some(progress);
                    }
                }),
                ..Default::default()
            };

            let result = process_replay(Some(config));

            if let Ok(mut status) = status_clone.lock() {
                *status = match result {
                    Ok(_) => "Replay processing complete.".into(),
                    Err(e) => format!("Error: {}", e),
                };
            }
        });
    }

    pub fn process_online_replay(&mut self, replay_id: &str) {
        self.is_downloading = true;
        self.downloading_replay_id = Some(replay_id.to_string());
        self.show_info(format!("Downloading replay {}", replay_id));

        let replay_id_clone = replay_id.to_string();
        let status_clone = Arc::clone(&self.status);
        let progress_clone = Arc::clone(&self.download_progress);
        let downloaded_tx = self.downloaded_tx.clone();
        let download_dir = self.settings.download_dir.clone();

        thread::spawn(move || {
            if let Ok(mut status) = status_clone.lock() {
                *status = "Downloading replay...".to_string();
            }

            let client = match Client::builder().build() {
                Ok(client) => client,
                Err(e) => {
                    if let Ok(mut status) = status_clone.lock() {
                        *status = format!("Failed to initialize HTTP client: {}", e);
                    }
                    return;
                }
            };

            // Initialize progress tracking
            if let Ok(mut progress) = progress_clone.lock() {
                *progress = Some(DownloadProgress::default());
            }

            let download_progress_callback = {
                let progress_clone = Arc::clone(&progress_clone);
                Box::new(move |current: usize, total: usize| {
                    if let Ok(mut progress) = progress_clone.lock() {
                        if let Some(p) = progress.as_mut() {
                            p.download.current = current;
                            p.download.max = total;
                        }
                    }
                }) as Box<dyn Fn(usize, usize) + Send + Sync>
            };

            let result: Result<(), Box<dyn std::error::Error>> = (|| {
                let replay_data = match download_replay(&replay_id_clone, Some(download_progress_callback)) {
                    Ok(data) => data,
                    Err(e) => return Err(format!("Failed to download replay data: {}", e).into())
                };

                let update_build_progress = |current: usize, max: usize| {
                    if let Ok(mut progress) = progress_clone.lock() {
                        if let Some(p) = progress.as_mut() {
                            p.build.current = current;
                            p.build.max = max;
                        }
                    }
                };

                update_build_progress(0, 100);

                let metadata_result = match client
                    .get(&format!("{}/meta/{}", API_BASE_URL, replay_id_clone))
                    .send() {
                        Ok(resp) => {
                            update_build_progress(10, 100);
                            
                            if !resp.status().is_success() {
                                return Err(format!(
                                    "Failed to fetch replay metadata: Server returned {} - {}", 
                                    resp.status().as_u16(),
                                    resp.status().canonical_reason().unwrap_or("Unknown error")
                                ).into());
                            }
                            
                            match resp.json::<MetaData>() {
                                Ok(data) => {
                                    update_build_progress(20, 100);
                                    data
                                },
                                Err(e) => return Err(format!(
                                    "Failed to parse replay metadata: {}. The API format may have changed.", e
                                ).into())
                            }
                        },
                        Err(e) => {
                            return if e.is_timeout() {
                                Err("Connection timed out while fetching replay metadata.".into())
                            } else if e.is_connect() {
                                Err("Failed to connect to metadata server. Please check your internet connection.".into())
                            } else {
                                Err(format!("Network error retrieving metadata: {}", e).into())
                            }
                        }
                    };

                update_build_progress(30, 100);

                let created_datetime = match chrono::DateTime::parse_from_rfc3339(&metadata_result.created)
                    .or_else(|_| -> Result<_, Box<dyn std::error::Error>> {
                        let ts = metadata_result.created
                            .parse::<i64>()
                            .map_err(|e| format!("Invalid timestamp format: {}", e))?;
                        chrono::DateTime::from_timestamp(ts, 0)
                            .map(|dt| dt.fixed_offset())
                            .ok_or_else(|| "Invalid timestamp".into())
                    }) {
                        Ok(dt) => {
                            update_build_progress(40, 100);
                            dt
                        },
                        Err(e) => return Err(format!("Failed to parse replay date: {}", e).into())
                    };

                update_build_progress(50, 100);

                let formatted_date = created_datetime.format("%Y.%m.%d-%H.%M.%S");
                let sanitized_name = metadata_result.friendly_name.replace([' ', '/', '\\', ':'], "-");
                let filename = format!(
                    "{}-{}-{}({}).replay",
                    sanitized_name,
                    metadata_result.game_mode,
                    formatted_date,
                    replay_id_clone
                );

                update_build_progress(75, 100);
                
                let output_path = download_dir.join(filename);
                update_build_progress(90, 100);
                
                match fs::write(output_path, replay_data) {
                    Ok(_) => {
                        update_build_progress(100, 100);
                    },
                    Err(e) => return Err(format!("Failed to save replay file: {}", e).into())
                }

                let _ = downloaded_tx.send(replay_id_clone);

                if let Ok(mut status) = status_clone.lock() {
                    *status = "Replay downloaded and processed successfully.".to_string();
                }

                Ok(())
            })();

            if let Err(e) = result {
                if let Ok(mut status) = status_clone.lock() {
                    *status = format!("Error: {}", e);
                }
            }

            if let Ok(mut progress) = progress_clone.lock() {
                *progress = None;
            }
        });
    }

    fn check_downloaded_replays(&mut self) {
        if let Ok(entries) = fs::read_dir(std::env::current_dir().unwrap_or_default()) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_file() {
                        if let Some(ext) = entry.path().extension() {
                            if ext == "replay" {
                                if let Some(filename) = entry.path().file_name() {
                                    if let Some(filename_str) = filename.to_str() {
                                        if let Some(id_start) = filename_str.rfind('(') {
                                            if let Some(id_end) = filename_str[id_start..].find(')') {
                                                let id = &filename_str[id_start + 1..id_start + id_end];
                                                self.downloaded_replays.insert(id.to_string());
                                                continue;
                                            }
                                        }

                                        if let Ok(mut file) = fs::File::open(entry.path()) {
                                            let mut buffer = [0; 1024];
                                            if file.read(&mut buffer).is_ok() {
                                                let content = String::from_utf8_lossy(&buffer);
                                                if let Some(id_start) = content.find("\"id\":\"") {
                                                    let id_start = id_start + 6;
                                                    if let Some(id_end) = content[id_start..].find('"') {
                                                        let id = &content[id_start..id_start + id_end];
                                                        self.downloaded_replays.insert(id.to_string());
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn render_download_progress(&mut self, ctx: &Context) {
        if let Some(_replay_id) = &self.downloading_replay_id {
            if let Ok(progress) = self.download_progress.lock() {
                if let Some(p) = &*progress {
                    egui::Window::new("Downloading Replay")
                        .collapsible(false)
                        .resizable(false)
                        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                        .show(ctx, |ui| {
                            ui.set_min_width(300.0);

                            ui.label("Downloading components:");
                            ui.add(egui::ProgressBar::new(
                                p.download.progress())
                                .show_percentage()
                                .animate(true)
                            );

                            ui.add_space(8.0);
                            ui.label("Building replay:");
                            ui.add(egui::ProgressBar::new(
                                p.build.progress())
                                .show_percentage()
                                .animate(true)
                            );

                            ui.add_space(8.0);
                            if let Ok(status) = self.status.lock() {
                                ui.label(&*status);
                            }
                        });
                } else {
                    self.downloading_replay_id = None;
                }
            }
        }
    }

    pub fn styled_button(&self, ui: &mut egui::Ui, text: &str) -> egui::Response {
        ui.add_sized(
            [ui.available_width().min(120.0), 32.0],
            egui::Button::new(text)
        )
    }

    pub fn get_filtered_replays(&self) -> Vec<ReplayItem> {
        self.replay_list.replays.iter()
            .filter(|replay| {
                if !self.replay_list.filters.game_mode.is_empty() && 
                   !replay.game_mode.to_lowercase().contains(&self.replay_list.filters.game_mode.to_lowercase()) {
                    return false;
                }

                if !self.replay_list.filters.map_name.is_empty() && 
                   !replay.map_name.to_lowercase().contains(&self.replay_list.filters.map_name.to_lowercase()) {
                    return false;
                }

                if !self.replay_list.filters.workshop_mods.is_empty() && 
                   !replay.workshop_mods.to_lowercase().contains(&self.replay_list.filters.workshop_mods.to_lowercase()) {
                    return false;
                }

                if !self.replay_list.filters.user_id.is_empty() &&
                   !replay.users.iter().any(|user| user.to_lowercase().contains(&self.replay_list.filters.user_id.to_lowercase())) {
                    return false;
                }

                true
            })
            .cloned()
            .collect()
    }

    pub fn render_user_avatar(&mut self, ui: &mut egui::Ui, ctx: &Context, user: &str) {
        let avatar_size = egui::vec2(64.0, 64.0);
        
        egui::Frame::new()
            .fill(ui.style().visuals.window_fill)
            .inner_margin(0.0)
            .outer_margin(0.0)
            .show(ui, |ui| {
                ui.set_min_size(avatar_size);
                ui.set_max_size(avatar_size);
                
                let mut response = None;
                
                if let Some(texture) = self.profile_textures.get(user) {
                    ui.centered_and_justified(|ui| {
                        let btn_response = ui.add_sized(
                            avatar_size,
                            egui::Button::image_and_text(texture, "")
                                .frame(false)
                        );
                        
                        if btn_response.clicked() {
                            ctx.copy_text(user.to_string());
                        }
                        
                        response = Some(btn_response);
                    });
                } else {
                    ui.centered_and_justified(|ui| {
                        let btn_response = ui.add_sized(avatar_size, egui::Button::new("Loading"));
                        
                        if btn_response.clicked() {
                            ctx.copy_text(user.to_string());
                        }
                        
                        response = Some(btn_response);
                    });
                    
                    if !self.loading_profiles.contains(user) {
                        self.load_profile(user.to_string());
                    }
                }
                
                if let Some(resp) = response {
                    if resp.hovered() {
                        let rect = resp.rect;
                        ui.painter().rect_stroke(
                            rect.expand(2.0), 
                            egui::epaint::CornerRadius::ZERO,
                            egui::Stroke::new(2.0, ui.style().visuals.selection.bg_fill),
                            egui::epaint::StrokeKind::Outside,
                        );
                        
                        resp.on_hover_text(user);
                    }
                }
            });
    }

    pub fn show_success(&mut self, message: impl Into<String>) {
        self.show_notification(message.into(), NotificationType::Success)
    }

    pub fn show_error(&mut self, message: impl Into<String>) {
        self.show_notification(message.into(), NotificationType::Error)
    }

    fn load_settings() -> Result<Settings, Box<dyn std::error::Error>> {
        let settings_dir = Self::get_settings_dir()?;
        let settings_file = settings_dir.join("settings.json");
        
        if !settings_file.exists() {
            return Ok(Settings::default());
        }

        let settings_str = fs::read_to_string(settings_file)?;
        let settings = serde_json::from_str(&settings_str)?;
        Ok(settings)
    }

    pub fn save_settings(&self) -> Result<(), Box<dyn std::error::Error>> {
        let settings_dir = Self::get_settings_dir()?;
        fs::create_dir_all(&settings_dir)?;
        
        let settings_file = settings_dir.join("settings.json");
        let settings_str = serde_json::to_string_pretty(&self.settings)?;
        
        fs::write(settings_file, settings_str)?;
        Ok(())
    }

    fn get_settings_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let path = if let Some(proj_dirs) = directories::ProjectDirs::from("com", "PavlovVR", "ReplayToolbox") {
            proj_dirs.config_dir().to_path_buf()
        } else {
            let mut path = std::env::current_dir()?;
            path.push(".config");
            path
        };
        
        fs::create_dir_all(&path)?;
        Ok(path)
    }

    fn show_notification(&mut self, message: String, notification_type: NotificationType) {
        let id = self.next_notification_id;
        self.next_notification_id += 1;
        
        self.notifications.push(Notification {
            id,
            message,
            created_at: Instant::now(),
            duration_ms: 4000,
            notification_type,
            position: 0.0,
        });
    }
    
    fn show_info(&mut self, message: impl Into<String>) {
        self.show_notification(message.into(), NotificationType::Info)
    }
    
    #[allow(dead_code)]
    fn show_warning(&mut self, message: impl Into<String>) {
        self.show_notification(message.into(), NotificationType::Warning)
    }
    
    fn update_notifications(&mut self) {
        let now = Instant::now();
        
        self.notifications.retain(|notification| {
            now.duration_since(notification.created_at).as_millis() < notification.duration_ms as u128
        });
        
        self.notifications.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        
        for notification in &mut self.notifications {
            let elapsed_ms = now.duration_since(notification.created_at).as_millis() as f32;
            let animation_duration = 400.0;
            let t = (elapsed_ms / animation_duration).min(1.0);
            notification.position = Self::cubic_ease_out(t);
        }
    }

    fn cubic_ease_out(t: f32) -> f32 {
        let f = t - 1.0;
        f * f * f + 1.0
    }

    fn render_notifications(&self, ctx: &Context) {
        let notification_height = 40.0;
        let notification_spacing = 8.0;
        let max_visible = 5;
        let bottom_margin = 20.0;
        
        let visible_notifications = self.notifications.iter().take(max_visible).collect::<Vec<_>>();
        
        // Render notifications from bottom to top
        for (idx, notification) in visible_notifications.iter().enumerate() {
            let pos = notification.position;
            
            let elapsed_ms = Instant::now().duration_since(notification.created_at).as_millis() as f32;
            let fade_out_start = notification.duration_ms as f32 - 1000.0; 
            
            let alpha = if pos < 0.4 { 
                Self::cubic_ease_out(pos / 0.4)
            } else if elapsed_ms > fade_out_start {
                (1.0 - ((elapsed_ms - fade_out_start) / 900.0).min(1.0)).powf(2.0)
            } else {
                1.0
            };
            
            // Base position in stack
            let base_position = idx as f32 * (notification_height + notification_spacing);
            let slide_offset = if pos < 1.0 { (1.0 - pos) * notification_height * 1.2 } else { 0.0 };
            
            // Final position
            let bottom_offset = bottom_margin + base_position + slide_offset;
            
            let bg_color = match notification.notification_type {
                NotificationType::Info => egui::Color32::from_rgba_unmultiplied(30, 130, 220, (alpha * 220.0) as u8),
                NotificationType::Success => egui::Color32::from_rgba_unmultiplied(30, 150, 30, (alpha * 220.0) as u8),
                NotificationType::Warning => egui::Color32::from_rgba_unmultiplied(220, 160, 20, (alpha * 220.0) as u8),
                NotificationType::Error => egui::Color32::from_rgba_unmultiplied(220, 40, 40, (alpha * 220.0) as u8),
            };
            
            // Render notification
            egui::Area::new(egui::Id::new(format!("notification_{}", notification.id)))
                .anchor(egui::Align2::CENTER_BOTTOM, egui::Vec2::new(0.0, -bottom_offset))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    egui::Frame::new() 
                        .fill(bg_color)
                        .corner_radius(8.0) 
                        .shadow(egui::epaint::Shadow {
                            offset: [0, 2],  
                            blur: 4,         
                            spread: 0,       
                            color: ctx.style().visuals.window_shadow.color, 
                        }) 
                        .show(ui, |ui| {
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                ui.add_space(12.0);
                                ui.colored_label(
                                    egui::Color32::from_rgba_unmultiplied(255, 255, 255, (alpha * 255.0) as u8),
                                    &notification.message
                                );
                                ui.add_space(12.0);
                            });
                            ui.add_space(6.0);
                        });
                });
        }
    }
}

impl App for ReplayApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // Check for updates
        if let Ok(update_info) = self.update_rx.try_recv() {
            self.update_info = Some(update_info.clone());
            self.show_info(format!(
                "New version {} available! You are running {}",
                update_info.latest_version,
                update_info.current_version
            ));
        }
        
        // Show update dialog if we have update info
        if let Some(update_info) = &self.update_info {
            let release_url = update_info.release_url.clone();
            let current_version = update_info.current_version.clone();
            let latest_version = update_info.latest_version.clone();
            let release_name = update_info.release_name.clone();
            let release_date = update_info.release_date.clone();
            let release_notes = update_info.release_notes.clone();
            
            let mut should_close = false;
            let mut error_message = None;
            
            egui::Window::new("Update Available")
                .collapsible(true)
                .resizable(true)
                .default_size([400.0, 300.0])
                .show(ctx, |ui| {
                    ui.heading("New Version Available!");
                    ui.add_space(8.0);
                    
                    ui.horizontal(|ui| {
                        ui.label("Current Version:");
                        ui.strong(current_version);
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Latest Version:");
                        ui.strong(latest_version);
                    });
                    
                    ui.add_space(8.0);
                    ui.label(format!("Release: {}", release_name));
                    ui.label(format!("Released on: {}", release_date));
                    
                    ui.add_space(8.0);
                    ui.label("Release Notes:");
                    ui.add_space(4.0);
                    
                    egui::ScrollArea::vertical()
                        .max_height(120.0)
                        .show(ui, |ui| {
                            ui.label(&release_notes);
                        });
                    
                    ui.add_space(8.0);
                    
                    if ui.button("Download Update").clicked() {
                        if let Err(err) = open::that(&release_url) {
                            error_message = Some(format!("Failed to open browser: {}", err));
                        }
                    }
                    
                    if ui.button("Remind Me Later").clicked() {
                        should_close = true;
                    }
                });
            
            // Process results after the UI closure
            if let Some(err) = error_message {
                self.show_error(err);
            }
            
            if should_close {
                self.update_info = None;
            }
        }
        
        // Update notifications
        self.update_notifications();
        
        self.render_download_progress(ctx);
        
        while let Ok((user, color_image)) = self.profile_rx.try_recv() {
            let texture_handle = ctx.load_texture(
                &format!("avatar_{}", user),
                color_image,
                egui::TextureOptions {
                    magnification: egui::TextureFilter::Linear,
                    minification: egui::TextureFilter::Linear,
                    ..Default::default()
                },
            );
            self.profile_textures.insert(user.clone(), texture_handle);
            self.loading_profiles.remove(&user);
        }
        
        while let Ok(replay_id) = self.downloaded_rx.try_recv() {
            self.downloaded_replays.insert(replay_id.clone());
            self.show_success(format!("Replay {} downloaded successfully", replay_id));
        }

        if self.show_completion_dialog {
            egui::Window::new("Processing Complete")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    if let Ok(status) = self.status.lock() {
                        ui.label(status.as_str());
                    }
                    if ui.button("OK").clicked() {
                        self.reset_state();
                    }
                });
        }

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let button_height = 32.0;

                ui.add_sized(
                    [80.0, button_height],
                    egui::SelectableLabel::new(
                        self.current_page == Page::Main,
                        "Replays"
                    )
                ).clicked().then(|| {
                    self.current_page = Page::Main;
                });

                ui.add_sized(
                    [120.0, button_height],
                    egui::SelectableLabel::new(
                        self.current_page == Page::ProcessLocal,
                        "Local Processing"
                    )
                ).clicked().then(|| {
                    self.current_page = Page::ProcessLocal;
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_sized(
                        [80.0, button_height],
                        egui::SelectableLabel::new(
                            self.current_page == Page::Settings,
                            "Settings"
                        )
                    ).clicked().then(|| {
                        self.current_page = Page::Settings;
                    });
                });
            });
            ui.add_space(4.0);
            ui.separator();
        });

        CentralPanel::default().show(ctx, |ui| {
            match self.current_page {
                Page::Main => pages::render_main_page(self, ui, ctx),
                Page::ProcessLocal => pages::render_process_page(self, ui),
                Page::Settings => pages::render_settings_page(self, ui),
            }
        });

        if self.is_processing_local {
            if let Ok(status) = self.status.lock() {
                if status.contains("complete") || status.contains("Error") {
                    self.show_completion_dialog = true;
                    self.is_processing_local = false;
                }
            }
        }
        
        if self.is_downloading && self.downloading_replay_id.is_none() {
            self.is_downloading = false;
        }

        if self.settings.auto_refresh_enabled && 
           self.last_refresh_time.elapsed() > Duration::from_secs(self.settings.auto_refresh_interval_mins * 60) &&
           self.current_page == Page::Main && 
           !self.is_downloading {
            self.refresh_replays();
        } else if self.settings.auto_download_enabled &&
                 !self.is_downloading && 
                 self.current_page == Page::Main {
            self.check_auto_download_triggers();
        }

        self.render_notifications(ctx);
        
        ctx.request_repaint_after(Duration::from_millis(32));
    }
}