use chrono::DateTime;
use rayon::prelude::*;
use rayon::ThreadPoolBuilder;
use reqwest::blocking::Client;
use reqwest::StatusCode;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    error::Error,
    fs,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::Arc,
    thread::sleep,
    time::Duration,
};

use crate::tools::build_meta::build_meta;
use crate::tools::build_replay::{build_replay, write_part, ReplayPart};

pub const API_BASE_URL: &str = "https://tv.vankrupt.net";

#[derive(Debug, Clone, Default)]
pub struct DownloadProgress {
    pub download: ProgressUpdate,
    pub build: ProgressUpdate,
}

#[derive(Clone, Debug)]
pub struct DownloadOptions {
    pub use_disk_cache: bool,
    pub cache_dir: PathBuf,
    pub max_parallel_downloads: usize,
}

fn default_download_thread_count() -> usize {
    std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(4)
}

impl Default for DownloadOptions {
    fn default() -> Self {
        let base_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            use_disk_cache: true,
            cache_dir: base_dir.join(".replay_cache"),
            max_parallel_downloads: default_download_thread_count(),
        }
    }
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
    #[allow(dead_code)]
    pub competitive: bool,
    #[allow(dead_code)]
    pub modcount: i32,
    #[allow(dead_code)]
    pub shack: bool,
    pub workshop_mods: String,
    #[allow(dead_code)]
    pub live: bool,
    pub users: Vec<String>,
    pub expires_date: String,
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

#[derive(Clone, Copy)]
struct RetryConfig {
    max_retries: Option<u32>,
    initial_backoff: Duration,
    max_backoff: Duration,
}

impl RetryConfig {
    fn infinite() -> Self {
        Self {
            max_retries: None,
            initial_backoff: Duration::from_secs(2),
            max_backoff: Duration::from_secs(30),
        }
    }
}

fn next_backoff(current: Duration, max: Duration) -> Duration {
    let next = current.saturating_mul(2);
    if next > max {
        max
    } else {
        next
    }
}

fn max_retries_reached(retry: &RetryConfig, attempt: u32) -> bool {
    retry.max_retries.map(|max| attempt >= max).unwrap_or(false)
}

fn file_is_nonempty(path: &Path) -> bool {
    path.metadata().map(|meta| meta.len() > 0).unwrap_or(false)
}

fn file_suffix_path(path: &Path, suffix: &str) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("chunk");
    path.with_file_name(format!("{}.{}", file_name, suffix))
}

fn temp_path(path: &Path) -> PathBuf {
    file_suffix_path(path, "part")
}

fn timing_path(path: &Path) -> PathBuf {
    file_suffix_path(path, "timing")
}

fn write_atomic(path: &Path, data: &[u8]) -> Result<(), Box<dyn Error + Send + Sync>> {
    let tmp_path = temp_path(path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&tmp_path, data)?;
    if path.exists() {
        let _ = fs::remove_file(path);
    }
    fs::rename(&tmp_path, path)?;
    Ok(())
}

fn write_json_atomic<T: Serialize>(
    path: &Path,
    value: &T,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let data = serde_json::to_vec(value)?;
    write_atomic(path, &data)
}

fn read_json_cached<T: DeserializeOwned>(
    path: &Path,
) -> Result<Option<T>, Box<dyn Error + Send + Sync>> {
    if !path.exists() {
        return Ok(None);
    }

    let data = match fs::read(path) {
        Ok(data) => data,
        Err(_) => {
            let _ = fs::remove_file(path);
            return Ok(None);
        }
    };

    match serde_json::from_slice(&data) {
        Ok(value) => Ok(Some(value)),
        Err(_) => {
            let _ = fs::remove_file(path);
            Ok(None)
        }
    }
}

fn write_chunk_timing(
    path: &Path,
    time1: Option<i32>,
    time2: Option<i32>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut buf = [0u8; 8];
    let t1 = time1.unwrap_or(0);
    let t2 = time2.unwrap_or(0);
    buf[0..4].copy_from_slice(&t1.to_le_bytes());
    buf[4..8].copy_from_slice(&t2.to_le_bytes());
    write_atomic(path, &buf)
}

fn read_chunk_timing(path: &Path) -> Result<(i32, i32), Box<dyn Error + Send + Sync>> {
    let data = fs::read(path)?;
    if data.len() != 8 {
        return Err(format!("Invalid timing file length at {:?}", path).into());
    }
    let time1 = i32::from_le_bytes(data[0..4].try_into()?);
    let time2 = i32::from_le_bytes(data[4..8].try_into()?);
    Ok((time1, time2))
}

fn cleanup_partial_files(cache_dir: &Path) {
    if let Ok(entries) = fs::read_dir(cache_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.ends_with(".part") {
                    let _ = fs::remove_file(path);
                }
            }
        }
    }
}

fn should_retry_status(status: StatusCode) -> bool {
    status.is_server_error()
        || status == StatusCode::TOO_MANY_REQUESTS
        || status == StatusCode::REQUEST_TIMEOUT
}

fn get_json<T: DeserializeOwned>(
    client: &Client,
    url: &str,
    retry: &RetryConfig,
) -> Result<T, Box<dyn Error + Send + Sync>> {
    let mut attempt = 0;
    let mut backoff = retry.initial_backoff;
    loop {
        attempt += 1;
        match client.get(url).send() {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    match resp.json::<T>() {
                        Ok(value) => return Ok(value),
                        Err(e) => {
                            if max_retries_reached(retry, attempt) {
                                return Err(format!(
                                    "GET {} failed after {} attempts: {}",
                                    url, attempt, e
                                )
                                .into());
                            }
                        }
                    }
                } else if !should_retry_status(status)
                    || max_retries_reached(retry, attempt)
                {
                    return Err(format!("GET {} failed with status: {}", url, status).into());
                }
            }
            Err(e) => {
                if max_retries_reached(retry, attempt) {
                    return Err(format!("GET {} failed after {} attempts: {}", url, attempt, e).into());
                }
            }
        }

        sleep(backoff);
        backoff = next_backoff(backoff, retry.max_backoff);
    }
}

fn post_json<T: DeserializeOwned>(
    client: &Client,
    url: &str,
    retry: &RetryConfig,
) -> Result<T, Box<dyn Error + Send + Sync>> {
    let mut attempt = 0;
    let mut backoff = retry.initial_backoff;
    loop {
        attempt += 1;
        match client.post(url).send() {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    match resp.json::<T>() {
                        Ok(value) => return Ok(value),
                        Err(e) => {
                            if max_retries_reached(retry, attempt) {
                                return Err(format!(
                                    "POST {} failed after {} attempts: {}",
                                    url, attempt, e
                                )
                                .into());
                            }
                        }
                    }
                } else if !should_retry_status(status)
                    || max_retries_reached(retry, attempt)
                {
                    return Err(format!("POST {} failed with status: {}", url, status).into());
                }
            }
            Err(e) => {
                if max_retries_reached(retry, attempt) {
                    return Err(format!("POST {} failed after {} attempts: {}", url, attempt, e).into());
                }
            }
        }

        sleep(backoff);
        backoff = next_backoff(backoff, retry.max_backoff);
    }
}

fn get_bytes(
    client: &Client,
    url: &str,
    retry: &RetryConfig,
) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
    let mut attempt = 0;
    let mut backoff = retry.initial_backoff;
    loop {
        attempt += 1;
        match client.get(url).send() {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    match resp.bytes() {
                        Ok(bytes) => return Ok(bytes.to_vec()),
                        Err(e) => {
                            if max_retries_reached(retry, attempt) {
                                return Err(format!(
                                    "GET {} failed after {} attempts: {}",
                                    url, attempt, e
                                )
                                .into());
                            }
                        }
                    }
                } else if !should_retry_status(status)
                    || max_retries_reached(retry, attempt)
                {
                    return Err(format!("GET {} failed with status: {}", url, status).into());
                }
            }
            Err(e) => {
                if max_retries_reached(retry, attempt) {
                    return Err(format!("GET {} failed after {} attempts: {}", url, attempt, e).into());
                }
            }
        }

        sleep(backoff);
        backoff = next_backoff(backoff, retry.max_backoff);
    }
}

fn get_chunk(
    client: &Client,
    url: &str,
    retry: &RetryConfig,
) -> Result<(Vec<u8>, Option<i32>, Option<i32>), Box<dyn Error + Send + Sync>> {
    let mut attempt = 0;
    let mut backoff = retry.initial_backoff;
    loop {
        attempt += 1;
        match client.get(url).send() {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    let time1 = resp
                        .headers()
                        .get("mtime1")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse().ok());
                    let time2 = resp
                        .headers()
                        .get("mtime2")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse().ok());
                    match resp.bytes() {
                        Ok(bytes) => return Ok((bytes.to_vec(), time1, time2)),
                        Err(e) => {
                            if max_retries_reached(retry, attempt) {
                                return Err(format!(
                                    "GET {} failed after {} attempts: {}",
                                    url, attempt, e
                                )
                                .into());
                            }
                        }
                    }
                } else if !should_retry_status(status)
                    || max_retries_reached(retry, attempt)
                {
                    return Err(format!("GET {} failed with status: {}", url, status).into());
                }
            }
            Err(e) => {
                if max_retries_reached(retry, attempt) {
                    return Err(format!("GET {} failed after {} attempts: {}", url, attempt, e).into());
                }
            }
        }

        sleep(backoff);
        backoff = next_backoff(backoff, retry.max_backoff);
    }
}

pub fn download_replay(
    replay_id: &str,
    options: DownloadOptions,
    progress_callback: Option<Box<dyn Fn(usize, usize) + Send + Sync>>,
) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
    // Validate replay id (only accept alphanumeric IDs)
    if !replay_id.chars().all(|c| c.is_alphanumeric()) {
        return Err("Invalid replay id".into());
    }

    const SERVER: &str = API_BASE_URL;
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    let retry = RetryConfig::infinite();
    let optional_retry = RetryConfig::infinite();
    let max_parallel = options.max_parallel_downloads.max(1);

    if options.use_disk_cache {
        return Err("Disk cache enabled; use download_replay_to_path".into());
    }
    
    let mut offset = 0;
    let mut find_all_response = None;

    // Loop through available pages to find the matching replay.
    while find_all_response.is_none() {
        let url = format!("{}/find/?game=all&offset={}&live=false", SERVER, offset);
        let find_all: ApiResponse = get_json(&client, &url, &retry)?;

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
    
    let _ = find_all_response.ok_or("Recording not available")?;
    let start_url = format!("{}/replay/{}/startDownloading?user", SERVER, replay_id);
    let start_download: serde_json::Value = post_json(&client, &start_url, &retry)?;

    if start_download["state"] != "Recorded" {
        return Err("Recording must be finished before download".into());
    }
    
    let num_chunks = start_download["numChunks"].as_i64().unwrap_or(0) as usize;
    
    // Calculate total number of components: header + chunks + metadata sets
    let total_components = num_chunks + 4; // Header + numChunks + meta + events + events_pavlov
    let mut completed_components = 0;
    
    // Function to update progress
    let update_progress = |step: usize| {
        if let Some(callback) = &progress_callback {
            callback(step, total_components);
        }
    };
    
    // Report initial progress
    update_progress(completed_components);
    
    // Download header (in-memory)
    let header_url = format!("{}/replay/{}/file/replay.header", SERVER, replay_id);
    let header_data = get_bytes(&client, &header_url, &retry)?;

    completed_components += 1;
    update_progress(completed_components);

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

    // Get metadata
    let meta: MetaData = get_json(
        &client,
        &format!("{}/meta/{}", SERVER, replay_id),
        &retry,
    )?;

    completed_components += 1;
    update_progress(completed_components);

    // Get events
    let events: EventsWrapper = match get_json(
        &client,
        &format!("{}/replay/{}/event?group=checkpoint", SERVER, replay_id),
        &optional_retry,
    )
    {
        Ok(events) => events,
        Err(_) => EventsWrapper { events: Vec::new() },
    };

    completed_components += 1;
    update_progress(completed_components);

    // Get Pavlov events
    let events_pavlov: EventsWrapper = get_json(
        &client,
        &format!("{}/replay/{}/event?group=Pavlov", SERVER, replay_id),
        &retry,
    )?;

    completed_components += 1;
    update_progress(completed_components);

    // Use atomic counter for thread-safe progress tracking
    use std::sync::atomic::{AtomicUsize, Ordering};
    let downloaded_chunks = Arc::new(AtomicUsize::new(0));

    let pool = ThreadPoolBuilder::new()
        .num_threads(max_parallel)
        .build()
        .map_err(|e| format!("Failed to create download thread pool: {}", e))?;

    // Download stream chunks in parallel
    let mut stream_chunks: Vec<(usize, Chunk)> = pool
        .install(|| {
            (0..num_chunks)
                .into_par_iter()
                .map(|i| {
                    let chunk_url = format!("{}/replay/{}/file/stream.{}", SERVER, replay_id, i);

                    // Each parallel thread uses the same client instance.
                    let (chunk_data, time1, time2) = get_chunk(&client, &chunk_url, &retry)?;

                    // Update progress after each chunk is downloaded
                    let downloaded = downloaded_chunks.fetch_add(1, Ordering::SeqCst) + 1;
                    if let Some(callback) = &progress_callback {
                        callback(completed_components + downloaded, total_components);
                    }

                    Ok((
                        i,
                        Chunk {
                            data: chunk_data,
                            chunk_type: 1,
                            time1,
                            time2,
                            id: None,
                            group: None,
                            metadata: None,
                            size_in_bytes: None,
                        },
                    ))
                })
                .collect::<Result<Vec<_>, Box<dyn Error + Send + Sync>>>()
        })?;

    stream_chunks.sort_by_key(|(i, _)| *i);
    download_chunks.extend(stream_chunks.into_iter().map(|(_, chunk)| chunk));

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
    let meta_buffer = build_meta(&meta)
        .map_err(|e| -> Box<dyn Error + Send + Sync> { e.to_string().into() })?;
    let mut parts = vec![ReplayPart::Meta(meta_buffer)];
    parts.extend(download_chunks.into_iter().map(ReplayPart::Chunk));

    // Final progress update
    update_progress(total_components);

    build_replay(parts)
        .map_err(|e| -> Box<dyn Error + Send + Sync> { e.to_string().into() })
}

pub fn download_replay_to_path(
    replay_id: &str,
    options: DownloadOptions,
    output_path: &Path,
    meta_override: Option<MetaData>,
    progress_callback: Option<Box<dyn Fn(usize, usize) + Send + Sync>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Validate replay id (only accept alphanumeric IDs)
    if !replay_id.chars().all(|c| c.is_alphanumeric()) {
        return Err("Invalid replay id".into());
    }

    if !options.use_disk_cache {
        return Err("Disk cache disabled; use download_replay".into());
    }

    const SERVER: &str = API_BASE_URL;
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    let retry = RetryConfig::infinite();
    let max_parallel = options.max_parallel_downloads.max(1);

    let mut offset = 0;
    let mut find_all_response = None;

    // Loop through available pages to find the matching replay.
    while find_all_response.is_none() {
        let url = format!("{}/find/?game=all&offset={}&live=false", SERVER, offset);
        let find_all: ApiResponse = get_json(&client, &url, &retry)?;

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

    let _ = find_all_response.ok_or("Recording not available")?;
    let start_url = format!("{}/replay/{}/startDownloading?user", SERVER, replay_id);
    let start_download: serde_json::Value = post_json(&client, &start_url, &retry)?;

    if start_download["state"] != "Recorded" {
        return Err("Recording must be finished before download".into());
    }

    let num_chunks = start_download["numChunks"].as_i64().unwrap_or(0) as usize;

    // Calculate total number of components: header + chunks + metadata sets
    let total_components = num_chunks + 4; // Header + numChunks + meta + events + events_pavlov
    let mut completed_components = 0;

    // Function to update progress
    let update_progress = |step: usize| {
        if let Some(callback) = &progress_callback {
            callback(step, total_components);
        }
    };

    // Report initial progress
    update_progress(completed_components);

    let cache_dir = options.cache_dir.join(replay_id);
    fs::create_dir_all(&cache_dir)?;
    cleanup_partial_files(&cache_dir);

    // Download header
    let header_url = format!("{}/replay/{}/file/replay.header", SERVER, replay_id);
    let header_path = cache_dir.join("replay.header");
    if !file_is_nonempty(&header_path) {
        let header_data = get_bytes(&client, &header_url, &retry)?;
        write_atomic(&header_path, &header_data)?;
    }

    completed_components += 1;
    update_progress(completed_components);

    // Get metadata
    let meta_path = cache_dir.join("meta.json");
    let meta: MetaData = if let Some(meta_override) = meta_override {
        let _ = write_json_atomic(&meta_path, &meta_override);
        meta_override
    } else if let Some(cached) = read_json_cached(&meta_path)? {
        cached
    } else {
        let meta = get_json(&client, &format!("{}/meta/{}", SERVER, replay_id), &retry)?;
        write_json_atomic(&meta_path, &meta)?;
        meta
    };

    completed_components += 1;
    update_progress(completed_components);

    // Get events (checkpoint)
    let events_path = cache_dir.join("events_checkpoint.json");
    let events: EventsWrapper = if let Some(cached) = read_json_cached(&events_path)? {
        cached
    } else {
        let events = get_json(
            &client,
            &format!("{}/replay/{}/event?group=checkpoint", SERVER, replay_id),
            &retry,
        )?;
        let _ = write_json_atomic(&events_path, &events);
        events
    };

    completed_components += 1;
    update_progress(completed_components);

    // Get Pavlov events
    let events_pavlov_path = cache_dir.join("events_pavlov.json");
    let events_pavlov: EventsWrapper = if let Some(cached) = read_json_cached(&events_pavlov_path)? {
        cached
    } else {
        let events_pavlov = get_json(
            &client,
            &format!("{}/replay/{}/event?group=Pavlov", SERVER, replay_id),
            &retry,
        )?;
        write_json_atomic(&events_pavlov_path, &events_pavlov)?;
        events_pavlov
    };

    completed_components += 1;
    update_progress(completed_components);

    // Use atomic counter for thread-safe progress tracking
    use std::sync::atomic::{AtomicUsize, Ordering};
    let downloaded_chunks = Arc::new(AtomicUsize::new(0));
    let mut missing_chunks = Vec::new();

    for i in 0..num_chunks {
        let chunk_path = cache_dir.join(format!("stream.{}", i));
        let chunk_timing_path = timing_path(&chunk_path);
        let chunk_valid = file_is_nonempty(&chunk_path) && file_is_nonempty(&chunk_timing_path);
        if chunk_valid {
            if read_chunk_timing(&chunk_timing_path).is_ok() {
                downloaded_chunks.fetch_add(1, Ordering::SeqCst);
                continue;
            }
        }

        let _ = fs::remove_file(&chunk_path);
        let _ = fs::remove_file(&chunk_timing_path);
        missing_chunks.push(i);
    }

    if let Some(callback) = &progress_callback {
        let cached = downloaded_chunks.load(Ordering::SeqCst);
        callback(completed_components + cached, total_components);
    }

    if !missing_chunks.is_empty() {
        let pool = ThreadPoolBuilder::new()
            .num_threads(max_parallel)
            .build()
            .map_err(|e| format!("Failed to create download thread pool: {}", e))?;

        pool.install(|| {
            missing_chunks
                .into_par_iter()
                .map(|i| {
                    let chunk_url = format!("{}/replay/{}/file/stream.{}", SERVER, replay_id, i);
                    let (chunk_data, time1, time2) = get_chunk(&client, &chunk_url, &retry)?;

                    let chunk_path = cache_dir.join(format!("stream.{}", i));
                    let chunk_timing_path = timing_path(&chunk_path);
                    write_atomic(&chunk_path, &chunk_data)?;
                    write_chunk_timing(&chunk_timing_path, time1, time2)?;

                    let downloaded = downloaded_chunks.fetch_add(1, Ordering::SeqCst) + 1;
                    if let Some(callback) = &progress_callback {
                        callback(completed_components + downloaded, total_components);
                    }

                    Ok(())
                })
                .collect::<Result<Vec<_>, Box<dyn Error + Send + Sync>>>()
        })?;
    }

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let output_temp = temp_path(output_path);
    if output_temp.exists() {
        let _ = fs::remove_file(&output_temp);
    }

    let output_file = fs::File::create(&output_temp)?;
    let mut writer = BufWriter::new(output_file);
    let mut write = |part: ReplayPart| -> Result<(), Box<dyn Error + Send + Sync>> {
        write_part(&mut writer, &part)
            .map_err(|e| -> Box<dyn Error + Send + Sync> { e.to_string().into() })
    };

    // Build the replay by first constructing the meta buffer and then appending each chunk.
    let meta_buffer = build_meta(&meta)
        .map_err(|e| -> Box<dyn Error + Send + Sync> { e.to_string().into() })?;
    write(ReplayPart::Meta(meta_buffer))?;

    let header_data = fs::read(&header_path)?;
    write(ReplayPart::Chunk(Chunk {
        data: header_data,
        chunk_type: 0,
        time1: None,
        time2: None,
        id: None,
        group: None,
        metadata: None,
        size_in_bytes: None,
    }))?;

    for i in 0..num_chunks {
        let chunk_path = cache_dir.join(format!("stream.{}", i));
        let chunk_timing_path = timing_path(&chunk_path);
        let chunk_data = fs::read(&chunk_path)?;
        let (time1, time2) = read_chunk_timing(&chunk_timing_path)?;
        write(ReplayPart::Chunk(Chunk {
            data: chunk_data,
            chunk_type: 1,
            time1: Some(time1),
            time2: Some(time2),
            id: None,
            group: None,
            metadata: None,
            size_in_bytes: None,
        }))?;
    }

    for event in events.events {
        if let Some(data) = event.data.and_then(|d| d.data) {
            write(ReplayPart::Chunk(Chunk {
                data,
                chunk_type: 2,
                time1: event.time1,
                time2: event.time2,
                id: event.id,
                group: event.group,
                metadata: event.meta,
                size_in_bytes: None,
            }))?;
        }
    }

    for event in events_pavlov.events {
        if let Some(data) = event.data.and_then(|d| d.data) {
            write(ReplayPart::Chunk(Chunk {
                data,
                chunk_type: 3,
                time1: event.time1,
                time2: event.time2,
                id: event.id,
                group: event.group,
                metadata: event.meta,
                size_in_bytes: None,
            }))?;
        }
    }

    writer.flush()?;

    if output_path.exists() {
        fs::remove_file(output_path)?;
    }
    fs::rename(&output_temp, output_path)?;

    // Final progress update
    update_progress(total_components);

    fs::remove_dir_all(&cache_dir)?;
    Ok(())
}

pub fn replay_chunks_dir() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    exe_dir.join("replay_chunks")
}

pub fn load_json_file<T: for<'de> Deserialize<'de>>(file_path: &Path, file_name: &str) -> Result<T, Box<dyn Error>> {
    if !file_path.exists() {
        return Err(format!("{} file not found at {:?}", file_name, file_path).into());
    }
    let content = fs::read_to_string(file_path)?;
    let parsed = serde_json::from_str(&content)?;
    Ok(parsed)
}

pub fn load_chunk_file(file_path: &Path) -> Result<Vec<u8>, Box<dyn Error>> {
    if !file_path.exists() {
        return Err(format!("Chunk file not found: {:?}", file_path).into());
    }
    Ok(fs::read(file_path)?)
}

pub fn process_replay(config: Option<Config>) -> Result<Vec<u8>, Box<dyn Error>> {
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
    let mut progress = Progress {
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

    let replay = build_replay(parts)?;
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