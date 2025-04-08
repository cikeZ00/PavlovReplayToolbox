#![windows_subsystem = "windows"]
mod build_meta;
mod build_replay;
mod replay_buffer;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
};

use eframe::egui::{self, CentralPanel, Context};
use eframe::{run_native, App, CreationContext, NativeOptions};
use serde::{Deserialize, Serialize};

use build_meta::build_meta;
use build_replay::build_replay;

use reqwest::blocking::Client;
use std::time::Duration;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use chrono::DateTime;

const API_BASE_URL: &str = "https://tv.vankrupt.net";

#[derive(Debug, Clone, Default)]
struct DownloadProgress {
    download: ProgressUpdate,
    build: ProgressUpdate,
}

#[derive(Deserialize, Serialize)]
struct ApiResponse {
    replays: Vec<ApiReplay>,
    total: i32,
}

#[derive(Deserialize, Serialize)]
#[derive(Clone)]
struct ApiReplay {
    #[serde(rename = "_id")]
    id: String,
    #[serde(rename = "gameMode")]
    game_mode: String,
    #[serde(rename = "friendlyName")]
    map_name: String,
    shack: bool,
    created: String,
    expires: String,
    #[serde(rename = "secondsSince")]
    time_since: i32,
    workshop_mods: String,
    competitive: bool,
    live: bool,
    users: Option<Vec<String>>,
    modcount: i32,
}

#[derive(Debug, Clone)]
struct ReplayItem {
    id: String,
    game_mode: String,
    map_name: String,
    created_date: String,
    time_since: i32,
    competitive: bool,
    modcount: i32,
    shack: bool,
    workshop_mods: String,
    live: bool,
    users: Vec<String>,
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

#[derive(Deserialize, Serialize, Debug)]
struct MetadataFile {
    meta: Option<MetaData>,
    #[serde(rename = "events_pavlov")]
    events_pavlov: Option<EventsWrapper>,
    events: Option<EventsWrapper>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
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

#[derive(Deserialize, Serialize)]
#[derive(Debug)]
struct EventsWrapper {
    events: Vec<Event>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
struct Event {
    id: Option<String>,
    group: Option<String>,
    meta: Option<String>,
    time1: Option<i32>,
    time2: Option<i32>,
    data: Option<EventData>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
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

fn download_replay(replay_id: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    // Validate replay id (only accept alphanumeric IDs, similar to Python's isalnum check)
    if !replay_id.chars().all(|c| c.is_alphanumeric()) {
        return Err("Invalid replay id".into());
    }

    // Use the constant for API base URL
    const SERVER: &str = API_BASE_URL;
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    // This will serve as our in-memory "dictionary" (like the Python replay_data)
    let mut replay_data = serde_json::Map::new();
    let mut offset = 0;
    let mut find_all_response = None;

    // Loop through available pages to find the matching replay.
    while find_all_response.is_none() {
        let find_all: ApiResponse = client
            .get(&format!("{}/find/?game=all&offset={}&live=false", SERVER, offset))
            .send()?
            .json()?;

        // Find the replay within the current response.
        find_all_response = find_all
            .replays
            .iter()
            .find(|r| r.id == replay_id)
            .cloned();

        if offset >= find_all.total as usize {
            break;
        }
        offset += 100;
    }

    // Ensure we found the replay; otherwise return an error.
    let replay_info = find_all_response.ok_or("Recording not available")?;
    replay_data.insert("find".into(), serde_json::to_value(&replay_info)?);

    // Start the download process
    let start_download: serde_json::Value = client
        .post(&format!("{}/replay/{}/startDownloading?user", SERVER, replay_id))
        .send()?
        .json()?;
    replay_data.insert("start_downloading".into(), start_download.clone());

    if start_download["state"] != "Recorded" {
        return Err("Recording must be finished before download".into());
    }

    // Fetch metadata and event data
    let meta: MetaData = client
        .get(&format!("{}/meta/{}", SERVER, replay_id))
        .send()?
        .json()?;
    replay_data.insert("meta".into(), serde_json::to_value(&meta)?);

    let events: EventsWrapper = client
        .get(&format!("{}/replay/{}/event?group=checkpoint", SERVER, replay_id))
        .send()?
        .json()?;
    replay_data.insert("events".into(), serde_json::to_value(&events)?);

    let events_pavlov: EventsWrapper = client
        .get(&format!("{}/replay/{}/event?group=Pavlov", SERVER, replay_id))
        .send()?
        .json()?;
    replay_data.insert("events_pavlov".into(), serde_json::to_value(&events_pavlov)?);

    // Download the header, keeping the bytes in memory
    let header_data = client
        .get(&format!("{}/replay/{}/file/replay.header", SERVER, replay_id))
        .send()?
        .bytes()?
        .to_vec();

    let mut download_chunks = Vec::new();
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

    // Download stream chunks into memory and capture header timing info
    let num_chunks = start_download["numChunks"].as_i64().unwrap_or(0) as usize;
    for i in 0..num_chunks {
        let response = client
            .get(&format!("{}/replay/{}/file/stream.{}", SERVER, replay_id, i))
            .send()?;

        let time1 = response.headers()
            .get("mtime1")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse().ok());
        let time2 = response.headers()
            .get("mtime2")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse().ok());

        let chunk_data = response.bytes()?.to_vec();
        download_chunks.push(Chunk {
            data: chunk_data,
            chunk_type: 1,
            time1,
            time2,
            id: None,
            group: None,
            metadata: None,
            size_in_bytes: None,
        });
    }

    // Process events from both groups and add them as chunks.
    for event in events.events {
        if let Some(data) = event.data.and_then(|d| d.data) {
            download_chunks.push(Chunk {
                data,
                chunk_type: 2,
                time1: event.time1,
                time2: event.time2,
                id: event.id,
                group: event.group,
                metadata: event.meta,
                size_in_bytes: None,
            });
        }
    }

    for event in events_pavlov.events {
        if let Some(data) = event.data.and_then(|d| d.data) {
            download_chunks.push(Chunk {
                data,
                chunk_type: 3,
                time1: event.time1,
                time2: event.time2,
                id: event.id,
                group: event.group,
                metadata: event.meta,
                size_in_bytes: None,
            });
        }
    }

    // Build the replay by first constructing the meta buffer and then appending each chunk.
    let meta_buffer = build_meta(&meta)?;
    let mut parts = vec![build_replay::ReplayPart::Meta(meta_buffer)];
    parts.extend(download_chunks.into_iter().map(build_replay::ReplayPart::Chunk));

    // Finally, build the replay and return its bytes—all in memory.
    build_replay(&parts)
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
    let created_datetime = DateTime::parse_from_rfc3339(&meta.created)
        .or_else(|_| -> Result<_, Box<dyn Error>> {
            let ts = meta.created
                .parse::<i64>()
                .map_err(|e| Box::new(e) as Box<dyn Error>)?;
            DateTime::from_timestamp(ts, 0)
                .map(|dt| dt.fixed_offset())
                .ok_or_else(|| "Invalid timestamp".into())
        })?;

    let formatted_date = created_datetime.format("%Y.%m.%d-%H.%M.%S");
    let sanitized_name = meta.friendly_name.replace([' ', '/', '\\', ':'], "-");
    let filename = format!("{}-{}-{}.replay", sanitized_name, meta.game_mode, formatted_date);
    let output_path = std::env::current_dir()?.join(filename);
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
    profile_textures: HashMap<String, egui::TextureHandle>,
    loading_profiles: HashSet<String>,
    profile_tx: std::sync::mpsc::Sender<(String, egui::ColorImage)>,
    profile_rx: std::sync::mpsc::Receiver<(String, egui::ColorImage)>,
    download_progress: Arc<Mutex<Option<DownloadProgress>>>,
    downloading_replay_id: Option<String>,
}

impl ReplayApp {
    fn new(cc: &CreationContext<'_>) -> Self {
        let (profile_tx, profile_rx) = std::sync::mpsc::channel();
        let mut app = Self {
            progress: Arc::new(Mutex::new(None)),
            status: Arc::new(Mutex::new("Loading replays...".to_string())),
            is_processing: false,
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
        };
        app.refresh_replays();
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
            "{}/find/?game=all&offset={}&shack=true&live=false",
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

fn refresh_replays(&mut self) {
        if let Ok(mut status) = self.status.lock() {
            *status = "Loading replays...".to_string();
        }

        match self.fetch_replays() {
            Ok(_) => {
                if let Ok(mut status) = self.status.lock() {
                    *status = "Replays loaded successfully".to_string();
                }
            }
            Err(e) => {
                if let Ok(mut status) = self.status.lock() {
                    *status = format!("Error loading replays: {}", e);
                }
            }
        }
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
                        ui.label(format!("Time Since: {}s", replay.time_since));
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
                Page::Main => self.render_main_page(ui, ctx),
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

        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

impl ReplayApp {
    fn process_online_replay(&mut self, replay_id: &str) {
        self.is_processing = true;
        self.downloading_replay_id = Some(replay_id.to_string());

        let replay_id = replay_id.to_string();
        let status_clone = Arc::clone(&self.status);
        let progress_clone = Arc::clone(&self.download_progress);

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

            let result: Result<(), Box<dyn Error>> = (|| {
                update_progress(current_step, total_steps, false);

                for step in 0..total_steps {
                    current_step = step;
                    update_progress(current_step, total_steps, false);
                    thread::sleep(Duration::from_millis(100));
                }

                let replay_data = download_replay(&replay_id)?;

                update_progress(0, 100, true);
                for i in 0..100 {
                    thread::sleep(Duration::from_millis(10));
                    update_progress(i + 1, 100, true);
                }

                // Fetch metadata
                let metadata_result = client
                    .get(&format!("{}/meta/{}", API_BASE_URL, replay_id))
                    .send()?
                    .json::<MetaData>()?;

                let created_datetime = DateTime::parse_from_rfc3339(&metadata_result.created)
                    .or_else(|_| -> Result<_, Box<dyn Error>> {
                        let ts = metadata_result.created
                            .parse::<i64>()
                            .map_err(|e| Box::new(e) as Box<dyn Error>)?;
                        DateTime::from_timestamp(ts, 0)
                            .map(|dt| dt.fixed_offset())
                            .ok_or_else(|| "Invalid timestamp".into())
                    })?;

                let formatted_date = created_datetime.format("%Y.%m.%d-%H.%M.%S");
                let sanitized_name = metadata_result.friendly_name.replace([' ', '/', '\\', ':'], "-");
                let filename = format!(
                    "{}-{}-{}.replay",
                    sanitized_name,
                    metadata_result.game_mode,
                    formatted_date
                );
                let output_path = std::env::current_dir()?.join(filename);

                fs::write(output_path, replay_data)?;

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


    // Add this to the update() method after the completion dialog
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

    // Note: The function signature now requires a &egui::Context reference
    // so that we can set clipboard contents and access textures.
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

                // Clone the replays to avoid borrow checker issues
                let replays = self.replay_list.replays.clone();
                for replay in &replays {
                    self.render_replay_item(ui, ctx, replay);
                }

                // Pagination controls
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

    fn render_replay_item(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, replay: &ReplayItem) {
        ui.push_id(replay.id.as_str(), |ui| {
            egui::Frame::none()
                .outer_margin(egui::style::Margin::symmetric(8.0, 4.0))
                .show(ui, |ui| {
                    egui::Frame::group(ui.style())
                        .fill(ui.style().visuals.extreme_bg_color)
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                // Top section with map name and button
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(&replay.map_name)
                                        .strong()
                                        .size(16.0));

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        let is_downloading = self.downloading_replay_id
                                            .as_ref()
                                            .map_or(false, |id| id == &replay.id);

                                        if !is_downloading &&
                                            self.styled_button(ui, "Download & Process").clicked() {
                                            self.process_online_replay(&replay.id);
                                        }
                                    });
                                });
    
                                // Rest of the rendering code remains the same
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
                                    ui.label("Total Time:");
                                    ui.label(format!("{}s", replay.time_since));
                                });
    
                                ui.separator();
    
                                egui::ScrollArea::horizontal()
                                    .id_source(format!("scroll_{}", replay.id))
                                    .max_height(72.0)
                                    .show(ui, |ui| {
                                        ui.horizontal_wrapped(|ui| {
                                            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
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
    }

    fn render_user_avatar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, user: &str) {
        if let Some(texture) = self.profile_textures.get(user) {
            if ui.add_sized(egui::vec2(64.0, 64.0), egui::ImageButton::new(texture)).clicked() {
                ctx.output_mut(|out| {
                    out.copied_text = user.to_string();
                });
            }
        } else {
            if ui.add_sized(egui::vec2(64.0, 64.0), egui::Button::new("Loading")).clicked() {
                ctx.output_mut(|out| {
                    out.copied_text = user.to_string();
                });
            }
            if !self.loading_profiles.contains(user) {
                self.load_profile(user.to_string());
            }
        }
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
    let icon_data = image::load_from_memory(include_bytes!("../assets/icon.png"))
        .expect("Failed to load icon")
        .to_rgba8();
    let (icon_width, icon_height) = icon_data.dimensions();

    let native_options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_min_inner_size([800.0, 600.0])
            .with_inner_size([1024.0, 768.0])
            .with_decorations(true)
            .with_drag_and_drop(true)
            .with_resizable(true)
            .with_title("Pavlov Replay Toolbox")
            .with_icon(egui::IconData {
                rgba: icon_data.into_raw(),
                width: icon_width,
                height: icon_height,
            }),
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