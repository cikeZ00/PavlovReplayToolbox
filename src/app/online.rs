use std::{fs, sync::Arc, thread, time::Duration};

use reqwest::blocking::Client;

use crate::tools::replay_processor::{
    download_replay, download_replay_to_path, DownloadOptions, DownloadProgress, MetaData,
    API_BASE_URL,
};

use super::ReplayApp;

impl ReplayApp {
    pub fn queue_download(&mut self, replay_id: &str) {
        if self.downloaded_replays.contains(replay_id) {
            return;
        }

        if self.active_downloads.contains(replay_id) {
            return;
        }

        if self.download_queue.iter().any(|id| id == replay_id) {
            return;
        }

        self.download_errors.remove(replay_id);
        self.download_queue.push_back(replay_id.to_string());
        self.show_info(format!("Queued replay {}", replay_id));
    }

    pub(crate) fn start_queued_downloads(&mut self) {
        let max_concurrent = self.settings.download_concurrency.max(1);

        while self.active_downloads.len() < max_concurrent {
            let Some(next_id) = self.download_queue.pop_front() else {
                break;
            };

            self.start_download_task(next_id);
        }

        self.is_downloading = !self.active_downloads.is_empty();
    }

    fn start_download_task(&mut self, replay_id: String) {
        self.active_downloads.insert(replay_id.clone());
        self.is_downloading = true;

        let replay_id_clone = replay_id.clone();
        let status_clone = Arc::clone(&self.status);
        let progress_clone = Arc::clone(&self.download_progress);
        let downloaded_tx = self.downloaded_tx.clone();
        let download_error_tx = self.download_error_tx.clone();
        let download_dir = self.settings.download_dir.clone();
        let download_options = DownloadOptions {
            use_disk_cache: self.settings.download_use_disk_cache,
            cache_dir: self.settings.download_dir.join(".replay_cache"),
            max_parallel_downloads: self.settings.download_thread_count,
        };
        let retry_enabled = self.settings.download_retry_enabled;

        thread::spawn(move || {
            let client = match Client::builder().build() {
                Ok(client) => client,
                Err(e) => {
                    let _ = download_error_tx.send((
                        replay_id_clone.clone(),
                        format!("Failed to initialize HTTP client: {}", e),
                    ));
                    return;
                }
            };

            let mut retry_backoff = Duration::from_secs(3);
            let max_backoff = Duration::from_secs(30);

            loop {
                if let Ok(mut progress_map) = progress_clone.lock() {
                    progress_map
                        .entry(replay_id_clone.clone())
                        .or_insert_with(DownloadProgress::default);
                }

                if let Ok(mut status) = status_clone.lock() {
                    *status = format!("Downloading replay {}", replay_id_clone);
                }

                let result: Result<(), Box<dyn std::error::Error>> = (|| {
                    let download_progress_callback = {
                        let progress_clone = Arc::clone(&progress_clone);
                        let replay_id = replay_id_clone.clone();
                        Box::new(move |current: usize, total: usize| {
                            if let Ok(mut progress_map) = progress_clone.lock() {
                                let entry = progress_map
                                    .entry(replay_id.clone())
                                    .or_insert_with(DownloadProgress::default);
                                entry.download.current = current;
                                entry.download.max = total;
                            }
                        }) as Box<dyn Fn(usize, usize) + Send + Sync>
                    };
                    let build_progress_callback = {
                        let progress_clone = Arc::clone(&progress_clone);
                        let replay_id = replay_id_clone.clone();
                        Box::new(move |current: usize, total: usize| {
                            if let Ok(mut progress_map) = progress_clone.lock() {
                                let entry = progress_map
                                    .entry(replay_id.clone())
                                    .or_insert_with(DownloadProgress::default);
                                entry.build.current = current;
                                entry.build.max = total;
                            }
                        }) as Box<dyn Fn(usize, usize) + Send + Sync>
                    };

                    let metadata_result = match client
                        .get(&format!("{}/meta/{}", API_BASE_URL, replay_id_clone))
                        .send() {
                            Ok(resp) => {
                                if !resp.status().is_success() {
                                    return Err(format!(
                                        "Failed to fetch replay metadata: Server returned {} - {}",
                                        resp.status().as_u16(),
                                        resp.status().canonical_reason().unwrap_or("Unknown error")
                                    ).into());
                                }

                                match resp.json::<MetaData>() {
                                    Ok(data) => data,
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

                    let created_datetime = match chrono::DateTime::parse_from_rfc3339(&metadata_result.created)
                        .or_else(|_| -> Result<_, Box<dyn std::error::Error>> {
                            let ts = metadata_result.created
                                .parse::<i64>()
                                .map_err(|e| format!("Invalid timestamp format: {}", e))?;
                            chrono::DateTime::from_timestamp(ts, 0)
                                .map(|dt| dt.fixed_offset())
                                .ok_or_else(|| "Invalid timestamp".into())
                        }) {
                            Ok(dt) => dt,
                            Err(e) => return Err(format!("Failed to parse replay date: {}", e).into())
                        };

                    let formatted_date = created_datetime.format("%Y.%m.%d-%H.%M.%S");
                    let sanitized_name = metadata_result.friendly_name.replace([' ','<','>',':','"','/',',','\\','?','*','='], "-");
                    let filename = format!(
                        "{}-{}-{}({}).replay",
                        sanitized_name,
                        metadata_result.game_mode,
                        formatted_date,
                        replay_id_clone
                    );

                    let output_path = download_dir.join(filename);
                    let download_options = download_options.clone();

                    if download_options.use_disk_cache {
                        download_replay_to_path(
                            &replay_id_clone,
                            download_options,
                            &output_path,
                            Some(metadata_result.clone()),
                            Some(download_progress_callback),
                            Some(build_progress_callback),
                        )
                        .map_err(|e| format!("Failed to download replay data: {}", e))?;
                    } else {
                        let replay_data = download_replay(
                            &replay_id_clone,
                            download_options,
                            Some(download_progress_callback),
                            Some(build_progress_callback),
                        )
                        .map_err(|e| format!("Failed to download replay data: {}", e))?;
                        fs::write(output_path, replay_data)
                            .map_err(|e| format!("Failed to save replay file: {}", e))?;
                    }

                    Ok(())
                })();

                match result {
                    Ok(()) => {
                        let _ = downloaded_tx.send(replay_id_clone.clone());
                        if let Ok(mut status) = status_clone.lock() {
                            *status = "Replay downloaded and processed successfully.".to_string();
                        }
                        break;
                    }
                    Err(e) => {
                        if !retry_enabled {
                            let _ = download_error_tx.send((replay_id_clone.clone(), e.to_string()));
                            break;
                        }

                        if let Ok(mut status) = status_clone.lock() {
                            *status = format!(
                                "Retrying download {} after error: {}",
                                replay_id_clone, e
                            );
                        }

                        if let Ok(mut progress_map) = progress_clone.lock() {
                            progress_map.insert(replay_id_clone.clone(), DownloadProgress::default());
                        }

                        thread::sleep(retry_backoff);
                        retry_backoff = std::cmp::min(retry_backoff.saturating_mul(2), max_backoff);
                    }
                }
            }
        });
    }
}
