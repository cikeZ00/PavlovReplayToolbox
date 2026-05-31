mod dialogs;
mod navigation;
mod notifications;
mod widgets;

pub use dialogs::{render_completion_dialog, render_update_dialog};
pub use navigation::{render_current_page, render_top_panel};
pub use notifications::{render_notifications, update_notifications};
pub use widgets::render_user_avatar;
