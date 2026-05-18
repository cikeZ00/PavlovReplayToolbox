use std::time::Instant;

use eframe::egui::{self, Context};

use super::super::{NotificationType, ReplayApp};

pub fn update_notifications(app: &mut ReplayApp) {
    let now = Instant::now();

    app.notifications.retain(|notification| {
        now.duration_since(notification.created_at).as_millis() < notification.duration_ms as u128
    });

    app.notifications.sort_by(|a, b| a.created_at.cmp(&b.created_at));

    for notification in &mut app.notifications {
        let elapsed_ms = now.duration_since(notification.created_at).as_millis() as f32;
        let animation_duration = 400.0;
        let t = (elapsed_ms / animation_duration).min(1.0);
        notification.position = cubic_ease_out(t);
    }
}

pub fn render_notifications(app: &ReplayApp, ctx: &Context) {
    let notification_height = 40.0;
    let notification_spacing = 8.0;
    let max_visible = 5;
    let bottom_margin = 20.0;

    let visible_notifications = app.notifications.iter().take(max_visible).collect::<Vec<_>>();

    for (idx, notification) in visible_notifications.iter().enumerate() {
        let pos = notification.position;

        let elapsed_ms = Instant::now().duration_since(notification.created_at).as_millis() as f32;
        let fade_out_start = notification.duration_ms as f32 - 1000.0;

        let alpha = if pos < 0.4 {
            cubic_ease_out(pos / 0.4)
        } else if elapsed_ms > fade_out_start {
            (1.0 - ((elapsed_ms - fade_out_start) / 900.0).min(1.0)).powf(2.0)
        } else {
            1.0
        };

        let base_position = idx as f32 * (notification_height + notification_spacing);
        let slide_offset = if pos < 1.0 { (1.0 - pos) * notification_height * 1.2 } else { 0.0 };

        let bottom_offset = bottom_margin + base_position + slide_offset;

        let bg_color = match notification.notification_type {
            NotificationType::Info => egui::Color32::from_rgba_unmultiplied(30, 130, 220, (alpha * 220.0) as u8),
            NotificationType::Success => egui::Color32::from_rgba_unmultiplied(30, 150, 30, (alpha * 220.0) as u8),
            NotificationType::Warning => egui::Color32::from_rgba_unmultiplied(220, 160, 20, (alpha * 220.0) as u8),
            NotificationType::Error => egui::Color32::from_rgba_unmultiplied(220, 40, 40, (alpha * 220.0) as u8),
        };

        egui::Area::new(egui::Id::new(format!("notification_{}", notification.id)))
            .anchor(egui::Align2::CENTER_BOTTOM, egui::Vec2::new(0.0, -bottom_offset))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(bg_color)
                    .corner_radius(8.0)
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 2],
                        blur: 4,
                        spread: 0,
                        color: ctx.style().visuals.window_shadow.color,
                    })
                    .show(ui, |ui| {
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.add_space(12.0);
                            ui.colored_label(
                                egui::Color32::from_rgba_unmultiplied(255, 255, 255, (alpha * 255.0) as u8),
                                &notification.message,
                            );
                            ui.add_space(12.0);
                        });
                        ui.add_space(6.0);
                    });
            });
    }
}

fn cubic_ease_out(t: f32) -> f32 {
    let f = t - 1.0;
    f * f * f + 1.0
}
