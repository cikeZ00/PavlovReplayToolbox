use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs;
use std::path::PathBuf;

fn default_download_use_disk_cache() -> bool {
    true
}

fn default_download_thread_count() -> usize {
    std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(4)
}

fn default_download_concurrency() -> usize {
    1
}

fn default_download_retry_enabled() -> bool {
    true
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Settings {
    pub download_dir: PathBuf,
    #[serde(default = "default_download_use_disk_cache")]
    pub download_use_disk_cache: bool,
    #[serde(default = "default_download_thread_count")]
    pub download_thread_count: usize,
    #[serde(default = "default_download_concurrency")]
    pub download_concurrency: usize,
    #[serde(default = "default_download_retry_enabled")]
    pub download_retry_enabled: bool,
    pub auto_refresh_enabled: bool,
    pub auto_refresh_interval_mins: u64,
    pub auto_download_enabled: bool,
    pub auto_download_trigger_user_id: String,
    pub modio_api_url: String,
    pub modio_api_token: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            download_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            download_use_disk_cache: default_download_use_disk_cache(),
            download_thread_count: default_download_thread_count(),
            download_concurrency: default_download_concurrency(),
            download_retry_enabled: default_download_retry_enabled(),
            auto_refresh_enabled: true,
            auto_refresh_interval_mins: 5,
            auto_download_enabled: false,
            auto_download_trigger_user_id: String::new(),
            modio_api_url: "https://api.mod.io/v1".to_string(),
            modio_api_token: String::new(),
        }
    }
}

pub fn load_settings() -> Result<Settings, Box<dyn Error>> {
    let settings_dir = get_settings_dir()?;
    let settings_file = settings_dir.join("settings.json");

    if !settings_file.exists() {
        return Ok(Settings::default());
    }

    let settings_str = fs::read_to_string(settings_file)?;
    let settings = serde_json::from_str(&settings_str)?;
    Ok(settings)
}

pub fn save_settings(settings: &Settings) -> Result<(), Box<dyn Error>> {
    let settings_dir = get_settings_dir()?;
    fs::create_dir_all(&settings_dir)?;

    let settings_file = settings_dir.join("settings.json");
    let settings_str = serde_json::to_string_pretty(settings)?;

    fs::write(settings_file, settings_str)?;
    Ok(())
}

fn get_settings_dir() -> Result<PathBuf, Box<dyn Error>> {
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
