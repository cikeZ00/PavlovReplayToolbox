use eframe::egui::{self, Context};

use super::super::ReplayApp;

pub fn render_user_avatar(app: &mut ReplayApp, ui: &mut egui::Ui, ctx: &Context, user: &str) {
    let avatar_size = egui::vec2(64.0, 64.0);

    egui::Frame::new()
        .fill(ui.style().visuals.window_fill)
        .inner_margin(0.0)
        .outer_margin(0.0)
        .show(ui, |ui| {
            ui.set_min_size(avatar_size);
            ui.set_max_size(avatar_size);

            let mut response = None;

            let texture_handle = app.profile_textures.get(user).cloned();

            if let Some(texture) = texture_handle {
                ui.centered_and_justified(|ui| {
                    let btn_response = ui.add_sized(
                        avatar_size,
                        egui::Button::image_and_text(&texture, "")
                            .frame(false),
                    );

                    if btn_response.clicked() {
                        ctx.copy_text(user.to_string());
                    }

                    response = Some(btn_response);
                });
            } else {
                ui.centered_and_justified(|ui| {
                    let btn_response = ui.add_sized(avatar_size, egui::Button::new("Loading"));

                    if btn_response.clicked() {
                        ctx.copy_text(user.to_string());
                    }

                    response = Some(btn_response);
                });

                if !app.loading_profiles.contains(user) {
                    app.load_profile(user.to_string());
                }
            }

            if let Some(resp) = &response {
                if resp.clicked() {
                    app.show_success(format!("Copied user ID: {}", user));
                }

                if resp.hovered() {
                    let rect = resp.rect;
                    ui.painter().rect_stroke(
                        rect.expand(2.0),
                        egui::epaint::CornerRadius::ZERO,
                        egui::Stroke::new(2.0, ui.style().visuals.selection.bg_fill),
                        egui::epaint::StrokeKind::Outside,
                    );

                    resp.clone().on_hover_text(user);
                }
            }
        });
}
