use eframe::egui;
use reqwest::blocking::Client;
use serde::Deserialize;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct ModInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub thumbnail_url: Option<String>,
    pub is_loading: bool,
    pub failed: bool,
}

#[derive(Deserialize, Debug)]
struct ModioResponse {
    #[serde(default)]
    name: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    description_plaintext: String,
    #[serde(default)]
    logo: ModioLogo,
}

#[derive(Deserialize, Debug, Default)]
struct ModioLogo {
    #[serde(default)]
    thumb_320x180: String,
}

#[derive(Deserialize, Debug)]
struct ModioErrorResponse {
    error: ModioErrorDetails,
}

#[derive(Deserialize, Debug)]
struct ModioErrorDetails {
    message: String,
    code: u32,
}

pub fn parse_mod_ids(workshop_mods: &str) -> Vec<String> {
    let mut mod_ids = Vec::new();

    let cleaned_str = workshop_mods.strip_prefix("AdditionalMods=").unwrap_or(workshop_mods);

    for part in cleaned_str.split('-') {
        if part.starts_with("UGC") {
            if let Some(slash_pos) = part.find('/') {
                let id = &part[3..slash_pos];
                mod_ids.push(id.to_string());
            }
        }
    }

    mod_ids
}

pub fn spawn_mod_info_load(mod_id: String, api_url: String, api_token: String, mod_info_tx: Sender<ModInfo>) {
    thread::spawn(move || {
        let client = match Client::builder()
            .timeout(Duration::from_secs(10))
            .build() {
                Ok(client) => client,
                Err(_) => {
                    let _ = mod_info_tx.send(ModInfo {
                        id: mod_id,
                        name: "Error".to_string(),
                        description: "Failed to create HTTP client".to_string(),
                        thumbnail_url: None,
                        is_loading: false,
                        failed: true,
                    });
                    return;
                }
            };

        if api_token.is_empty() {
            let _ = mod_info_tx.send(ModInfo {
                id: mod_id.clone(),
                name: format!("Mod ID: {}", mod_id),
                description: "Configure mod.io API in settings to see mod details.".to_string(),
                thumbnail_url: None,
                is_loading: false,
                failed: false,
            });
            return;
        }

        let url = format!("{}/games/3959/mods/{}?api_key={}", api_url, mod_id, api_token);

        match client.get(&url).send() {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        match response.json::<ModioResponse>() {
                            Ok(mod_data) => {
                                let _ = mod_info_tx.send(ModInfo {
                                    id: mod_id,
                                    name: mod_data.name,
                                    description: if mod_data.description_plaintext.is_empty() {
                                        mod_data.summary
                                    } else {
                                        mod_data.description_plaintext
                                    },
                                    thumbnail_url: if !mod_data.logo.thumb_320x180.is_empty() {
                                        Some(mod_data.logo.thumb_320x180)
                                    } else {
                                        None
                                    },
                                    is_loading: false,
                                    failed: false,
                                });
                            },
                            Err(e) => {
                                let _ = mod_info_tx.send(ModInfo {
                                    id: mod_id,
                                    name: "Parse Error".to_string(),
                                    description: format!("Failed to parse mod.io API response: {}", e),
                                    thumbnail_url: None,
                                    is_loading: false,
                                    failed: true,
                                });
                            }
                        }
                    } else {
                        let error_text = response.text().unwrap_or_default();
                        let error_msg = match serde_json::from_str::<ModioErrorResponse>(&error_text) {
                            Ok(error) => format!("{} (Code: {})", error.error.message, error.error.code),
                            Err(_) => format!("HTTP error: {}", status)
                        };

                        let _ = mod_info_tx.send(ModInfo {
                            id: mod_id,
                            name: "API Error".to_string(),
                            description: error_msg,
                            thumbnail_url: None,
                            is_loading: false,
                            failed: true,
                        });
                    }
                },
                Err(e) => {
                    let _ = mod_info_tx.send(ModInfo {
                        id: mod_id,
                        name: "Network Error".to_string(),
                        description: format!("Connection failed: {}", e),
                        thumbnail_url: None,
                        is_loading: false,
                        failed: true,
                    });
                }
            }
    });
}

pub fn spawn_mod_thumbnail_load(
    mod_id: String,
    thumbnail_url: String,
    thumbnail_tx: Sender<(String, egui::ColorImage)>,
) {
    thread::spawn(move || {
        let client = match Client::builder()
            .timeout(Duration::from_secs(10))
            .build() {
                Ok(client) => client,
                Err(_) => return,
            };

        match client.get(&thumbnail_url).send() {
            Ok(response) => {
                if response.status().is_success() {
                    match response.bytes() {
                        Ok(bytes) => {
                            match image::load_from_memory(&bytes) {
                                Ok(img) => {
                                    let img = img.to_rgba8();
                                    let size = [img.width() as usize, img.height() as usize];
                                    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &img.into_raw());
                                    let _ = thumbnail_tx.send((mod_id, color_image));
                                },
                                Err(_) => {}
                            }
                        },
                        Err(_) => {}
                    }
                }
            },
            Err(_) => {}
        }
    });
}
