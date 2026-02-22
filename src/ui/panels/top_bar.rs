use eframe::egui;

use crate::app::App;

pub fn render(ctx: &egui::Context, app: &mut App) {
    let m = app.state.messages.clone();
    egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
        egui::menu::bar(ui, |ui| {
            ui.menu_button(m.get("top_bar.file_menu"), |ui| {
                if ui.button(m.get("top_bar.settings")).clicked() {
                    app.dialogs.settings.open = true;
                    ui.close_menu();
                }
                if app.state.master_key.is_some()
                    && ui.button(m.get("top_bar.lock_master_password")).clicked()
                {
                    app.clear_master_key();
                    let locked_message = m.get("master_password.locked_message");
                    app.open_master_password_unlock_dialog(locked_message.as_str());
                    ui.close_menu();
                }
            });

            ui.separator();
            ui.label(m.get("top_bar.search"));
            ui.text_edit_singleline(&mut app.state.search_query);
        });
    });
}
