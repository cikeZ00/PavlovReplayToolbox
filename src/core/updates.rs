use reqwest::blocking::Client;
use serde::Deserialize;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub release_url: String,
    pub release_name: String,
    pub release_date: String,
    pub release_notes: String,
}

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    name: String,
    body: Option<String>,
    published_at: String,
}

pub fn spawn_update_check(update_tx: Sender<UpdateInfo>) {
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

        let latest_version = github_release.tag_name.trim_start_matches('v').to_string();

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
            let _ = update_tx.send(update_info);
        }
    });
}
