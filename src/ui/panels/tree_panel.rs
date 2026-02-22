use eframe::egui;

use crate::app::App;

pub fn render(ctx: &egui::Context, app: &mut App) {
    egui::SidePanel::left("tree_panel")
        .default_width(260.0)
        .min_width(180.0)
        .show(ctx, |ui_panel| {
            egui::ScrollArea::vertical().show(ui_panel, |ui| {
                crate::ui::tree::render_tree(ui, app);
            });
        });
}
