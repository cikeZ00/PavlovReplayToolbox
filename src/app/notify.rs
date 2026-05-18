use std::time::Instant;

use super::state::{Notification, NotificationType, ReplayApp};

impl ReplayApp {
    fn show_notification(&mut self, message: String, notification_type: NotificationType) {
        let id = self.next_notification_id;
        self.next_notification_id += 1;

        self.notifications.push(Notification {
            id,
            message,
            created_at: Instant::now(),
            duration_ms: 4000,
            notification_type,
            position: 0.0,
        });
    }

    pub fn show_success(&mut self, message: impl Into<String>) {
        self.show_notification(message.into(), NotificationType::Success)
    }

    pub fn show_error(&mut self, message: impl Into<String>) {
        self.show_notification(message.into(), NotificationType::Error)
    }

    pub fn show_info(&mut self, message: impl Into<String>) {
        self.show_notification(message.into(), NotificationType::Info)
    }

    #[allow(dead_code)]
    fn show_warning(&mut self, message: impl Into<String>) {
        self.show_notification(message.into(), NotificationType::Warning)
    }
}
