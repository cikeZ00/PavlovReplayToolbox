use std::{
    fs,
    path::{Path, PathBuf},
    error::Error,
    collections::HashMap,
};
use serde::{Deserialize, Serialize};
use reqwest::blocking::Client;
use std::time::Duration;
use chrono::DateTime;

use crate::tools::build_meta::{self, build_meta};
use crate::tools::build_replay::{self, build_replay, ReplayPart};

pub const API_BASE_URL: &str = "https://tv.vankrupt.net";

#[derive(Debug, Clone, Default)]
pub struct DownloadProgress {
    pub download: ProgressUpdate,
    pub build: ProgressUpdate,
}

#[derive(Deserialize, Serialize)]
pub struct ApiResponse {
    pub replays: Vec<ApiReplay>,
    pub total: i32,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct ApiReplay {
    #[serde(rename = "_id")]
    pub id: String,
    #[serde(rename = "gameMode")]
    pub game_mode: String,
    #[serde(rename = "friendlyName")]
    pub map_name: String,
    pub shack: bool,
    pub created: String,
    pub expires: String,
    #[serde(rename = "secondsSince")]
    pub time_since: i32,
    pub workshop_mods: String,
    pub competitive: bool,
    pub live: bool,
    pub users: Option<Vec<String>>,
    pub modcount: i32,
}

#[derive(Debug, Clone)]
pub struct ReplayItem {
    pub id: String,
    pub game_mode: String,
    pub map_name: String,
    pub created_date: String,
    pub time_since: i32,
    pub competitive: bool,
    pub modcount: i32,
    pub shack: bool,
    pub workshop_mods: String,
    pub live: bool,
    pub users: Vec<String>,
}

pub struct Config {
    pub update_callback: Box<dyn Fn(Progress) + Send + Sync>,
    pub data_count: usize,
    pub event_count: usize,
    pub checkpoint_count: usize,
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
pub struct Progress {
    pub header: ProgressUpdate,
    pub data_chunks: ProgressUpdate,
    pub event_chunks: ProgressUpdate,
    pub checkpoint_chunks: ProgressUpdate,
}

#[derive(Debug, Clone, Default)]
pub struct ProgressUpdate {
    pub current: usize,
    pub max: usize,
}

impl ProgressUpdate {
    pub fn progress(&self) -> f32 {
        if self.max == 0 {
            return 0.0;
        }
        self.current as f32 / self.max as f32
    }
}

#[derive(Deserialize, Serialize, Debug)]
pub struct MetadataFile {
    pub meta: Option<MetaData>,
    #[serde(rename = "events_pavlov")]
    pub events_pavlov: Option<EventsWrapper>,
    pub events: Option<EventsWrapper>,
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

#[derive(Deserialize, Serialize, Debug)]
pub struct EventsWrapper {
    pub events: Vec<Event>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct Event {
    pub id: Option<String>,
    pub group: Option<String>,
    pub meta: Option<String>,
    pub time1: Option<i32>,
    pub time2: Option<i32>,
    pub data: Option<EventData>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct EventData {
    #[serde(rename = "type")]
    pub typ: Option<String>,
    pub data: Option<Vec<u8>>,
}

#[derive(Deserialize)]
pub struct TimingEntry {
    pub numchunks: String,
    pub mtime1: String,
    pub mtime2: String,
}

#[derive(Debug)]
pub struct Chunk {
    pub data: Vec<u8>,
    pub chunk_type: u32,
    pub time1: Option<i32>,
    pub time2: Option<i32>,
    pub id: Option<String>,
    pub group: Option<String>,
    pub metadata: Option<String>,
    pub size_in_bytes: Option<i32>,
}

pub fn download_replay(replay_id: &str) -> Result<Vec<u8>, Box<dyn Error>> {
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
    let mut parts = vec![ReplayPart::Meta(meta_buffer)];
    parts.extend(download_chunks.into_iter().map(ReplayPart::Chunk));

    // Finally, build the replay and return its bytes—all in memory.
    build_replay(&parts)
}

pub fn replay_chunks_dir() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    exe_dir.join("replay_chunks")
}

pub fn load_json_file<T: for<'de> Deserialize<'de>>(file_path: &Path, file_name: &str) -> Result<T, Box<dyn std::error::Error>> {
    if !file_path.exists() {
        return Err(format!("{} file not found at {:?}", file_name, file_path).into());
    }
    let content = fs::read_to_string(file_path)?;
    let parsed = serde_json::from_str(&content)?;
    Ok(parsed)
}

pub fn load_chunk_file(file_path: &Path) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if !file_path.exists() {
        return Err(format!("Chunk file not found: {:?}", file_path).into());
    }
    Ok(fs::read(file_path)?)
}

pub fn process_replay(config: Option<Config>) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
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

    let mut parts = vec![ReplayPart::Meta(meta_buffer)];
    parts.extend(download_chunks.into_iter().map(ReplayPart::Chunk));

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