use std::{sync::Arc, thread};

use crate::tools::replay_processor::{process_replay, Config};

use super::ReplayApp;

impl ReplayApp {
    pub fn start_processing(&mut self) {
        if self.is_processing_local || self.selected_path.is_none() {
            return;
        }
        self.is_processing_local = true;

        let progress_clone = Arc::clone(&self.progress);
        let status_clone = Arc::clone(&self.status);
        let path_clone = self.selected_path.clone().unwrap();

        thread::spawn(move || {
            if let Err(e) = std::env::set_current_dir(&path_clone) {
                if let Ok(mut status) = status_clone.lock() {
                    *status = format!("Error: Failed to set working directory - {}", e);
                }
                return;
            }

            let config = Config {
                update_callback: Box::new(move |progress| {
                    if let Ok(mut lock) = progress_clone.lock() {
                        *lock = Some(progress);
                    }
                }),
                ..Default::default()
            };

            let result = process_replay(Some(config));

            if let Ok(mut status) = status_clone.lock() {
                *status = match result {
                    Ok(_) => "Replay processing complete.".into(),
                    Err(e) => format!("Error: {}", e),
                };
            }
        });
    }
}
