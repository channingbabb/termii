use eframe::egui;
use std::path::PathBuf;

use crate::app::App;
use crate::domain::connection::{AuthMethod, SavedConnection};
use crate::services::crypto;
use crate::state::PendingAction;

fn ssh_key_dialog(title: String) -> rfd::FileDialog {
    let mut dlg = rfd::FileDialog::new().set_title(title);
    if let Some(ssh) = crate::services::ssh_keys::ssh_dir() {
        if ssh.is_dir() {
            dlg = dlg.set_directory(&ssh);
        }
    }
    dlg
}

pub fn render(ctx: &egui::Context, app: &mut App) {
    if !app.dialogs.editor.open {
        return;
    }

    let m = app.state.messages.clone();
    let mut open = true;
    egui::Window::new(m.get("connection_editor.title"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .min_width(350.0)
        .show(ctx, |ui| {
            egui::Grid::new("conn_editor_grid")
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label(m.get("connection_editor.display_name"));
                    ui.text_edit_singleline(&mut app.dialogs.editor.display_name);
                    ui.end_row();

                    ui.label(m.get("connection_editor.host"));
                    ui.text_edit_singleline(&mut app.dialogs.editor.host);
                    ui.end_row();

                    ui.label(m.get("connection_editor.port"));
                    ui.text_edit_singleline(&mut app.dialogs.editor.port);
                    ui.end_row();

                    ui.label(m.get("connection_editor.username"));
                    ui.text_edit_singleline(&mut app.dialogs.editor.username);
                    ui.end_row();

                    ui.label(m.get("connection_editor.auth"));
                    ui.horizontal(|ui| {
                        ui.radio_value(
                            &mut app.dialogs.editor.use_key,
                            true,
                            m.get("connection_editor.auth_key"),
                        );
                        ui.radio_value(
                            &mut app.dialogs.editor.use_key,
                            false,
                            m.get("connection_editor.auth_password"),
                        );
                    });
                    ui.end_row();

                    if app.dialogs.editor.use_key {
                        ui.label(m.get("connection_editor.key_path"));
                        ui.horizontal(|ui| {
                            ui.text_edit_singleline(&mut app.dialogs.editor.key_path);
                            if ui.button(m.get("common.browse")).clicked() {
                                if let Some(path) =
                                    ssh_key_dialog(m.get("file_dialog.select_ssh_key")).pick_file()
                                {
                                    app.dialogs.editor.key_path =
                                        path.to_string_lossy().into_owned();
                                }
                            }
                        });
                        ui.end_row();
                    } else {
                        ui.label(m.get("connection_editor.password"));
                        let password: &mut String = &mut app.dialogs.editor.password;
                        ui.add(egui::TextEdit::singleline(password).password(true));
                        ui.end_row();
                        ui.label("");
                        ui.weak(m.get("connection_editor.password_note"));
                        ui.end_row();
                    }
                });

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button(m.get("common.save")).clicked() {
                    let port: u16 = app.dialogs.editor.port.parse().unwrap_or(22);
                    let auth = if app.dialogs.editor.use_key {
                        AuthMethod::Key {
                            path: PathBuf::from(&app.dialogs.editor.key_path),
                            passphrase_required: false,
                        }
                    } else {
                        AuthMethod::Password
                    };

                    let mut encrypted_password = None;
                    if !app.dialogs.editor.use_key {
                        if app.state.master_key.is_none() {
                            if app.state.config.master_password.is_some() {
                                let unlock_required = m.get("connection_editor.unlock_required");
                                app.open_master_password_unlock_dialog(unlock_required.as_str());
                            } else {
                                let create_required = m.get("connection_editor.create_required");
                                app.open_master_password_create_dialog(create_required.as_str());
                            }
                            return;
                        }

                        if !app.dialogs.editor.password.is_empty() {
                            match crypto::encrypt_secret(
                                app.state.master_key.as_ref().expect("checked above"),
                                &app.dialogs.editor.password,
                            ) {
                                Ok(enc) => encrypted_password = Some(enc),
                                Err(e) => {
                                    log::error!("Failed to encrypt saved password: {}", e);
                                    return;
                                }
                            }
                        } else if app.dialogs.editor.existing_encrypted_password.is_some() {
                            encrypted_password =
                                app.dialogs.editor.existing_encrypted_password.clone();
                        } else {
                            let _ = rfd::MessageDialog::new()
                                .set_title(m.get("connection_editor.password_required_title"))
                                .set_description(
                                    m.get("connection_editor.password_required_description"),
                                )
                                .set_level(rfd::MessageLevel::Warning)
                                .show();
                            return;
                        }
                    }

                    if let Some(ref edit_id) = app.dialogs.editor.editing_id {
                        crate::domain::folder_tree::remove_connection(
                            &mut app.state.config.folders,
                            edit_id,
                        );
                    }

                    let conn = SavedConnection {
                        id: app
                            .dialogs
                            .editor
                            .editing_id
                            .clone()
                            .unwrap_or_else(|| app.state.config.generate_id()),
                        display_name: app.dialogs.editor.display_name.clone(),
                        host: app.dialogs.editor.host.clone(),
                        port,
                        username: app.dialogs.editor.username.clone(),
                        auth,
                        encrypted_password,
                        text_tint_rgba: app.dialogs.editor.text_tint_rgba,
                    };

                    let folder_id = app.dialogs.editor.target_folder_id.clone();
                    if let Some(folder) = crate::domain::folder_tree::find_folder_mut(
                        &mut app.state.config.folders,
                        &folder_id,
                    ) {
                        folder.connections.push(conn);
                    } else if let Some(folder) = app.state.config.folders.first_mut() {
                        folder.connections.push(conn);
                    }

                    app.state.pending.push(PendingAction::SaveConfig);
                    app.dialogs.editor.clear_secrets();
                    app.dialogs.editor.open = false;
                }
                if ui.button(m.get("common.cancel")).clicked() {
                    app.dialogs.editor.clear_secrets();
                    app.dialogs.editor.open = false;
                }
            });
        });

    if !open {
        app.dialogs.editor.clear_secrets();
        app.dialogs.editor.open = false;
    }
}
