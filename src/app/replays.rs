use reqwest::blocking::Client;
use std::time::Instant;

use crate::core::{self, PlatformFilter};
use crate::tools::replay_processor::{ApiResponse, ReplayItem, API_BASE_URL};

use super::ReplayApp;

fn parse_date(raw: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(raw) {
        return dt.with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
    }

    if let Ok(timestamp) = raw.parse::<i64>() {
        if let Some(dt) = chrono::DateTime::from_timestamp(timestamp, 0) {
            return dt.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string();
        }
    }

    raw.to_string()
}

impl ReplayApp {
    fn fetch_replays(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let client = match Client::builder()
            .timeout(std::time::Duration::from_secs(10))
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
                created_date: parse_date(&r.created),
                time_since: r.time_since,
                shack: r.shack,
                modcount: r.modcount,
                competitive: r.competitive,
                workshop_mods: r.workshop_mods,
                live: r.live,
                users: r.users.unwrap_or_default(),
                expires_date: parse_date(&r.expires),
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
                self.rebuild_filtered_replays();
                if let Ok(mut status) = self.status.lock() {
                    *status = "Replays loaded successfully".to_string();
                }
                self.show_success("Replays loaded successfully");
                self.last_refresh_time = Instant::now();

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

    pub(crate) fn check_auto_download_triggers(&mut self) {
        if !self.settings.auto_download_enabled
            || self.settings.auto_download_trigger_user_id.is_empty()
            || self.is_downloading {
            return;
        }

        let trigger_user_id = self.settings.auto_download_trigger_user_id.to_lowercase();

        let replay_to_download = self.replay_list.replays.iter()
            .find(|replay| {
                !self.downloaded_replays.contains(&replay.id)
                    && replay.users.iter().any(|user| user.to_lowercase().contains(&trigger_user_id))
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

    pub fn rebuild_filtered_replays(&mut self) {
        core::replays::rebuild_filtered_replays(&mut self.replay_list);
    }

    pub fn filtered_replays(&self) -> &[ReplayItem] {
        &self.replay_list.filtered_replays
    }
}
