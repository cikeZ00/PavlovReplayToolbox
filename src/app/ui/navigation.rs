use eframe::egui::{self, CentralPanel, Context};

use crate::pages;

use super::super::{Page, ReplayApp};

pub fn render_top_panel(app: &mut ReplayApp, ctx: &Context) {
    egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let button_height = 32.0;

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
        }
    });
}
