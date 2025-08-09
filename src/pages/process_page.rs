use eframe::egui;
use crate::app::ReplayApp;

pub fn render_process_page(app: &mut ReplayApp, ui: &mut egui::Ui) {
    ui.heading("Process Local Replay (Legacy Replay Format)");
    ui.separator();

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    if let Some(path) = &app.selected_path {
                        ui.label("Directory:");
                        ui.add(egui::Label::new(path.display().to_string()).wrap());
                    } else {
                        ui.label("No directory selected");
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if app.styled_button(ui, "Select Directory").clicked() {
                            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                app.selected_path = Some(path);
                            }
                        }
                    });
                });
            });

            if !app.is_processing_local && !app.show_completion_dialog {
                let can_process = app.selected_path.is_some();
                ui.add_space(8.0);
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui| {
                    if ui.add_enabled(
                        can_process,
                        egui::Button::new("Start Processing")
                            .min_size(egui::vec2(ui.available_width().min(120.0), 32.0))
                    ).clicked() {
                        app.start_processing();
                    }
                });
                if !can_process {
                    ui.colored_label(ui.style().visuals.error_fg_color, "Please select a directory first");
                }
            }

            if let Ok(progress) = app.progress.lock() {
                if let Some(p) = &*progress {
                    ui.add_space(16.0);
                    egui::Frame::group(ui.style())
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.spacing_mut().item_spacing.y = 8.0;

                            let progress_bar = |ui: &mut egui::Ui, label, progress| {
                                ui.label(label);
                                ui.add(egui::ProgressBar::new(progress)
                                    .show_percentage()
                                    .animate(true));
                            };

                            progress_bar(ui, "Header:", p.header.progress());
                            progress_bar(ui, "Data Chunks:", p.data_chunks.progress());
                            progress_bar(ui, "Event Chunks:", p.event_chunks.progress());
                            progress_bar(ui, "Checkpoint Chunks:", p.checkpoint_chunks.progress());
                        });
                }
            }

            if let Ok(status) = app.status.lock() {
                ui.add_space(8.0);
                ui.separator();
                ui.colored_label(
                    if status.contains("Error") {
                        ui.style().visuals.error_fg_color
                    } else {
                        ui.style().visuals.text_color()
                    },
                    status.as_str()
                );
            }
        });
}