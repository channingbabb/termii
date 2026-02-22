use eframe::egui;

use crate::services::config::ThemePreference;
use crate::state::app_state::AppState;

pub fn apply_theme(state: &AppState, ctx: &egui::Context) {
    match state.config.theme {
        ThemePreference::Dark => ctx.set_visuals(egui::Visuals::dark()),
        ThemePreference::Light => ctx.set_visuals(egui::Visuals::light()),
        ThemePreference::System => match dark_light::detect() {
            Ok(dark_light::Mode::Light) => ctx.set_visuals(egui::Visuals::light()),
            _ => ctx.set_visuals(egui::Visuals::dark()),
        },
    }
}
