use eframe::egui;
use reqwest::blocking::Client;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

pub fn spawn_profile_load(
    user: String,
    profile_tx: Sender<(String, egui::ColorImage)>,
    status: Option<Arc<Mutex<String>>>,
) {
    thread::spawn(move || {
        let client = match Client::builder()
            .timeout(Some(Duration::from_secs(10)))
            .build() {
                Ok(client) => client,
                Err(e) => {
                    if let Some(status) = status {
                        if let Ok(mut status) = status.lock() {
                            *status = format!("Failed to initialize HTTP client for profile: {}", e);
                        }
                    }
                    return;
                }
            };

        let url = format!("http://prod.cdn.pavlov-vr.com/avatar/{}.png", user);

        match client.get(&url).send() {
            Ok(response) => {
                if !response.status().is_success() {
                    return;
                }

                match response.bytes() {
                    Ok(bytes) => {
                        match image::load_from_memory(&bytes) {
                            Ok(img) => {
                                let img = img.to_rgba8();
                                let size = [img.width() as usize, img.height() as usize];
                                let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &img.into_raw());
                                let _ = profile_tx.send((user, color_image));
                            },
                            Err(_) => {}
                        }
                    },
                    Err(_) => {}
                }
            },
            Err(e) => {
                if e.is_timeout() || e.is_connect() {
                    return;
                }
            }
        }
    });
}
