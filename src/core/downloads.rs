use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone, Debug)]
pub struct DownloadedReplayInfo {
    pub id: String,
    pub filename: String,
    pub full_path: PathBuf,
    pub file_size: u64,
    pub modified_time: Option<SystemTime>,
    pub game_mode: Option<String>,
    pub map_name: Option<String>,
    pub date: Option<String>,
}

pub fn scan_downloaded_replays(download_dir: &Path) -> Vec<DownloadedReplayInfo> {
    let mut replays = Vec::new();

    if let Ok(entries) = fs::read_dir(download_dir) {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_file() {
                    if let Some(ext) = entry.path().extension() {
                        if ext == "replay" {
                            if let Some(filename) = entry.path().file_name() {
                                if let Some(filename_str) = filename.to_str() {
                                    let full_path = entry.path();

                                    let metadata = fs::metadata(&full_path).ok();
                                    let file_size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
                                    let modified_time = metadata.and_then(|m| m.modified().ok());

                                    let replay_id = if let Some(id_start) = filename_str.rfind('(') {
                                        if let Some(id_end) = filename_str[id_start..].find(')') {
                                            filename_str[id_start + 1..id_start + id_end].to_string()
                                        } else {
                                            "Unknown".to_string()
                                        }
                                    } else {
                                        if let Ok(mut file) = fs::File::open(&full_path) {
                                            let mut buffer = [0; 1024];
                                            if file.read(&mut buffer).is_ok() {
                                                let content = String::from_utf8_lossy(&buffer);
                                                if let Some(id_start) = content.find("\"id\":\"") {
                                                    let id_start = id_start + 6;
                                                    if let Some(id_end) = content[id_start..].find('"') {
                                                        content[id_start..id_start + id_end].to_string()
                                                    } else {
                                                        "Unknown".to_string()
                                                    }
                                                } else {
                                                    "Unknown".to_string()
                                                }
                                            } else {
                                                "Unknown".to_string()
                                            }
                                        } else {
                                            "Unknown".to_string()
                                        }
                                    };

                                    let all_parts: Vec<&str> = filename_str.split('-').collect();
                                    let (game_mode, map_name, date) = if all_parts.len() >= 3 {
                                        let common_modes = ["SND", "TDM", "DM", "KOTH", "TTT", "ZWV", "PUSH", "TANKTDM"];

                                        let mut mode_index = None;
                                        for (i, part) in all_parts.iter().enumerate() {
                                            if common_modes.iter().any(|&mode| part.to_uppercase() == mode) {
                                                mode_index = Some(i);
                                                break;
                                            }
                                        }

                                        if let Some(mode_idx) = mode_index {
                                            let map_name = if mode_idx > 0 {
                                                Some(all_parts[0..mode_idx].join("-"))
                                            } else {
                                                None
                                            };

                                            let game_mode = Some(all_parts[mode_idx].to_string());

                                            let mut date = None;
                                            for part in &all_parts[mode_idx + 1..] {
                                                if part.len() >= 10 && part.chars().nth(4) == Some('.') && part.chars().nth(7) == Some('.') {
                                                    let date_clean = part.split('(').next().unwrap_or("");
                                                    if !date_clean.is_empty() {
                                                        date = Some(date_clean.replace('.', "/"));
                                                    }
                                                    break;
                                                }
                                            }

                                            (game_mode, map_name, date)
                                        } else {
                                            let mut date_index = None;
                                            for (i, part) in all_parts.iter().enumerate() {
                                                if part.len() >= 10 && part.chars().nth(4) == Some('.') && part.chars().nth(7) == Some('.') {
                                                    date_index = Some(i);
                                                    break;
                                                }
                                            }

                                            if let Some(date_idx) = date_index {
                                                if date_idx > 0 {
                                                    let potential_mode = all_parts[date_idx - 1];
                                                    if common_modes.iter().any(|&mode| potential_mode.to_uppercase() == mode) {
                                                        let map_name = if date_idx > 1 {
                                                            Some(all_parts[0..date_idx - 1].join("-"))
                                                        } else {
                                                            None
                                                        };
                                                        let game_mode = Some(potential_mode.to_string());
                                                        let date_clean = all_parts[date_idx].split('(').next().unwrap_or("");
                                                        let date = if !date_clean.is_empty() {
                                                            Some(date_clean.replace('.', "/"))
                                                        } else {
                                                            None
                                                        };
                                                        (game_mode, map_name, date)
                                                    } else {
                                                        (None, None, None)
                                                    }
                                                } else {
                                                    (None, None, None)
                                                }
                                            } else {
                                                (None, None, None)
                                            }
                                        }
                                    } else {
                                        (None, None, None)
                                    };

                                    replays.push(DownloadedReplayInfo {
                                        id: replay_id,
                                        filename: filename_str.to_string(),
                                        full_path,
                                        file_size,
                                        modified_time,
                                        game_mode,
                                        map_name,
                                        date,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    replays.sort_by(|a, b| b.modified_time.cmp(&a.modified_time));

    replays
}

pub fn delete_replay_file(replay_info: &DownloadedReplayInfo) -> Result<(), std::io::Error> {
    fs::remove_file(&replay_info.full_path)
}
