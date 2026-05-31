use std::{fs, sync::Arc, thread};

use reqwest::blocking::Client;

use crate::tools::replay_processor::{
    download_replay, download_replay_to_path, DownloadOptions, DownloadProgress, MetaData,
    API_BASE_URL,
};

use super::ReplayApp;

impl ReplayApp {
    pub fn process_online_replay(&mut self, replay_id: &str) {
        self.is_downloading = true;
        self.downloading_replay_id = Some(replay_id.to_string());
        self.show_info(format!("Downloading replay {}", replay_id));

        let replay_id_clone = replay_id.to_string();
        let status_clone = Arc::clone(&self.status);
        let progress_clone = Arc::clone(&self.download_progress);
        let downloaded_tx = self.downloaded_tx.clone();
        let download_dir = self.settings.download_dir.clone();
        let download_options = DownloadOptions {
            use_disk_cache: self.settings.download_use_disk_cache,
            cache_dir: self.settings.download_dir.join(".replay_cache"),
            max_parallel_downloads: self.settings.download_thread_count,
        };

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
                let sanitized_name = metadata_result.friendly_name.replace([' ','<','>',':','"','/',',','\\','?','*','='], "-");
                let filename = format!(
                    "{}-{}-{}({}).replay",
                    sanitized_name,
                    metadata_result.game_mode,
                    formatted_date,
                    replay_id_clone
                );

                update_build_progress(75, 100);

                let output_path = download_dir.join(filename);
                let use_disk_cache = download_options.use_disk_cache;

                if use_disk_cache {
                    download_replay_to_path(
                        &replay_id_clone,
                        download_options,
                        &output_path,
                        Some(metadata_result.clone()),
                        Some(download_progress_callback),
                    )
                    .map_err(|e| format!("Failed to download replay data: {}", e))?;
                    update_build_progress(100, 100);
                } else {
                    let replay_data = download_replay(
                        &replay_id_clone,
                        download_options,
                        Some(download_progress_callback),
                    )
                    .map_err(|e| format!("Failed to download replay data: {}", e))?;

                    update_build_progress(90, 100);
                    fs::write(output_path, replay_data)
                        .map_err(|e| format!("Failed to save replay file: {}", e))?;
                    update_build_progress(100, 100);
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
}
