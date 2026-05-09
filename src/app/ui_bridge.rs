use eframe::egui::{self, Context};

use super::{ui, ReplayApp};

impl ReplayApp {
    pub fn styled_button(&self, ui: &mut egui::Ui, text: &str) -> egui::Response {
        ui.add_sized(
            [ui.available_width().min(120.0), 32.0],
            egui::Button::new(text)
        )
    }

    pub fn render_user_avatar(&mut self, ui: &mut egui::Ui, ctx: &Context, user: &str) {
        ui::render_user_avatar(self, ui, ctx, user);
    }
}
