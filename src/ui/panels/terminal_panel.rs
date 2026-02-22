use eframe::egui;

use crate::app::App;
use crate::state::PendingAction;

pub fn render(ctx: &egui::Context, app: &mut App) {
    let m = app.state.messages.clone();
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.horizontal(|ui| {
            let mut close_idx: Option<usize> = None;
            let mut restart_idx: Option<usize> = None;
            let mut clone_idx: Option<usize> = None;
            let mut settings_idx: Option<usize> = None;
            let mut rename_idx: Option<usize> = None;

            for (i, session) in app.state.sessions.iter().enumerate() {
                let resp = ui.small_button(session.name.as_str());
                if resp.clicked() {
                    app.state.active_tab = i;
                }
                resp.context_menu(|ui| {
                    if ui.button(m.get("terminal_panel.rename_session")).clicked() {
                        rename_idx = Some(i);
                        ui.close_menu();
                    }
                    if ui.button(m.get("terminal_panel.restart_session")).clicked() {
                        restart_idx = Some(i);
                        ui.close_menu();
                    }
                    if ui.button(m.get("terminal_panel.clone_session")).clicked() {
                        clone_idx = Some(i);
                        ui.close_menu();
                    }
                    if ui
                        .button(m.get("terminal_panel.session_settings"))
                        .clicked()
                    {
                        settings_idx = Some(i);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(m.get("common.close")).clicked() {
                        close_idx = Some(i);
                        ui.close_menu();
                    }
                });
            }

            if let Some(idx) = rename_idx {
                if let Some(session) = app.state.sessions.get(idx) {
                    app.dialogs.rename_session.open = true;
                    app.dialogs.rename_session.tab_idx = Some(idx);
                    app.dialogs.rename_session.new_name = session.name.clone();
                }
            }
            if let Some(idx) = restart_idx {
                app.state.pending.push(PendingAction::RestartSession(idx));
            }
            if let Some(idx) = clone_idx {
                app.state.pending.push(PendingAction::CloneSession(idx));
            }
            if let Some(idx) = settings_idx {
                app.state
                    .pending
                    .push(PendingAction::OpenSessionSettings(idx));
            }
            if let Some(idx) = close_idx {
                app.state.pending.push(PendingAction::CloseTab(idx));
            }
        });

        ui.separator();

        if let Some(session) = app.state.sessions.get_mut(app.state.active_tab) {
            crate::ui::terminal_view::render(ui, session);
        } else {
            ui.centered_and_justified(|ui| {
                ui.heading(m.get("terminal_panel.no_active_session_hint"));
            });
        }
    });
}
