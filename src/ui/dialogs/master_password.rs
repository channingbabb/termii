use eframe::egui;

use crate::app::App;
use crate::services::{config, crypto};
use crate::state::{MasterPasswordDialogState, MasterPasswordMode};

pub fn render(ctx: &egui::Context, app: &mut App) {
    if !app.dialogs.master_password_dialog.open {
        return;
    }

    let m = app.state.messages.clone();
    let (title, show_confirm) = match app.dialogs.master_password_dialog.mode {
        MasterPasswordMode::Create => (m.get("master_password.create_title"), true),
        MasterPasswordMode::Unlock => (m.get("master_password.unlock_title"), false),
    };

    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .default_width(420.0)
        .show(ctx, |ui| {
            ui.label(m.get("master_password.requirements"));
            ui.add_space(6.0);

            ui.label(m.get("master_password.password"));
            let password: &mut String = &mut app.dialogs.master_password_dialog.password;
            ui.add(egui::TextEdit::singleline(password).password(true));

            if show_confirm {
                ui.label(m.get("master_password.confirm_password"));
                let confirm: &mut String = &mut app.dialogs.master_password_dialog.confirm;
                ui.add(egui::TextEdit::singleline(confirm).password(true));
            }

            if !app.dialogs.master_password_dialog.error.is_empty() {
                ui.colored_label(
                    egui::Color32::from_rgb(230, 80, 80),
                    &app.dialogs.master_password_dialog.error,
                );
            }

            ui.separator();
            let action_label = if show_confirm {
                m.get("common.create")
            } else {
                m.get("common.unlock")
            };
            if ui.button(action_label).clicked() {
                match app.dialogs.master_password_dialog.mode {
                    MasterPasswordMode::Create => {
                        if app.dialogs.master_password_dialog.password
                            != app.dialogs.master_password_dialog.confirm
                        {
                            app.dialogs.master_password_dialog.error =
                                m.get("master_password.confirm_mismatch");
                            return;
                        }
                        match crypto::create_master_password(
                            &app.dialogs.master_password_dialog.password,
                        ) {
                            Ok((cfg, key)) => {
                                app.state.config.master_password = Some(cfg);
                                app.replace_master_key(key);
                                app.dialogs.master_password_dialog.clear_secrets();
                                app.dialogs.master_password_dialog =
                                    MasterPasswordDialogState::default();
                                let _ = config::save_config(&app.state.config);
                            }
                            Err(e) => {
                                app.dialogs.master_password_dialog.error = e.to_string();
                            }
                        }
                    }
                    MasterPasswordMode::Unlock => {
                        let Some(ref cfg) = app.state.config.master_password else {
                            app.dialogs.master_password_dialog.error =
                                m.get("master_password.not_configured");
                            return;
                        };
                        match crypto::unlock_master_password(
                            cfg,
                            &app.dialogs.master_password_dialog.password,
                        ) {
                            Ok(key) => {
                                app.replace_master_key(key);
                                app.dialogs.master_password_dialog.clear_secrets();
                                app.dialogs.master_password_dialog =
                                    MasterPasswordDialogState::default();
                            }
                            Err(e) => {
                                app.dialogs.master_password_dialog.error = e.to_string();
                            }
                        }
                    }
                }
            }
        });
}
