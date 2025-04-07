#![windows_subsystem = "windows"]
mod build_meta;
mod build_replay;
mod replay_buffer;

use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
};

use eframe::egui::{self, CentralPanel, Context};
use eframe::{run_native, App, CreationContext, NativeOptions};
use serde::Deserialize;

use build_meta::build_meta;
use build_replay::build_replay;

use reqwest::blocking::Client;
use std::time::Duration;


#[derive(Debug, Clone)]
struct ReplayItem {
    id: String,
    game_mode: String,
    map_name: String,
    created_date: String,
    total_time: i32,
    version: i32,
    competitive: bool,
    workshop_mods: String,
    live: bool,
}

#[derive(Default, Clone)]
struct ReplayFilters {
    game_mode: String,
    map_name: String,
    workshop_mods: String,
}

#[derive(Clone, Default)]
struct ReplayListState {
    replays: Vec<ReplayItem>,
    current_page: usize,
    total_pages: usize,
    filters: ReplayFilters,
}

struct Config {
    update_callback: Box<dyn Fn(Progress) + Send + Sync>,
    data_count: usize,
    event_count: usize,
    checkpoint_count: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            update_callback: Box::new(|progress| {
                println!("Progress: {:?}", progress);
            }),
            data_count: usize::MAX,
            event_count: usize::MAX,
            checkpoint_count: usize::MAX,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct Progress {
    header: ProgressUpdate,
    data_chunks: ProgressUpdate,
    event_chunks: ProgressUpdate,
    checkpoint_chunks: ProgressUpdate,
}

#[derive(Debug, Clone, Default)]
struct ProgressUpdate {
    current: usize,
    max: usize,
}

impl ProgressUpdate {
    fn progress(&self) -> f32 {
        if self.max == 0 {
            return 0.0;
        }
        self.current as f32 / self.max as f32
    }
}

#[derive(Deserialize)]
struct MetadataFile {
    meta: Option<MetaData>,
    #[serde(rename = "events_pavlov")]
    events_pavlov: Option<EventsWrapper>,
    events: Option<EventsWrapper>,
}

#[derive(Deserialize, Clone)]
pub struct MetaData {
    #[serde(rename = "gameMode")]
    pub game_mode: String,
    #[serde(rename = "friendlyName")]
    pub friendly_name: String,
    pub competitive: bool,
    pub workshop_mods: String,
    pub live: bool,
    #[serde(rename = "totalTime")]
    pub total_time: i32,
    #[serde(rename = "__v")]
    pub version: i32,
    pub created: String,
}

#[derive(Deserialize)]
struct EventsWrapper {
    events: Vec<Event>,
}

#[derive(Debug, Deserialize, Clone)]
struct Event {
    id: Option<String>,
    group: Option<String>,
    meta: Option<String>,
    time1: Option<i32>,
    time2: Option<i32>,
    data: Option<EventData>,
}

#[derive(Debug, Deserialize, Clone)]
struct EventData {
    #[serde(rename = "type")]
    typ: Option<String>,
    data: Option<Vec<u8>>,
}

#[derive(Deserialize)]
struct TimingEntry {
    numchunks: String,
    mtime1: String,
    mtime2: String,
}

#[derive(Debug)]
struct Chunk {
    data: Vec<u8>,
    chunk_type: u32,
    time1: Option<i32>,
    time2: Option<i32>,
    id: Option<String>,
    group: Option<String>,
    metadata: Option<String>,
    size_in_bytes: Option<i32>,
}

fn replay_chunks_dir() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    exe_dir.join("replay_chunks")
}

fn load_json_file<T: for<'de> Deserialize<'de>>(file_path: &Path, file_name: &str) -> Result<T, Box<dyn std::error::Error>> {
    if !file_path.exists() {
        return Err(format!("{} file not found at {:?}", file_name, file_path).into());
    }
    let content = fs::read_to_string(file_path)?;
    let parsed = serde_json::from_str(&content)?;
    Ok(parsed)
}

fn load_chunk_file(file_path: &Path) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if !file_path.exists() {
        return Err(format!("Chunk file not found: {:?}", file_path).into());
    }
    Ok(fs::read(file_path)?)
}

fn process_replay(config: Option<Config>) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let config = config.unwrap_or_default();
    let chunks_dir = replay_chunks_dir();
    let metadata_path = chunks_dir.join("metadata.json");
    let timing_path = chunks_dir.join("timing.json");

    let metadata_file: MetadataFile = load_json_file(&metadata_path, "Metadata")?;
    let timing_data: Vec<TimingEntry> = load_json_file(&timing_path, "Timing Data")?;

    let meta = metadata_file
        .meta
        .ok_or_else(|| "Invalid metadata: missing 'meta' field")?;

    let update_callback = &config.update_callback;
    let mut download_chunks: Vec<Chunk> = Vec::new();
    let mut progress = Progress::default();

    let pavlov_events = metadata_file
        .events_pavlov
        .as_ref()
        .map(|ew| ew.events.clone())
        .unwrap_or_default();
    let checkpoint_events = metadata_file
        .events
        .as_ref()
        .map(|ew| ew.events.clone())
        .unwrap_or_default();

    let meta_buffer = build_meta(&meta)?;

    let header_file = chunks_dir.join("replay.header");
    let header_data = load_chunk_file(&header_file)?;
    download_chunks.push(Chunk {
        data: header_data,
        chunk_type: 0,
        time1: None,
        time2: None,
        id: None,
        group: None,
        metadata: None,
        size_in_bytes: None,
    });

    let mut stream_files: Vec<PathBuf> = fs::read_dir(&chunks_dir)?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.file_name().map(|f| f.to_string_lossy().starts_with("stream.")).unwrap_or(false))
        .collect();

    stream_files.sort_by_key(|p| {
        p.file_name()
            .and_then(|s| s.to_str())
            .and_then(|s| s.split('.').nth(1))
            .and_then(|num| num.parse::<i32>().ok())
            .unwrap_or(0)
    });

    // Initialize progress
    progress = Progress {
        header: ProgressUpdate { current: 0, max: 1 },
        data_chunks: ProgressUpdate {
            current: 0,
            max: std::cmp::min(stream_files.len(), config.data_count),
        },
        event_chunks: ProgressUpdate {
            current: 0,
            max: std::cmp::min(pavlov_events.len(), config.event_count),
        },
        checkpoint_chunks: ProgressUpdate {
            current: 0,
            max: std::cmp::min(checkpoint_events.len(), config.checkpoint_count),
        },
    };
    update_callback(progress.clone());

    // Update header progress
    progress.header.current = 1;
    update_callback(progress.clone());

    let mut current_offset = 0usize;

    // Process stream files
    for (index, file_path) in stream_files.into_iter().enumerate() {
        if index >= config.data_count {
            break;
        }
        let file_data = load_chunk_file(&file_path)?;
        if file_data.is_empty() {
            continue;
        }

        let chunk_number = index + 1;
        let timing_entry = timing_data.iter().find(|entry| {
            entry
                .numchunks
                .parse::<usize>()
                .map(|n| n == chunk_number)
                .unwrap_or(false)
        });
        let time1 = timing_entry.and_then(|t| t.mtime1.parse::<i32>().ok()).unwrap_or(0);
        let time2 = timing_entry.and_then(|t| t.mtime2.parse::<i32>().ok()).unwrap_or(0);

        download_chunks.push(Chunk {
            data: file_data.clone(),
            chunk_type: 1,
            time1: Some(time1),
            time2: Some(time2),
            id: None,
            group: None,
            metadata: None,
            size_in_bytes: None,
        });
        current_offset += file_data.len();

        progress.data_chunks.current = index + 1;
        update_callback(progress.clone());
    }

    let mut add_event_chunk = |event: &Event, chunk_type: u32, index: usize, max_count: usize| {
        if index >= max_count || event.id.is_none() || event.group.is_none() {
            return;
        }
        let event_buffer = event
            .data
            .as_ref()
            .and_then(|edata| edata.typ.as_ref().filter(|&t| t == "Buffer").and(edata.data.clone()))
            .unwrap_or_default();

        download_chunks.push(Chunk {
            data: event_buffer.clone(),
            chunk_type,
            time1: event.time1.or(Some(0)),
            time2: event.time2.or(Some(0)),
            id: event.id.clone(),
            group: event.group.clone(),
            metadata: event.meta.clone(),
            size_in_bytes: None,
        });
        current_offset += event_buffer.len();
    };

    // Process Pavlov events
    for (index, event) in pavlov_events.iter().enumerate() {
        if index >= config.event_count {
            break;
        }
        add_event_chunk(event, 3, index, config.event_count);
        progress.event_chunks.current = index + 1;
        update_callback(progress.clone());
    }

    // Process checkpoint events
    for (index, event) in checkpoint_events.iter().enumerate() {
        if index >= config.checkpoint_count {
            break;
        }
        add_event_chunk(event, 2, index, config.checkpoint_count);
        progress.checkpoint_chunks.current = index + 1;
        update_callback(progress.clone());
    }

    let mut parts = vec![build_replay::ReplayPart::Meta(meta_buffer)];
    parts.extend(download_chunks.into_iter().map(build_replay::ReplayPart::Chunk));

    let replay = build_replay(&parts)?;
    let output_path = std::env::current_dir()?.join("processed_replay.replay");
    fs::write(&output_path, &replay)?;
    Ok(replay)
}

#[derive(Debug, Clone, PartialEq)]
enum Page {
    Main,
    ProcessLocal,
}

struct ReplayApp {
    progress: Arc<Mutex<Option<Progress>>>,
    status: Arc<Mutex<String>>,
    is_processing: bool,
    selected_path: Option<PathBuf>,
    show_completion_dialog: bool,
    current_page: Page,
    replay_list: ReplayListState,
}

impl ReplayApp {
    fn new(cc: &CreationContext<'_>) -> Self {
        Self {
            progress: Arc::new(Mutex::new(None)),
            status: Arc::new(Mutex::new("Idle".to_string())),
            is_processing: false,
            selected_path: None,
            show_completion_dialog: false,
            current_page: Page::Main,
            replay_list: ReplayListState::default(),
        }
    }

    fn refresh_replays(&mut self) {
        // TODO: Load replays from storage/API
    }

    fn render_replay_list(&mut self, ui: &mut egui::Ui) {
        // Filters
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label("Game Mode:");
                ui.text_edit_singleline(&mut self.replay_list.filters.game_mode);
                ui.label("Map:");
                ui.text_edit_singleline(&mut self.replay_list.filters.map_name);
                ui.label("Workshop Mods:");
                ui.text_edit_singleline(&mut self.replay_list.filters.workshop_mods);
            });
        });

        // Replay list
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                for replay in &self.replay_list.replays {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.heading(&replay.map_name);
                                ui.label(format!("Game Mode: {} | Date: {}", replay.game_mode, replay.created_date));
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if self.styled_button(ui, "Process").clicked() {
                                    // TODO: Process replay
                                }
                            });
                        });
                        ui.label(format!("Workshop Mods: {}", replay.workshop_mods));
                        ui.label(format!("Total Time: {}s", replay.total_time));
                    });
                    ui.add_space(8.0);
                }
            });

        // Pagination
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if self.styled_button(ui, "< Previous").clicked() && self.replay_list.current_page > 0 {
                    self.replay_list.current_page -= 1;
                    self.refresh_replays();
                }
                ui.label(format!("Page {} of {}",
                                 self.replay_list.current_page + 1,
                                 self.replay_list.total_pages.max(1)));
                if self.styled_button(ui, "Next >").clicked() &&
                    self.replay_list.current_page < self.replay_list.total_pages - 1 {
                    self.replay_list.current_page += 1;
                    self.refresh_replays();
                }
            });
        });
    }

    fn reset_state(&mut self) {
        self.is_processing = false;
        self.show_completion_dialog = false;
        if let Ok(mut progress) = self.progress.lock() {
            *progress = None;
        }
        if let Ok(mut status) = self.status.lock() {
            *status = "Idle".to_string();
        }
    }

    fn start_processing(&mut self) {
        if self.is_processing || self.selected_path.is_none() {
            return;
        }
        self.is_processing = true;

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
}

impl App for ReplayApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // Process completion dialog (keep at top level)
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

        // Top navigation bar
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
                        "Process Local"
                    )
                ).clicked().then(|| {
                    self.current_page = Page::ProcessLocal;
                });
            });
            ui.add_space(4.0);
            ui.separator();
        });

        // Main content area
        CentralPanel::default().show(ctx, |ui| {
            match self.current_page {
                Page::Main => self.render_main_page(ui),
                Page::ProcessLocal => self.render_process_page(ui),
            }
        });

        // Check processing status
        if self.is_processing {
            if let Ok(status) = self.status.lock() {
                if status.contains("complete") || status.contains("Error") {
                    self.show_completion_dialog = true;
                    self.is_processing = false;
                }
            }
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

impl ReplayApp {
    fn styled_button(&self, ui: &mut egui::Ui, text: &str) -> egui::Response {
        ui.add_sized(
            [ui.available_width().min(120.0), 32.0],
            egui::Button::new(text)
        )
    }

    fn render_main_page(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Available Replays");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.styled_button(ui, "Refresh").clicked() {
                    self.refresh_replays();
                }
            });
        });
        ui.separator();
    
        // Filters at the top
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label("Game Mode:");
                ui.add_sized([120.0, 24.0], 
                    egui::TextEdit::singleline(&mut self.replay_list.filters.game_mode));
                ui.label("Map:");
                ui.add_sized([120.0, 24.0], 
                    egui::TextEdit::singleline(&mut self.replay_list.filters.map_name));
                ui.label("Workshop Mods:");
                ui.add_sized([120.0, 24.0], 
                    egui::TextEdit::singleline(&mut self.replay_list.filters.workshop_mods));
            });
        });
    
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 8.0);
    
                for replay in &self.replay_list.replays {
                    egui::Frame::none()
                        .outer_margin(egui::style::Margin::symmetric(8.0, 4.0))
                        .show(ui, |ui| {
                            egui::Frame::group(ui.style())
                                .fill(ui.style().visuals.extreme_bg_color)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.set_min_width(ui.available_width() - 150.0);
                                        ui.vertical(|ui| {
                                            ui.label(egui::RichText::new(&replay.map_name)
                                                .strong()
                                                .size(16.0));
                                            ui.label(format!(
                                                "Game Mode: {} | Date: {}", 
                                                replay.game_mode, 
                                                replay.created_date
                                            ));
                                            ui.label(format!("Workshop Mods: {}", replay.workshop_mods));
                                            ui.label(format!("Total Time: {}s", replay.total_time));
                                        });
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            if self.styled_button(ui, "Download & Process").clicked() {
                                                // TODO: Handle replay processing
                                            }
                                        });
                                    });
                                });
                        });
                }
    
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if self.styled_button(ui, "< Previous").clicked() 
                            && self.replay_list.current_page > 0 
                        {
                            self.replay_list.current_page -= 1;
                            self.refresh_replays();
                        }
                        ui.label(format!("Page {} of {}", 
                            self.replay_list.current_page + 1,
                            self.replay_list.total_pages.max(1)
                        ));
                        if self.styled_button(ui, "Next >").clicked() 
                            && self.replay_list.current_page < self.replay_list.total_pages - 1 
                        {
                            self.replay_list.current_page += 1;
                            self.refresh_replays();
                        }
                    });
                });
            });
    }

    fn render_process_page(&mut self, ui: &mut egui::Ui) {
        ui.heading("Process Local Replay");
        ui.separator();

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                // Directory selection
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

                if !self.is_processing && !self.show_completion_dialog {
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

                // Progress indicators
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

                // Status message
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
}

fn main() -> eframe::Result<()> {
    let native_options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_min_inner_size([800.0, 600.0])
            .with_inner_size([1024.0, 768.0])
            .with_decorations(true)
            .with_drag_and_drop(true)
            .with_resizable(true)
            .with_title("Pavlov Replay Toolbox"),
        default_theme: eframe::Theme::Dark,
        follow_system_theme: true,
        centered: true,
        vsync: true,
        multisampling: 0,
        ..Default::default()
    };

    run_native(
        "Pavlov Replay Toolbox",
        native_options,
        Box::new(|cc| Box::new(ReplayApp::new(cc))),
    )
}
