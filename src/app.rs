use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

use serde::{Serialize, Deserialize};
use eframe::egui::{self, CentralPanel, Context};
use eframe::{App, CreationContext};
use reqwest::blocking::Client;

use crate::tools::replay_processor::{
    download_replay, process_replay, Config, Progress, ProgressUpdate, DownloadProgress,
    ReplayItem, ApiResponse, ApiReplay, MetaData, API_BASE_URL,
};

type DownloadedReplaysSender = std::sync::mpsc::Sender<String>;
type DownloadedReplaysReceiver = std::sync::mpsc::Receiver<String>;

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
    progress: Arc<Mutex<Option<Progress>>>,
    status: Arc<Mutex<String>>,
    is_processing_local: bool,
    is_downloading: bool,
    selected_path: Option<PathBuf>,
    show_completion_dialog: bool,
    current_page: Page,
    replay_list: ReplayListState,
    profile_textures: HashMap<String, egui::TextureHandle>,
    loading_profiles: HashSet<String>,
    profile_tx: std::sync::mpsc::Sender<(String, egui::ColorImage)>,
    profile_rx: std::sync::mpsc::Receiver<(String, egui::ColorImage)>,
    download_progress: Arc<Mutex<Option<DownloadProgress>>>,
    downloading_replay_id: Option<String>,
    downloaded_replays: HashSet<String>,
    downloaded_tx: DownloadedReplaysSender,
    downloaded_rx: DownloadedReplaysReceiver,
    settings: Settings,
    last_refresh_time: Instant,
}

impl ReplayApp {
    pub fn new(cc: &CreationContext<'_>) -> Self {
        let (profile_tx, profile_rx) = std::sync::mpsc::channel();
        let (downloaded_tx, downloaded_rx) = std::sync::mpsc::channel();

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
            last_refresh_time: std::time::Instant::now(),
        };
        app.refresh_replays();
        app.check_downloaded_replays();
        app
    }

    fn load_profile(&mut self, user: String) {
        self.loading_profiles.insert(user.clone());
        let profile_tx = self.profile_tx.clone();
        thread::spawn(move || {
            let client = Client::builder()
                .timeout(Some(Duration::from_secs(10)))
                .build()
                .expect("Failed to build HTTP client");
            let url = format!("http://prod.cdn.pavlov-vr.com/avatar/{}.png", user);
            if let Ok(response) = client.get(&url).send() {
                if let Ok(bytes) = response.bytes() {
                    if let Ok(img) = image::load_from_memory(&bytes) {
                        let img = img.to_rgba8();
                        let size = [img.width() as usize, img.height() as usize];
                        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &img.into_raw());
                        let _ = profile_tx.send((user, color_image));
                    }
                }
            }
        });
    }

    fn fetch_replays(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;

        let offset = self.replay_list.current_page * 100;
        let url = format!(
            "{}/find/?game=all&offset={}&live=false",
            API_BASE_URL, offset
        );

        let response = client.get(&url).send()?.json::<ApiResponse>()?;
        self.replay_list.total_pages = (response.total as f32 / 100.0).ceil() as usize;
        self.replay_list.replays = response
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
                self.last_refresh_time = std::time::Instant::now();
                
                // Check for auto-download triggers after refreshing
                self.check_auto_download_triggers();
            }
            Err(e) => {
                if let Ok(mut status) = self.status.lock() {
                    *status = format!("Error loading replays: {}", e);
                }
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

    fn start_processing(&mut self) {
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

    fn process_online_replay(&mut self, replay_id: &str) {
        self.is_downloading = true;
        self.downloading_replay_id = Some(replay_id.to_string());

        let replay_id_clone = replay_id.to_string();
        let status_clone = Arc::clone(&self.status);
        let progress_clone = Arc::clone(&self.download_progress);
        let downloaded_tx = self.downloaded_tx.clone();
        let download_dir = self.settings.download_dir.clone();

        thread::spawn(move || {
            if let Ok(mut status) = status_clone.lock() {
                *status = "Downloading replay...".to_string();
            }

            let client = Client::builder()
                .build()
                .unwrap();

            let total_steps = 5;
            let mut current_step = 0;

            let update_progress = |current: usize, max: usize, is_build: bool| {
                if let Ok(mut progress) = progress_clone.lock() {
                    let progress_val = if max == 0 { 0.0 } else { current as f32 / max as f32 };
                    if let Some(p) = progress.as_mut() {
                        if is_build {
                            p.build.current = current;
                            p.build.max = max;
                        } else {
                            p.download.current = current;
                            p.download.max = max;
                        }
                    }
                }
            };

            if let Ok(mut progress) = progress_clone.lock() {
                *progress = Some(DownloadProgress::default());
            }

            let result: Result<(), Box<dyn std::error::Error>> = (|| {
                update_progress(current_step, total_steps, false);

                for step in 0..total_steps {
                    current_step = step;
                    update_progress(current_step, total_steps, false);
                    thread::sleep(Duration::from_millis(100));
                }

                let replay_data = download_replay(&replay_id_clone)
                    .map_err(|e| -> Box<dyn std::error::Error> { e })?;

                update_progress(0, 100, true);
                for i in 0..100 {
                    thread::sleep(Duration::from_millis(10));
                    update_progress(i + 1, 100, true);
                }

                let metadata_result = client
                    .get(&format!("{}/meta/{}", API_BASE_URL, replay_id_clone))
                    .send()?
                    .json::<MetaData>()?;

                let created_datetime = chrono::DateTime::parse_from_rfc3339(&metadata_result.created)
                    .or_else(|_| -> Result<_, Box<dyn std::error::Error>> {
                        let ts = metadata_result.created
                            .parse::<i64>()
                            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                        chrono::DateTime::from_timestamp(ts, 0)
                            .map(|dt| dt.fixed_offset())
                            .ok_or_else(|| "Invalid timestamp".into())
                    })?;

                let formatted_date = created_datetime.format("%Y.%m.%d-%H.%M.%S");
                let sanitized_name = metadata_result.friendly_name.replace([' ', '/', '\\', ':'], "-");
                let filename = format!(
                    "{}-{}-{}({}).replay",
                    sanitized_name,
                    metadata_result.game_mode,
                    formatted_date,
                    replay_id_clone
                );
                let output_path = download_dir.join(filename);

                fs::write(output_path, replay_data)?;

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
        if let Ok(entries) = std::fs::read_dir(std::env::current_dir().unwrap_or_default()) {
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

                                        if let Ok(mut file) = std::fs::File::open(entry.path()) {
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
        if let Some(replay_id) = &self.downloading_replay_id {
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

    fn styled_button(&self, ui: &mut egui::Ui, text: &str) -> egui::Response {
        ui.add_sized(
            [ui.available_width().min(120.0), 32.0],
            egui::Button::new(text)
        )
    }

    fn get_filtered_replays(&self) -> Vec<ReplayItem> {
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

                match self.replay_list.filters.platform {
                    PlatformFilter::All => true,
                    PlatformFilter::Quest => replay.shack,
                    PlatformFilter::PC => !replay.shack, 
                }
            })
            .cloned()
            .collect()
    }

    fn render_main_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.heading("Available Replays");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.styled_button(ui, "Refresh").clicked() {
                    self.refresh_replays();
                }
            });
        });
        ui.separator();

        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label("Game Mode:");
                ui.add_sized([120.0, 24.0],
                             egui::TextEdit::singleline(&mut self.replay_list.filters.game_mode)
                             .hint_text("Filter"));
                
                ui.label("Map:");
                ui.add_sized([120.0, 24.0],
                             egui::TextEdit::singleline(&mut self.replay_list.filters.map_name)
                             .hint_text("Filter"));
                
                ui.label("Workshop Mods:");
                ui.add_sized([120.0, 24.0],
                             egui::TextEdit::singleline(&mut self.replay_list.filters.workshop_mods)
                             .hint_text("Filter"));
                
                ui.label("User ID:");
                ui.add_sized([120.0, 24.0],
                             egui::TextEdit::singleline(&mut self.replay_list.filters.user_id)
                             .hint_text("Filter"));
                
                ui.label("Platform:");
                egui::ComboBox::from_id_source("platform_filter")
                    .selected_text(match self.replay_list.filters.platform {
                        PlatformFilter::All => "All",
                        PlatformFilter::Quest => "Quest",
                        PlatformFilter::PC => "PC",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.replay_list.filters.platform, PlatformFilter::All, "All");
                        ui.selectable_value(&mut self.replay_list.filters.platform, PlatformFilter::Quest, "Quest");
                        ui.selectable_value(&mut self.replay_list.filters.platform, PlatformFilter::PC, "PC");
                    });
            });
        });

        let filtered_replays = self.get_filtered_replays();

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 8.0);

                if filtered_replays.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.label("No replays match the current filters");
                    });
                } else {
                    for replay in &filtered_replays {
                        self.render_replay_item(ui, ctx, replay);
                    }
                }
            });
            
        if self.replay_list.total_pages > 0 {
            egui::Area::new("pagination_controls")
                .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-20.0, -20.0))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    egui::Frame::none()
                        .fill(ctx.style().visuals.window_fill)
                        .shadow(egui::epaint::Shadow::small_dark())
                        .rounding(5.0)
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                if ui.add_enabled(
                                    self.replay_list.current_page > 0,
                                    egui::Button::new("<")
                                        .min_size(egui::vec2(32.0, 32.0))
                                ).clicked() {
                                    self.replay_list.current_page -= 1;
                                    self.refresh_replays();
                                }
                                
                                ui.label(format!("Page {} of {}",
                                    self.replay_list.current_page + 1,
                                    self.replay_list.total_pages.max(1)
                                ));
                                
                                if ui.add_enabled(
                                    self.replay_list.current_page < self.replay_list.total_pages - 1,
                                    egui::Button::new(">")
                                        .min_size(egui::vec2(32.0, 32.0))
                                ).clicked() {
                                    self.replay_list.current_page += 1;
                                    self.refresh_replays();
                                }
                            });
                        });
                });
        }
    }

    fn render_replay_item(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, replay: &ReplayItem) {
        ui.push_id(replay.id.as_str(), |ui| {
            egui::Frame::none()
                .outer_margin(egui::style::Margin::symmetric(8.0, 4.0))
                .show(ui, |ui| {
                    egui::Frame::group(ui.style())
                        .fill(ui.style().visuals.extreme_bg_color)
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(&replay.map_name)
                                        .strong()
                                        .size(16.0));

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        let is_downloading = self.downloading_replay_id
                                            .as_ref()
                                            .map_or(false, |id| id == &replay.id);
                                        
                                        let is_downloaded = self.downloaded_replays.contains(&replay.id);

                                        if is_downloaded {
                                            ui.add_enabled(
                                                false, 
                                                egui::Button::new("Downloaded")
                                                    .min_size(egui::vec2(ui.available_width().min(120.0), 32.0))
                                            );
                                        } else if !is_downloading && 
                                            self.styled_button(ui, "Download & Process").clicked() {
                                            self.process_online_replay(&replay.id);
                                        }
                                    });
                                });
    
                                ui.horizontal_wrapped(|ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;
                                    ui.label("Game Mode:");
                                    ui.label(&replay.game_mode);
                                    ui.separator();
                                    ui.label("Date:");
                                    ui.label(&replay.created_date);
                                });
    
                                ui.horizontal_wrapped(|ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;
                                    ui.label("Workshop Mods:");
                                    ui.label(&replay.workshop_mods);
                                });
    
                                ui.horizontal_wrapped(|ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;
                                    ui.label("Time Since:");
                                    ui.label(format!("{}s", replay.time_since));
                                });
    
                                ui.separator();
    
                                egui::ScrollArea::horizontal()
                                    .id_source(format!("scroll_{}", replay.id))
                                    .max_height(72.0)
                                    .show(ui, |ui| {
                                        ui.vertical_centered(|ui| {
                                            ui.horizontal(|ui| {
                                                ui.spacing_mut().item_spacing = egui::vec2(8.0, 0.0);
                                                for (idx, user) in replay.users.iter().enumerate() {
                                                    ui.push_id(idx, |ui| {
                                                        self.render_user_avatar(ui, ctx, user);
                                                    });
                                                }
                                            });
                                        });
                                    });
                            });
                        });
                });
        });
    }

    fn render_user_avatar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, user: &str) {
        let avatar_size = egui::vec2(64.0, 64.0);
        
        egui::Frame::none()
            .fill(ui.style().visuals.window_fill)
            .inner_margin(egui::style::Margin::same(0.0))
            .outer_margin(egui::style::Margin::same(0.0))
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
                            ctx.output_mut(|out| {
                                out.copied_text = user.to_string();
                            });
                        }
                        
                        response = Some(btn_response);
                    });
                } else {
                    ui.centered_and_justified(|ui| {
                        let btn_response = ui.add_sized(avatar_size, egui::Button::new("Loading"));
                        
                        if btn_response.clicked() {
                            ctx.output_mut(|out| {
                                out.copied_text = user.to_string();
                            });
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
                            4.0,
                            egui::Stroke::new(2.0, ui.style().visuals.selection.bg_fill)
                        );
                        
                        resp.on_hover_text(user);
                    }
                }
            });
    }

    fn render_process_page(&mut self, ui: &mut egui::Ui) {
        ui.heading("Process Local Replay");
        ui.separator();

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        if let Some(path) = &self.selected_path {
                            ui.label("Directory:");
                            ui.add(egui::Label::new(path.display().to_string())
                                .wrap(true));
                        } else {
                            ui.label("No directory selected");
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if self.styled_button(ui, "Select Directory").clicked() {
                                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                    self.selected_path = Some(path);
                                }
                            }
                        });
                    });
                });

                if !self.is_processing_local && !self.show_completion_dialog {
                    let can_process = self.selected_path.is_some();
                    ui.add_space(8.0);
                    ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui| {
                        if ui.add_enabled(
                            can_process,
                            egui::Button::new("Start Processing")
                                .min_size(egui::vec2(ui.available_width().min(120.0), 32.0))
                        ).clicked() {
                            self.start_processing();
                        }
                    });
                    if !can_process {
                        ui.colored_label(ui.style().visuals.error_fg_color, "Please select a directory first");
                    }
                }

                if let Ok(progress) = self.progress.lock() {
                    if let Some(p) = &*progress {
                        ui.add_space(16.0);
                        egui::Frame::group(ui.style())
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.spacing_mut().item_spacing.y = 8.0;

                                let progress_bar = |ui: &mut egui::Ui, label, progress| {
                                    ui.label(label);
                                    ui.add(egui::ProgressBar::new(progress)
                                        .show_percentage()
                                        .animate(true));
                                };

                                progress_bar(ui, "Header:", p.header.progress());
                                progress_bar(ui, "Data Chunks:", p.data_chunks.progress());
                                progress_bar(ui, "Event Chunks:", p.event_chunks.progress());
                                progress_bar(ui, "Checkpoint Chunks:", p.checkpoint_chunks.progress());
                            });
                    }
                }

                if let Ok(status) = self.status.lock() {
                    ui.add_space(8.0);
                    ui.separator();
                    ui.colored_label(
                        if status.contains("Error") {
                            ui.style().visuals.error_fg_color
                        } else {
                            ui.style().visuals.text_color()
                        },
                        status.as_str()
                    );
                }
            });
    }

    fn render_settings_page(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings");
        ui.separator();
        
        ui.add_space(8.0);
        
        // Download directory settings
        ui.group(|ui| {
            ui.vertical(|ui| {
                ui.heading("Download Directory");
                ui.horizontal(|ui| {
                    let path_text = self.settings.download_dir.display().to_string();
                    ui.label("Save replays to:");
                    ui.add(egui::Label::new(path_text).wrap(true));
                    
                    if self.styled_button(ui, "Browse").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            self.settings.download_dir = path;
                            if let Err(err) = self.save_settings() {
                                if let Ok(mut status) = self.status.lock() {
                                    *status = format!("Error saving settings: {}", err);
                                }
                            }
                        }
                    }
                });
                
                ui.add_space(4.0);
                ui.label("This is where downloaded replays will be saved");
            });
        });
        
        ui.add_space(16.0);
        
        // Auto refresh settings
        ui.group(|ui| {
            ui.vertical(|ui| {
                ui.heading("Auto Refresh");
                
                ui.checkbox(&mut self.settings.auto_refresh_enabled, "Enable auto refresh");
                
                ui.add_enabled(
                    self.settings.auto_refresh_enabled,
                    egui::Slider::new(&mut self.settings.auto_refresh_interval_mins, 1..=60)
                        .text("Refresh interval (minutes)")
                        .clamp_to_range(true)
                );
                
                ui.add_space(4.0);
                ui.label("Automatically refresh the replay list at the specified interval");
            });
        });

        ui.add_space(16.0);

        // Auto download settings
        ui.group(|ui| {
            ui.vertical(|ui| {
                ui.heading("Auto Download");
                
                ui.checkbox(&mut self.settings.auto_download_enabled, "Enable auto download");
                
                ui.add_space(4.0);
                ui.label("User ID trigger:");
                ui.add_enabled(
                    self.settings.auto_download_enabled,
                    egui::TextEdit::singleline(&mut self.settings.auto_download_trigger_user_id)
                        .hint_text("Enter user ID to auto-download")
                );
                
                ui.add_space(4.0);
                ui.label("Automatically download replays containing the specified user ID");
            });
        });
        
        // Apply button
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                if self.styled_button(ui, "Apply").clicked() {
                    if let Err(err) = self.save_settings() {
                        if let Ok(mut status) = self.status.lock() {
                            *status = format!("Error saving settings: {}", err);
                        }
                    } else {
                        if let Ok(mut status) = self.status.lock() {
                            *status = "Settings saved successfully".to_string();
                        }
                    }
                }
            });
        });
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

    fn save_settings(&self) -> Result<(), Box<dyn std::error::Error>> {
        let settings_dir = Self::get_settings_dir()?;
        fs::create_dir_all(&settings_dir)?;
        
        let settings_file = settings_dir.join("settings.json");
        let settings_str = serde_json::to_string_pretty(&self.settings)?;
        
        fs::write(settings_file, settings_str)?;
        Ok(())
    }

    fn get_settings_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let mut path = if let Some(proj_dirs) = directories::ProjectDirs::from("com", "PavlovVR", "ReplayToolbox") {
            proj_dirs.config_dir().to_path_buf()
        } else {
            let mut path = std::env::current_dir()?;
            path.push(".config");
            path
        };
        
        fs::create_dir_all(&path)?;
        Ok(path)
    }
}

impl App for ReplayApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
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
            self.downloaded_replays.insert(replay_id);
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

                // Add flexible space to push the Settings button to the right
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
                Page::Main => self.render_main_page(ui, ctx),
                Page::ProcessLocal => self.render_process_page(ui),
                Page::Settings => self.render_settings_page(ui),
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
        
        ctx.request_repaint_after(Duration::from_millis(32));
    }
}