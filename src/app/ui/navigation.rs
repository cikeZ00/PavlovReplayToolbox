use eframe::egui::{self, CentralPanel, Context};

use crate::pages;

use super::super::{Page, ReplayApp};

pub fn render_top_panel(app: &mut ReplayApp, ctx: &Context) {
    egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let button_height = 32.0;
            let active_count = app.active_downloads.len();
            let queued_count = app.download_queue.len();
            let total_items = active_count + queued_count;
            let progress_snapshot = app
                .download_progress
                .lock()
                .map(|progress| progress.clone())
                .unwrap_or_default();
            let mut progress_ratio = 0.0f32;
            if total_items > 0 {
                let mut progress_sum = 0.0f32;
                for replay_id in app.active_downloads.iter() {
                    if let Some(progress) = progress_snapshot.get(replay_id) {
                        let download_progress = progress.download.progress();
                        let build_progress = if progress.build.max > 0 {
                            progress.build.progress()
                        } else {
                            0.0
                        };
                        let part_progress = if progress.build.max > 0 {
                            (download_progress + build_progress) * 0.5
                        } else {
                            download_progress
                        };
                        progress_sum += part_progress;
                    }
                }
                progress_ratio = (progress_sum / total_items as f32).clamp(0.0, 1.0);
            }

            ui.add_sized(
                [80.0, button_height],
                egui::SelectableLabel::new(
                    app.current_page == Page::Main,
                    "Replays",
                ),
            ).clicked().then(|| {
                app.current_page = Page::Main;
            });

            ui.add_sized(
                [120.0, button_height],
                egui::SelectableLabel::new(
                    app.current_page == Page::ProcessLocal,
                    "Local Processing",
                ),
            ).clicked().then(|| {
                app.current_page = Page::ProcessLocal;
            });

            ui.add_sized(
                [80.0, button_height],
                egui::SelectableLabel::new(
                    app.current_page == Page::Manage,
                    "Manage",
                ),
            ).clicked().then(|| {
                app.current_page = Page::Manage;
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_sized(
                    [80.0, button_height],
                    egui::SelectableLabel::new(
                        app.current_page == Page::Settings,
                        "Settings",
                    ),
                ).clicked().then(|| {
                    app.current_page = Page::Settings;
                });
                
                let download_button = ui.add_sized(
                    [40.0, button_height],
                    egui::SelectableLabel::new(
                        app.current_page == Page::Downloads,
                        "DL",
                    ),
                );
                if download_button.clicked() {
                    app.current_page = Page::Downloads;
                }
                if total_items > 0 {
                    let rect = download_button.rect;
                    let pulse = ui.ctx().input(|i| i.time) as f32;
                    let pulse = (pulse * 3.0).sin() * 0.5 + 0.5;
                    let base_color = ui.style().visuals.selection.bg_fill;
                    let alpha = (80.0 + 80.0 * pulse) as u8;
                    let fill = egui::Color32::from_rgba_premultiplied(
                        base_color.r(),
                        base_color.g(),
                        base_color.b(),
                        alpha,
                    );
                    let bar_height = 4.0;
                    let bar_width = rect.width() * progress_ratio;
                    let bar_rect = egui::Rect::from_min_max(
                        egui::pos2(rect.left(), rect.bottom() - bar_height),
                        egui::pos2(rect.left() + bar_width, rect.bottom()),
                    );
                    ui.painter().rect_filled(bar_rect, 0.0, fill);
                }

                let hover_text = if total_items > 0 {
                    format!(
                        "Download Queue\nActive: {}\nQueued: {}\nProgress: {:0.0}%",
                        active_count,
                        queued_count,
                        progress_ratio * 100.0
                    )
                } else {
                    "Download Queue".to_string()
                };
                download_button.on_hover_text(hover_text);
            });
        });
        ui.add_space(4.0);
        ui.separator();
    });
}

pub fn render_current_page(app: &mut ReplayApp, ctx: &Context) {
    CentralPanel::default().show(ctx, |ui| {
        match app.current_page {
            Page::Main => pages::render_main_page(app, ui, ctx),
            Page::ProcessLocal => pages::render_process_page(app, ui),
            Page::Settings => pages::render_settings_page(app, ui),
            Page::Manage => pages::render_manage_page(app, ui, ctx),
            Page::Downloads => pages::render_downloads_page(app, ui, ctx),
        }
    });
}
