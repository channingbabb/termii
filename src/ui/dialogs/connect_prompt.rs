use eframe::egui;
use std::path::PathBuf;

use crate::app::App;
use crate::services::ssh::AuthConfig;
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
    if !app.dialogs.connect_prompt.open {
        return;
    }

    let m = app.state.messages.clone();
    let mut open = true;
    let title = m.format(
        "connect_prompt.title",
        &[("name", &app.dialogs.connect_prompt.device_name)],
    );
    egui::Window::new(title)
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .min_width(380.0)
        .show(ctx, |ui| {
            egui::Grid::new("connect_prompt_grid")
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.label(m.get("connect_prompt.host"));
                    if !app.dialogs.connect_prompt.dns_name.is_empty() {
                        ui.horizontal(|ui| {
                            ui.radio_value(
                                &mut app.dialogs.connect_prompt.use_dns,
                                false,
                                &app.dialogs.connect_prompt.host,
                            );
                            ui.radio_value(
                                &mut app.dialogs.connect_prompt.use_dns,
                                true,
                                &app.dialogs.connect_prompt.dns_name,
                            );
                        });
                    } else {
                        ui.text_edit_singleline(&mut app.dialogs.connect_prompt.host);
                    }
                    ui.end_row();

                    ui.label(m.get("connect_prompt.port"));
                    ui.text_edit_singleline(&mut app.dialogs.connect_prompt.port);
                    ui.end_row();

                    ui.label(m.get("connect_prompt.username"));
                    ui.text_edit_singleline(&mut app.dialogs.connect_prompt.username);
                    ui.end_row();

                    ui.label(m.get("connect_prompt.auth"));
                    ui.horizontal(|ui| {
                        ui.radio_value(
                            &mut app.dialogs.connect_prompt.use_key,
                            true,
                            m.get("connect_prompt.auth_key"),
                        );
                        ui.radio_value(
                            &mut app.dialogs.connect_prompt.use_key,
                            false,
                            m.get("connect_prompt.auth_password"),
                        );
                    });
                    ui.end_row();

                    if app.dialogs.connect_prompt.use_key {
                        ui.label(m.get("connect_prompt.key_path"));
                        ui.horizontal(|ui| {
                            ui.text_edit_singleline(&mut app.dialogs.connect_prompt.key_path);
                            if ui.button(m.get("common.browse")).clicked() {
                                if let Some(path) =
                                    ssh_key_dialog(m.get("file_dialog.select_ssh_key")).pick_file()
                                {
                                    app.dialogs.connect_prompt.key_path =
                                        path.to_string_lossy().into_owned();
                                }
                            }
                        });
                        ui.end_row();

                        ui.label(m.get("connect_prompt.passphrase"));
                        let passphrase: &mut String = &mut app.dialogs.connect_prompt.passphrase;
                        ui.add(egui::TextEdit::singleline(passphrase).password(true));
                        ui.end_row();
                    } else {
                        ui.label(m.get("connect_prompt.password"));
                        let password: &mut String = &mut app.dialogs.connect_prompt.password;
                        ui.add(egui::TextEdit::singleline(password).password(true));
                        ui.end_row();
                    }
                });

            if app.dialogs.connect_prompt.username.is_empty() {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    m.get("connect_prompt.username_required"),
                );
            }

            ui.separator();
            ui.horizontal(|ui| {
                let can_connect = !app.dialogs.connect_prompt.username.is_empty()
                    && !app.dialogs.connect_prompt.host.is_empty();

                if ui
                    .add_enabled(
                        can_connect,
                        egui::Button::new(m.get("connect_prompt.connect")),
                    )
                    .clicked()
                {
                    let host = if app.dialogs.connect_prompt.use_dns
                        && !app.dialogs.connect_prompt.dns_name.is_empty()
                    {
                        app.dialogs.connect_prompt.dns_name.clone()
                    } else {
                        app.dialogs.connect_prompt.host.clone()
                    };
                    let port: u16 = app.dialogs.connect_prompt.port.parse().unwrap_or(22);

                    let auth = if app.dialogs.connect_prompt.use_key {
                        AuthConfig::Key {
                            path: PathBuf::from(&app.dialogs.connect_prompt.key_path),
                            passphrase: if app.dialogs.connect_prompt.passphrase.is_empty() {
                                None
                            } else {
                                Some(zeroize::Zeroizing::new(
                                    app.dialogs.connect_prompt.passphrase.to_string(),
                                ))
                            },
                        }
                    } else {
                        AuthConfig::Password(app.dialogs.connect_prompt.password.clone())
                    };

                    app.state.pending.push(PendingAction::ConnectRaw {
                        name: app.dialogs.connect_prompt.device_name.clone(),
                        host,
                        port,
                        username: app.dialogs.connect_prompt.username.clone(),
                        auth,
                        forwards: Vec::new(),
                    });
                    app.dialogs.connect_prompt.clear_secrets();
                    app.dialogs.connect_prompt.open = false;
                }
                if ui.button(m.get("common.cancel")).clicked() {
                    app.dialogs.connect_prompt.clear_secrets();
                    app.dialogs.connect_prompt.open = false;
                }
            });
        });

    if !open {
        app.dialogs.connect_prompt.clear_secrets();
        app.dialogs.connect_prompt.open = false;
    }
}
