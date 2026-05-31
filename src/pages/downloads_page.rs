use eframe::egui::{self, Context};

use crate::app::ReplayApp;

pub fn render_downloads_page(app: &mut ReplayApp, ui: &mut egui::Ui, _ctx: &Context) {
    ui.heading("Download Queue");
    ui.separator();

    let mut active_downloads: Vec<String> = app.active_downloads.iter().cloned().collect();
    active_downloads.sort();
    let queued_downloads: Vec<String> = app.download_queue.iter().cloned().collect();

    let progress_snapshot = app
        .download_progress
        .lock()
        .map(|progress| progress.clone())
        .unwrap_or_default();

    if active_downloads.is_empty() && queued_downloads.is_empty() {
        ui.label("No downloads queued.");
        return;
    }

    if !active_downloads.is_empty() {
        ui.add_space(6.0);
        ui.label(format!(
            "Active downloads: {} / {}",
            active_downloads.len(),
            app.settings.download_concurrency.max(1)
        ));
        ui.add_space(4.0);

        for replay_id in active_downloads {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.label(format!("Replay ID: {}", replay_id));

                    if let Some(progress) = progress_snapshot.get(&replay_id) {
                        ui.add(
                            egui::ProgressBar::new(progress.download.progress())
                                .show_percentage()
                                .text("Downloading"),
                        );
                        if progress.build.max > 0 {
                            ui.add(
                                egui::ProgressBar::new(progress.build.progress())
                                    .show_percentage()
                                    .text("Building"),
                            );
                        }
                    } else {
                        ui.label("Starting download...");
                    }

                    if let Some(error) = app.download_errors.get(&replay_id) {
                        ui.colored_label(ui.style().visuals.error_fg_color, error);
                    }
                });
            });
            ui.add_space(6.0);
        }
    }

    if !queued_downloads.is_empty() {
        ui.add_space(6.0);
        ui.label(format!("Queued downloads: {}", queued_downloads.len()));
        ui.add_space(4.0);

        for (index, replay_id) in queued_downloads.iter().enumerate() {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label(format!("#{} - {}", index + 1, replay_id));
            });
            ui.add_space(4.0);
        }
    }

    let failed_downloads: Vec<(&String, &String)> = app.download_errors.iter().collect();
    if !failed_downloads.is_empty() {
        ui.add_space(8.0);
        ui.label("Failed downloads:");
        ui.add_space(4.0);
        for (replay_id, error) in failed_downloads {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label(format!("{}", replay_id));
                ui.colored_label(ui.style().visuals.error_fg_color, error);
            });
            ui.add_space(4.0);
        }
    }
}
