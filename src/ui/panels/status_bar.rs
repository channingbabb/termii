use eframe::egui;

use crate::app::App;

pub fn render(ctx: &egui::Context, app: &mut App) {
    let m = app.state.messages.clone();
    egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            if let Some(session) = app.state.sessions.get(app.state.active_tab) {
                let color = if session.connected {
                    egui::Color32::GREEN
                } else {
                    egui::Color32::RED
                };
                ui.colored_label(color, &session.status);
            } else {
                ui.label(m.get("status_bar.no_active_session"));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let count = app.state.sessions.len().to_string();
                ui.label(m.format("status_bar.sessions_count", &[("count", &count)]));
            });
        });
    });
}
