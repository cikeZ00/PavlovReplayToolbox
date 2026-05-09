use eframe::egui::{self, Context};

use super::super::ReplayApp;

pub fn render_download_progress(app: &mut ReplayApp, ctx: &Context) {
    if let Some(_replay_id) = &app.downloading_replay_id {
        if let Ok(progress) = app.download_progress.lock() {
            if let Some(p) = &*progress {
                egui::Window::new("Downloading Replay")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                    .show(ctx, |ui| {
                        ui.set_min_width(300.0);

                        ui.label("Downloading components:");
                        ui.add(egui::ProgressBar::new(p.download.progress())
                            .show_percentage()
                            .animate(true)
                        );

                        ui.add_space(8.0);
                        ui.label("Building replay:");
                        ui.add(egui::ProgressBar::new(p.build.progress())
                            .show_percentage()
                            .animate(true)
                        );

                        ui.add_space(8.0);
                        if let Ok(status) = app.status.lock() {
                            ui.label(&*status);
                        }
                    });
            } else {
                app.downloading_replay_id = None;
            }
        }
    }
}
