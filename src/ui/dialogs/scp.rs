use eframe::egui;
use std::path::PathBuf;

use crate::app::App;
use crate::services::scp::{ScpAuth, ScpDirection, ScpTransferRequest};
use crate::state::{PendingAction, ScpAuthModeState, ScpDirectionState};

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
    if !app.dialogs.scp.open {
        return;
    }

    let m = app.state.messages.clone();
    let mut open = true;
    let title = m.format("scp.title", &[("name", &app.dialogs.scp.name)]);
    egui::Window::new(title)
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .min_width(640.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(m.get("scp.direction"));
                ui.radio_value(
                    &mut app.dialogs.scp.direction,
                    ScpDirectionState::LocalToRemote,
                    m.get("scp.local_to_remote"),
                );
                ui.radio_value(
                    &mut app.dialogs.scp.direction,
                    ScpDirectionState::RemoteToLocal,
                    m.get("scp.remote_to_local"),
                );
            });

            ui.separator();

            egui::Grid::new("scp_connection_grid")
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.label(m.get("scp.host"));
                    ui.text_edit_singleline(&mut app.dialogs.scp.host);
                    ui.end_row();

                    ui.label(m.get("scp.port"));
                    ui.text_edit_singleline(&mut app.dialogs.scp.port);
                    ui.end_row();

                    ui.label(m.get("scp.username"));
                    ui.text_edit_singleline(&mut app.dialogs.scp.username);
                    ui.end_row();

                    ui.label(m.get("scp.auth"));
                    ui.horizontal(|ui| {
                        ui.radio_value(
                            &mut app.dialogs.scp.auth_mode,
                            ScpAuthModeState::Key,
                            m.get("scp.auth_key"),
                        );
                        ui.radio_value(
                            &mut app.dialogs.scp.auth_mode,
                            ScpAuthModeState::Password,
                            m.get("scp.auth_password"),
                        );
                        ui.radio_value(
                            &mut app.dialogs.scp.auth_mode,
                            ScpAuthModeState::Agent,
                            m.get("scp.auth_agent"),
                        );
                    });
                    ui.end_row();

                    match app.dialogs.scp.auth_mode {
                        ScpAuthModeState::Key => {
                            ui.label(m.get("scp.key_path"));
                            ui.horizontal(|ui| {
                                ui.text_edit_singleline(&mut app.dialogs.scp.key_path);
                                if ui.button(m.get("common.browse")).clicked() {
                                    if let Some(path) =
                                        ssh_key_dialog(m.get("file_dialog.select_ssh_key"))
                                            .pick_file()
                                    {
                                        app.dialogs.scp.key_path =
                                            path.to_string_lossy().into_owned();
                                    }
                                }
                            });
                            ui.end_row();

                            ui.label(m.get("scp.passphrase"));
                            let passphrase: &mut String = &mut app.dialogs.scp.passphrase;
                            ui.add(egui::TextEdit::singleline(passphrase).password(true));
                            ui.end_row();
                        }
                        ScpAuthModeState::Password => {
                            ui.label(m.get("scp.password"));
                            let password: &mut String = &mut app.dialogs.scp.password;
                            ui.add(egui::TextEdit::singleline(password).password(true));
                            ui.end_row();
                        }
                        ScpAuthModeState::Agent => {}
                    }
                });

            ui.separator();

            match app.dialogs.scp.direction {
                ScpDirectionState::LocalToRemote => {
                    ui.label(m.get("scp.local_sources"));
                    egui::ScrollArea::vertical()
                        .max_height(120.0)
                        .show(ui, |ui| {
                            if app.dialogs.scp.local_sources.is_empty() {
                                ui.weak(m.get("scp.no_local_sources"));
                            } else {
                                for source in &app.dialogs.scp.local_sources {
                                    ui.label(source);
                                }
                            }
                        });
                    ui.horizontal(|ui| {
                        if ui.button(m.get("scp.add_files")).clicked() {
                            if let Some(files) = rfd::FileDialog::new()
                                .set_title(m.get("file_dialog.select_files_to_transfer"))
                                .pick_files()
                            {
                                for path in files {
                                    app.dialogs
                                        .scp
                                        .local_sources
                                        .push(path.to_string_lossy().into_owned());
                                }
                            }
                        }
                        if ui.button(m.get("scp.add_folder")).clicked() {
                            if let Some(folder) = rfd::FileDialog::new()
                                .set_title(m.get("file_dialog.select_folder_to_transfer"))
                                .pick_folder()
                            {
                                app.dialogs
                                    .scp
                                    .local_sources
                                    .push(folder.to_string_lossy().into_owned());
                            }
                        }
                        if ui.button(m.get("common.clear")).clicked() {
                            app.dialogs.scp.local_sources.clear();
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label(m.get("scp.remote_destination"));
                        ui.text_edit_singleline(&mut app.dialogs.scp.remote_destination);
                    });
                }
                ScpDirectionState::RemoteToLocal => {
                    ui.label(m.get("scp.remote_sources"));
                    ui.add(
                        egui::TextEdit::multiline(&mut app.dialogs.scp.remote_sources)
                            .desired_rows(5)
                            .hint_text(m.get("scp.remote_sources_hint")),
                    );

                    ui.horizontal(|ui| {
                        ui.label(m.get("scp.local_destination"));
                        ui.text_edit_singleline(&mut app.dialogs.scp.local_destination);
                        if ui.button(m.get("common.browse")).clicked() {
                            if let Some(folder) = rfd::FileDialog::new()
                                .set_title(m.get("file_dialog.select_destination_folder"))
                                .pick_folder()
                            {
                                app.dialogs.scp.local_destination =
                                    folder.to_string_lossy().into_owned();
                            }
                        }
                    });
                }
            }

            let host_ok = !app.dialogs.scp.host.trim().is_empty();
            let user_ok = !app.dialogs.scp.username.trim().is_empty();
            let direction_ok = match app.dialogs.scp.direction {
                ScpDirectionState::LocalToRemote => {
                    !app.dialogs.scp.local_sources.is_empty()
                        && !app.dialogs.scp.remote_destination.trim().is_empty()
                }
                ScpDirectionState::RemoteToLocal => {
                    !parse_remote_sources(&app.dialogs.scp.remote_sources).is_empty()
                        && !app.dialogs.scp.local_destination.trim().is_empty()
                }
            };

            let auth_ok = match app.dialogs.scp.auth_mode {
                ScpAuthModeState::Key => !app.dialogs.scp.key_path.trim().is_empty(),
                ScpAuthModeState::Password => !app.dialogs.scp.password.is_empty(),
                ScpAuthModeState::Agent => true,
            };

            let can_start = host_ok && user_ok && direction_ok && auth_ok;

            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(can_start, egui::Button::new(m.get("scp.start")))
                    .clicked()
                {
                    let port = app.dialogs.scp.port.parse().unwrap_or(22);
                    let direction = match app.dialogs.scp.direction {
                        ScpDirectionState::LocalToRemote => ScpDirection::LocalToRemote,
                        ScpDirectionState::RemoteToLocal => ScpDirection::RemoteToLocal,
                    };
                    let auth = match app.dialogs.scp.auth_mode {
                        ScpAuthModeState::Key => ScpAuth::Key {
                            path: PathBuf::from(app.dialogs.scp.key_path.clone()),
                            passphrase: if app.dialogs.scp.passphrase.is_empty() {
                                None
                            } else {
                                Some(zeroize::Zeroizing::new(
                                    app.dialogs.scp.passphrase.to_string(),
                                ))
                            },
                        },
                        ScpAuthModeState::Password => {
                            ScpAuth::Password(app.dialogs.scp.password.clone())
                        }
                        ScpAuthModeState::Agent => ScpAuth::Agent,
                    };

                    let (sources, destination) = match direction {
                        ScpDirection::LocalToRemote => (
                            app.dialogs.scp.local_sources.clone(),
                            app.dialogs.scp.remote_destination.clone(),
                        ),
                        ScpDirection::RemoteToLocal => (
                            parse_remote_sources(&app.dialogs.scp.remote_sources),
                            app.dialogs.scp.local_destination.clone(),
                        ),
                    };

                    app.state
                        .pending
                        .push(PendingAction::RunScp(ScpTransferRequest {
                            name: app.dialogs.scp.name.clone(),
                            host: app.dialogs.scp.host.clone(),
                            port,
                            username: app.dialogs.scp.username.clone(),
                            auth,
                            direction,
                            sources,
                            destination,
                        }));

                    app.dialogs.scp.clear_secrets();
                    app.dialogs.scp.open = false;
                }

                if ui.button(m.get("common.cancel")).clicked() {
                    app.dialogs.scp.clear_secrets();
                    app.dialogs.scp.open = false;
                }
            });
        });

    if !open {
        app.dialogs.scp.clear_secrets();
        app.dialogs.scp.open = false;
    }
}

fn parse_remote_sources(input: &str) -> Vec<String> {
    input
        .lines()
        .flat_map(|line| line.split(','))
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
        .collect()
}
