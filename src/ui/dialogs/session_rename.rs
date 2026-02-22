use eframe::egui;

use crate::app::App;
use crate::state::PendingAction;

pub fn render(ctx: &egui::Context, app: &mut App) {
    if !app.dialogs.rename_session.open {
        return;
    }

    let m = app.state.messages.clone();
    let mut open = true;
    egui::Window::new(m.get("session_rename.title"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(320.0)
        .show(ctx, |ui| {
            ui.label(m.get("session_rename.session_name"));
            ui.text_edit_singleline(&mut app.dialogs.rename_session.new_name);
            ui.separator();
            ui.horizontal(|ui| {
                let can_save = !app.dialogs.rename_session.new_name.trim().is_empty();
                if ui
                    .add_enabled(can_save, egui::Button::new(m.get("common.save")))
                    .clicked()
                {
                    if let Some(idx) = app.dialogs.rename_session.tab_idx {
                        app.state.pending.push(PendingAction::RenameSession {
                            idx,
                            new_name: app.dialogs.rename_session.new_name.trim().to_string(),
                        });
                    }
                    app.dialogs.rename_session.open = false;
                }
                if ui.button(m.get("common.cancel")).clicked() {
                    app.dialogs.rename_session.open = false;
                }
            });
        });

    if !open {
        app.dialogs.rename_session.open = false;
    }
}
