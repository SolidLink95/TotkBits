use egui::{Context, Ui, Widget};
use std::time::{Duration, Instant};

/// A notification widget that displays a message for a fixed amount of time.
pub struct TimedNotification {
    message: String,
    creation_time: Instant,
    duration: Duration,
}

impl TimedNotification {
    pub fn new(message: String) -> Self {
        Self {
            message,
            creation_time: Instant::now(),
            duration: Duration::new(5, 0), // 5 seconds
        }
    }

    /// Determines if the notification is still active.
    fn is_active(&self) -> bool {
        self.creation_time.elapsed() < self.duration
    }

    /// Calculates the current opacity for the fade effect.
    fn current_opacity(&self) -> f32 {
        let elapsed = self.creation_time.elapsed().as_secs_f32();
        let total_duration = self.duration.as_secs_f32();

        if elapsed < total_duration / 2.0 {
            // Fade in
            2.0 * elapsed / total_duration
        } else {
            // Fade out
            2.0 * (1.0 - elapsed / total_duration)
        }
    }
}

impl Widget for TimedNotification {
    fn ui(self, ui: &mut Ui) -> egui::Response {
        if self.is_active() {
            let opacity = self.current_opacity();
            let color = egui::Color32::from_black_alpha((opacity * 255.0) as u8);
            let text_style = egui::TextStyle::Body;

            ui.colored_label(color, &self.message)
        } else {
            ui.label("") // Display nothing when the notification is inactive
        }
    }
}

// Usage in your egui app
fn egui_ui(ctx: &Context) {
    egui::CentralPanel::default().show(ctx, |ui| {
        let notification = TimedNotification::new("Your message here".to_string());
        ui.add(notification);
    });
}
