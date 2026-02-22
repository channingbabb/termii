use eframe::egui;

use crate::app::App;
use crate::domain::types::ForwardType;
use crate::services::ssh::PortForwardRule;
use crate::state::PendingAction;

pub fn render(ctx: &egui::Context, app: &mut App) {
    if !app.dialogs.session_settings.open {
        return;
    }

    let m = app.state.messages.clone();
    let mut open = true;
    egui::Window::new(m.get("session_settings.title"))
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(760.0)
        .default_height(520.0)
        .show(ctx, |ui| {
            ui.heading(m.get("session_settings.port_forwarding"));
            ui.label(m.get("session_settings.description"));
            ui.separator();

            ui.label(m.get("session_settings.active_rules"));
            egui::Grid::new("session_forward_rules")
                .num_columns(6)
                .spacing([8.0, 6.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.strong(m.get("session_settings.rule_type"));
                    ui.strong(m.get("session_settings.rule_listen"));
                    ui.strong(m.get("session_settings.rule_target"));
                    ui.strong(m.get("session_settings.rule_enabled"));
                    ui.strong(" ");
                    ui.end_row();

                    let mut remove_idx: Option<usize> = None;
                    for (idx, rule) in app.dialogs.session_settings.rules.iter_mut().enumerate() {
                        let kind = match rule.kind {
                            ForwardType::Local => m.get("session_settings.local_forward"),
                            ForwardType::Remote => m.get("session_settings.reverse_forward"),
                        };
                        ui.label(kind);
                        ui.label(format!("{}:{}", rule.listen_host, rule.listen_port));
                        ui.label(format!("{}:{}", rule.target_host, rule.target_port));
                        ui.checkbox(&mut rule.enabled, "");
                        if ui.button(m.get("session_settings.remove")).clicked() {
                            remove_idx = Some(idx);
                        }
                        ui.end_row();
                    }
                    if let Some(idx) = remove_idx {
                        app.dialogs.session_settings.rules.remove(idx);
                    }
                });

            ui.separator();
            ui.collapsing(m.get("session_settings.add_local_section"), |ui| {
                egui::Grid::new("add_local_forward")
                    .num_columns(4)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        ui.label(m.get("session_settings.listen_host"));
                        ui.text_edit_singleline(
                            &mut app.dialogs.session_settings.local_listen_host,
                        );
                        ui.label(m.get("session_settings.listen_port"));
                        ui.text_edit_singleline(
                            &mut app.dialogs.session_settings.local_listen_port,
                        );
                        ui.end_row();

                        ui.label(m.get("session_settings.target_host"));
                        ui.text_edit_singleline(
                            &mut app.dialogs.session_settings.local_target_host,
                        );
                        ui.label(m.get("session_settings.target_port"));
                        ui.text_edit_singleline(
                            &mut app.dialogs.session_settings.local_target_port,
                        );
                        ui.end_row();
                    });

                if ui
                    .button(m.get("session_settings.add_local_button"))
                    .clicked()
                {
                    let listen_port = app
                        .dialogs
                        .session_settings
                        .local_listen_port
                        .parse::<u16>();
                    let target_port = app
                        .dialogs
                        .session_settings
                        .local_target_port
                        .parse::<u16>();
                    if let (Ok(listen_port), Ok(target_port)) = (listen_port, target_port) {
                        app.dialogs.session_settings.rules.push(PortForwardRule {
                            id: uuid::Uuid::new_v4().to_string(),
                            kind: ForwardType::Local,
                            listen_host: app
                                .dialogs
                                .session_settings
                                .local_listen_host
                                .trim()
                                .to_string(),
                            listen_port,
                            target_host: app
                                .dialogs
                                .session_settings
                                .local_target_host
                                .trim()
                                .to_string(),
                            target_port,
                            enabled: true,
                        });
                    }
                }
            });

            ui.separator();
            ui.collapsing(m.get("session_settings.add_reverse_section"), |ui| {
                egui::Grid::new("add_remote_forward")
                    .num_columns(4)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        ui.label(m.get("session_settings.remote_listen_host"));
                        ui.text_edit_singleline(
                            &mut app.dialogs.session_settings.remote_listen_host,
                        );
                        ui.label(m.get("session_settings.remote_listen_port"));
                        ui.text_edit_singleline(
                            &mut app.dialogs.session_settings.remote_listen_port,
                        );
                        ui.end_row();

                        ui.label(m.get("session_settings.local_target_host"));
                        ui.text_edit_singleline(
                            &mut app.dialogs.session_settings.remote_target_host,
                        );
                        ui.label(m.get("session_settings.local_target_port"));
                        ui.text_edit_singleline(
                            &mut app.dialogs.session_settings.remote_target_port,
                        );
                        ui.end_row();
                    });

                if ui
                    .button(m.get("session_settings.add_reverse_button"))
                    .clicked()
                {
                    let listen_port = app
                        .dialogs
                        .session_settings
                        .remote_listen_port
                        .parse::<u16>();
                    let target_port = app
                        .dialogs
                        .session_settings
                        .remote_target_port
                        .parse::<u16>();
                    if let (Ok(listen_port), Ok(target_port)) = (listen_port, target_port) {
                        app.dialogs.session_settings.rules.push(PortForwardRule {
                            id: uuid::Uuid::new_v4().to_string(),
                            kind: ForwardType::Remote,
                            listen_host: app
                                .dialogs
                                .session_settings
                                .remote_listen_host
                                .trim()
                                .to_string(),
                            listen_port,
                            target_host: app
                                .dialogs
                                .session_settings
                                .remote_target_host
                                .trim()
                                .to_string(),
                            target_port,
                            enabled: true,
                        });
                    }
                }
            });

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button(m.get("common.apply")).clicked() {
                    if let Some(idx) = app.dialogs.session_settings.tab_idx {
                        app.state.pending.push(PendingAction::SaveSessionSettings {
                            idx,
                            rules: app.dialogs.session_settings.rules.clone(),
                        });
                    }
                    app.dialogs.session_settings.open = false;
                }
                if ui.button(m.get("common.cancel")).clicked() {
                    app.dialogs.session_settings.open = false;
                }
            });
        });

    if !open {
        app.dialogs.session_settings.open = false;
    }
}
