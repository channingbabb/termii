use eframe::egui;
use std::collections::{BTreeMap, HashSet};

use crate::app::App;
use crate::domain::connection::AuthMethod;
use crate::services::config::Folder;
use crate::services::crypto;
use crate::services::ssh_keys::default_key_path;
use crate::services::tailscale::{self, TailscaleDevice};
use crate::state::{PendingAction, ScpAuthModeState, ScpDirectionState};
use zeroize::Zeroizing;

const TINT_PRESETS: [(&str, [u8; 4]); 8] = [
    ("Red", [220, 68, 55, 255]),
    ("Orange", [230, 145, 56, 255]),
    ("Yellow", [220, 180, 32, 255]),
    ("Green", [90, 180, 90, 255]),
    ("Cyan", [70, 170, 180, 255]),
    ("Blue", [80, 140, 220, 255]),
    ("Pink", [210, 90, 150, 255]),
    ("Gray", [150, 150, 150, 255]),
];

pub fn render_tree(ui: &mut egui::Ui, app: &mut App) {
    let m = app.state.messages.clone();
    let filter = app.state.search_query.to_lowercase();
    let folders = app.state.config.folders.clone();
    for folder in &folders {
        render_folder(ui, app, folder, &filter);
    }

    if ui.small_button(m.get("tree.add_folder")).clicked() {
        app.state
            .pending
            .push(PendingAction::NewFolder(String::new()));
    }

    if !app.state.config.disable_tailscale_auto_discovery {
        ui.separator();
        let devices = app.state.tailscale_devices.lock().unwrap().clone();
        let hidden: HashSet<String> = app
            .state
            .config
            .hidden_tailscale_devices
            .iter()
            .cloned()
            .collect();
        let peers: Vec<&TailscaleDevice> = devices
            .iter()
            .filter(|d| !d.is_self)
            .filter(|d| !hidden.contains(&d.hostname))
            .collect();
        let peer_count = peers.len().to_string();
        let groups = app.state.config.tailscale_groups.clone();
        let hidden_sections: HashSet<String> = app
            .state
            .config
            .hidden_tailscale_sections
            .iter()
            .cloned()
            .collect();

        let ts_header_resp = egui::CollapsingHeader::new(
            m.format("tree.tailscale_header", &[("count", &peer_count)]),
        )
        .default_open(true)
        .show(ui, |ui| {
            if peers.is_empty() {
                ui.label(m.get("tree.no_tailscale_devices"));
                return;
            }
            let mut grouped_ids: HashSet<String> = HashSet::new();
            for rule in &groups {
                let matched: Vec<&TailscaleDevice> = peers
                    .iter()
                    .filter(|d| tailscale::glob_match(&rule.pattern, &d.hostname))
                    .filter(|d| filter.is_empty() || d.hostname.to_lowercase().contains(&filter))
                    .copied()
                    .collect();

                if matched.is_empty() && !filter.is_empty() {
                    continue;
                }
                let section_id = custom_group_section_id(&rule.id);
                if hidden_sections.contains(&section_id) {
                    for device in &matched {
                        grouped_ids.insert(device.hostname.clone());
                    }
                    continue;
                }

                let matched_count = matched.len().to_string();
                let group_header = egui::CollapsingHeader::new(m.format(
                    "tree.group_header",
                    &[
                        ("name", &rule.name),
                        ("count", &matched_count),
                        ("pattern", &rule.pattern),
                    ],
                ))
                .default_open(true)
                .show(ui, |ui| {
                    for device in &matched {
                        grouped_ids.insert(device.hostname.clone());
                        render_ts_device(ui, app, device);
                    }
                    if matched.is_empty() {
                        ui.label(m.get("tree.no_matches"));
                    }
                });
                group_header.header_response.context_menu(|ui| {
                    if ui.button(m.get("tree.edit_group")).clicked() {
                        app.dialogs.ts_group_editor.open = true;
                        app.dialogs.ts_group_editor.editing_id = Some(rule.id.clone());
                        app.dialogs.ts_group_editor.name = rule.name.clone();
                        app.dialogs.ts_group_editor.pattern = rule.pattern.clone();
                        ui.close_menu();
                    }
                    if ui.button(m.get("tree.delete_group")).clicked() {
                        app.state
                            .pending
                            .push(PendingAction::DeleteTailscaleGroup(rule.id.clone()));
                        ui.close_menu();
                    }
                    if ui.button(m.get("tree.hide_section")).clicked() {
                        app.state
                            .pending
                            .push(PendingAction::HideTailscaleSection(section_id.clone()));
                        ui.close_menu();
                    }
                });
            }
            let ungrouped: Vec<&TailscaleDevice> = peers
                .iter()
                .filter(|d| !grouped_ids.contains(&d.hostname))
                .filter(|d| filter.is_empty() || d.hostname.to_lowercase().contains(&filter))
                .copied()
                .collect();
            let mut by_owner: BTreeMap<String, Vec<&TailscaleDevice>> = BTreeMap::new();
            for device in &ungrouped {
                let key = if device.owner.is_empty() {
                    m.get("tree.unknown_owner").to_string()
                } else {
                    device.owner.clone()
                };
                by_owner.entry(key).or_default().push(device);
            }

            if by_owner.len() <= 1 {
                for device in &ungrouped {
                    render_ts_device(ui, app, device);
                }
            } else {
                for (owner, owner_devices) in &by_owner {
                    let owner_count = owner_devices.len().to_string();
                    egui::CollapsingHeader::new(m.format(
                        "tree.owner_header",
                        &[("owner", owner.as_str()), ("count", &owner_count)],
                    ))
                    .default_open(true)
                    .show(ui, |ui| {
                        for device in owner_devices {
                            render_ts_device(ui, app, device);
                        }
                    });
                }
            }
        });
        ts_header_resp.header_response.context_menu(|ui| {
            if ui.button(m.get("tree.new_device_group")).clicked() {
                app.dialogs.ts_group_editor = crate::state::TsGroupEditorState {
                    open: true,
                    name: String::new(),
                    pattern: String::new(),
                    editing_id: None,
                };
                ui.close_menu();
            }
            if !app.state.config.hidden_tailscale_devices.is_empty()
                && ui.button(m.get("tree.unhide_all_devices")).clicked()
            {
                app.state
                    .pending
                    .push(PendingAction::UnhideAllTailscaleDevices);
                ui.close_menu();
            }
            if !app.state.config.hidden_tailscale_sections.is_empty()
                && ui.button(m.get("tree.unhide_all_sections")).clicked()
            {
                app.state
                    .pending
                    .push(PendingAction::UnhideAllTailscaleSections);
                ui.close_menu();
            }
            if !groups.is_empty() {
                ui.separator();
                ui.label(m.get("tree.existing_groups"));
                for rule in &groups {
                    ui.horizontal(|ui| {
                        ui.label(m.format(
                            "tree.existing_group_entry",
                            &[("name", &rule.name), ("pattern", &rule.pattern)],
                        ));
                        if ui.small_button(m.get("tree.edit_short")).clicked() {
                            app.dialogs.ts_group_editor.open = true;
                            app.dialogs.ts_group_editor.editing_id = Some(rule.id.clone());
                            app.dialogs.ts_group_editor.name = rule.name.clone();
                            app.dialogs.ts_group_editor.pattern = rule.pattern.clone();
                            ui.close_menu();
                        }
                        if ui.small_button(m.get("tree.delete_short")).clicked() {
                            app.state
                                .pending
                                .push(PendingAction::DeleteTailscaleGroup(rule.id.clone()));
                            ui.close_menu();
                        }
                    });
                }
            }
        });
    }

    if app.dialogs.dragging_connection.is_some() && ui.input(|i| i.pointer.any_released()) {
        app.dialogs.dragging_connection = None;
    }
}

fn render_ts_device(ui: &mut egui::Ui, app: &mut App, device: &TailscaleDevice) {
    let m = app.state.messages.clone();
    let ip_str = device.addresses.first().map(|s| s.as_str()).unwrap_or("?");
    let label = m.format(
        "tree.device_entry",
        &[
            ("hostname", &device.hostname),
            ("os", &device.os),
            ("ip", ip_str),
        ],
    );
    let resp = ui
        .selectable_label(false, &label)
        .on_hover_cursor(egui::CursorIcon::PointingHand);

    if resp.double_clicked() {
        app.state
            .pending
            .push(PendingAction::OpenConnectPrompt(device.clone()));
    }

    resp.context_menu(|ui| {
        if ui.button(m.get("tree.connect_ellipsis")).clicked() {
            app.state
                .pending
                .push(PendingAction::OpenConnectPrompt(device.clone()));
            ui.close_menu();
        }
        if ui.button(m.get("tree.add_saved_connection")).clicked() {
            app.state
                .pending
                .push(PendingAction::AddTailscaleAsSaved(device.clone()));
            ui.close_menu();
        }
        if ui.button(m.get("tree.scp_ellipsis")).clicked() {
            open_scp_for_tailscale(app, device);
            ui.close_menu();
        }
        if ui.button(m.get("tree.hide")).clicked() {
            app.state
                .pending
                .push(PendingAction::HideTailscaleDevice(device.hostname.clone()));
            ui.close_menu();
        }
        ui.separator();
        if !device.owner.is_empty() {
            ui.label(m.format("tree.owner_value", &[("owner", &device.owner)]));
        }
        if !device.dns_name.is_empty() {
            ui.label(m.format("tree.dns_value", &[("dns", &device.dns_name)]));
        }
        for addr in &device.addresses {
            ui.label(m.format("tree.ip_value", &[("ip", addr)]));
        }
        if let Some(ref seen) = device.last_seen {
            ui.label(m.format("tree.last_seen_value", &[("value", seen)]));
        }
    });
}

fn render_folder(ui: &mut egui::Ui, app: &mut App, folder: &Folder, filter: &str) {
    let m = app.state.messages.clone();
    let folder_id = folder.id.clone();
    let folder_name = folder.name.clone();

    let has_matches = filter.is_empty()
        || folder.name.to_lowercase().contains(filter)
        || folder
            .connections
            .iter()
            .any(|c| c.display_name.to_lowercase().contains(filter))
        || folder.subfolders.iter().any(|f| folder_matches(f, filter));

    if !has_matches {
        return;
    }

    let folder_header_text = tinted_rich_text(
        m.format("tree.folder_header", &[("name", &folder_name)]),
        folder.text_tint_rgba,
    );
    let header = egui::CollapsingHeader::new(folder_header_text)
            .default_open(true)
            .show(ui, |ui| {
                for conn in &folder.connections {
                    if !filter.is_empty() && !conn.display_name.to_lowercase().contains(filter) {
                        continue;
                    }
                    ui.add_space(3.0);
                    let label = tinted_rich_text(
                        m.format(
                        "tree.connection_entry",
                        &[
                            ("name", &conn.display_name),
                            ("username", &conn.username),
                            ("host", &conn.host),
                        ],
                        ),
                        conn.text_tint_rgba,
                    );
                    let resp = ui
                        .add(egui::Label::new(label).sense(egui::Sense::click_and_drag()))
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if resp.drag_started() {
                        app.dialogs.dragging_connection =
                            Some((conn.id.clone(), folder_id.clone()));
                    }

                    if resp.double_clicked() {
                        app.state
                            .pending
                            .push(PendingAction::ConnectSaved(conn.clone()));
                    }

                    resp.context_menu(|ui| {
                        if ui.button(m.get("tree.connect")).clicked() {
                            app.state
                                .pending
                                .push(PendingAction::ConnectSaved(conn.clone()));
                            ui.close_menu();
                        }
                        if ui.button(m.get("tree.connect_with_options")).clicked() {
                            app.state
                                .pending
                                .push(PendingAction::OpenConnectPromptForSaved(conn.clone()));
                            ui.close_menu();
                        }
                        if ui.button(m.get("tree.scp_ellipsis")).clicked() {
                            open_scp_for_saved(app, conn);
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button(m.get("common.edit")).clicked() {
                            app.state.pending.push(PendingAction::OpenEditorEdit(
                                conn.clone(),
                                folder_id.clone(),
                            ));
                            ui.close_menu();
                        }
                        if ui.button(m.get("tree.duplicate")).clicked() {
                            let mut dup = conn.clone();
                            dup.id = uuid::Uuid::new_v4().to_string();
                            dup.display_name =
                                m.format("tree.copy_suffix_name", &[("name", &conn.display_name)]);
                            app.state
                                .pending
                                .push(PendingAction::OpenEditorEdit(dup, folder_id.clone()));
                            ui.close_menu();
                        }
                        if ui.button(m.get("common.delete")).clicked() {
                            app.state
                                .pending
                                .push(PendingAction::DeleteConnection(conn.id.clone()));
                            ui.close_menu();
                        }
                        if let Some(tint) = render_tint_menu(
                            ui,
                            &m.get("tree.color_code"),
                            &m.get("tree.clear_color_code"),
                        ) {
                            if let Some(target_conn) = crate::domain::folder_tree::find_connection_mut(
                                &mut app.state.config.folders,
                                &conn.id,
                            ) {
                                target_conn.text_tint_rgba = tint;
                                app.state.pending.push(PendingAction::SaveConfig);
                            }
                            ui.close_menu();
                        }
                    });
                    ui.add_space(3.0);
                }

                for sub in &folder.subfolders {
                    render_folder(ui, app, sub, filter);
                }
            });

    if let Some((conn_id, source_folder_id)) = app.dialogs.dragging_connection.clone() {
        let drop_released = ui.input(|i| i.pointer.any_released());
        if drop_released && header.header_response.hovered() && source_folder_id != folder_id {
            app.state
                .pending
                .push(PendingAction::MoveConnectionToFolder {
                    conn_id,
                    target_folder_id: folder_id.clone(),
                });
            app.dialogs.dragging_connection = None;
        }
    }

    header.header_response.context_menu(|ui| {
        if ui.button(m.get("tree.new_connection")).clicked() {
            app.state
                .pending
                .push(PendingAction::OpenEditorNew(folder_id.clone()));
            ui.close_menu();
        }
        if ui.button(m.get("tree.new_subfolder")).clicked() {
            app.state
                .pending
                .push(PendingAction::NewFolder(folder_id.clone()));
            ui.close_menu();
        }
        if ui.button(m.get("common.rename")).clicked() {
            app.dialogs.rename_folder = Some((folder_id.clone(), folder_name.clone()));
            ui.close_menu();
        }
        if ui.button(m.get("tree.delete_folder")).clicked() {
            app.state
                .pending
                .push(PendingAction::DeleteFolder(folder_id.clone()));
            ui.close_menu();
        }
        if let Some(tint) =
            render_tint_menu(ui, &m.get("tree.color_code"), &m.get("tree.clear_color_code"))
        {
            if let Some(target_folder) =
                crate::domain::folder_tree::find_folder_mut(&mut app.state.config.folders, &folder_id)
            {
                target_folder.text_tint_rgba = tint;
                app.state.pending.push(PendingAction::SaveConfig);
            }
            ui.close_menu();
        }
        if ui
            .button(m.get("tree.propagate_folder_color_to_instances"))
            .clicked()
        {
            let folder_tint = crate::domain::folder_tree::find_folder_mut(
                &mut app.state.config.folders,
                &folder_id,
            )
            .and_then(|target_folder| target_folder.text_tint_rgba);
            if crate::domain::folder_tree::set_direct_child_connection_text_tint(
                &mut app.state.config.folders,
                &folder_id,
                folder_tint,
            ) {
                app.state.pending.push(PendingAction::SaveConfig);
            }
            ui.close_menu();
        }
    });

    if let Some((ref rename_id, ref mut name_buf)) = app.dialogs.rename_folder {
        if *rename_id == folder_id {
            let mut submitted = false;
            ui.horizontal(|ui| {
                ui.label(m.get("tree.rename_label"));
                let resp = ui.text_edit_singleline(name_buf);
                if resp.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if let Some(f) = crate::domain::folder_tree::find_folder_mut(
                        &mut app.state.config.folders,
                        &folder_id,
                    ) {
                        f.name = name_buf.clone();
                    }
                    app.state.pending.push(PendingAction::SaveConfig);
                    submitted = true;
                }
            });
            if submitted {
                app.dialogs.rename_folder = None;
            }
        }
    }
}

fn tinted_rich_text(text: String, tint: Option<[u8; 4]>) -> egui::RichText {
    if let Some(color) = tint.and_then(color_from_tint) {
        return egui::RichText::new(text).color(color);
    }
    egui::RichText::new(text)
}

fn color_from_tint(rgba: [u8; 4]) -> Option<egui::Color32> {
    if rgba[3] == 0 {
        return None;
    }
    Some(egui::Color32::from_rgba_premultiplied(
        rgba[0], rgba[1], rgba[2], rgba[3],
    ))
}

fn render_tint_menu(
    ui: &mut egui::Ui,
    menu_label: &str,
    clear_label: &str,
) -> Option<Option<[u8; 4]>> {
    let mut selected = None;
    ui.menu_button(menu_label, |ui| {
        if ui.button(clear_label).clicked() {
            selected = Some(None);
            ui.close_menu();
        }
        ui.separator();
        for (name, rgba) in TINT_PRESETS {
            if ui.button(name).clicked() {
                selected = Some(Some(rgba));
                ui.close_menu();
            }
        }
    });
    selected
}

fn folder_matches(folder: &Folder, filter: &str) -> bool {
    folder.name.to_lowercase().contains(filter)
        || folder
            .connections
            .iter()
            .any(|c| c.display_name.to_lowercase().contains(filter))
        || folder.subfolders.iter().any(|f| folder_matches(f, filter))
}

fn open_scp_for_saved(app: &mut App, conn: &crate::domain::connection::SavedConnection) {
    app.dialogs.scp.clear_secrets();
    let decrypted_saved_password: Option<Zeroizing<String>> = if let (Some(key), Some(enc)) = (
        app.state.master_key.as_ref(),
        conn.encrypted_password.as_ref(),
    ) {
        crypto::decrypt_secret(key, enc).ok().map(Zeroizing::new)
    } else {
        None
    };

    let (auth_mode, key_path) = match &conn.auth {
        AuthMethod::Key { path, .. } => {
            let p = path.to_string_lossy().into_owned();
            let resolved = if p.is_empty() { default_key_path() } else { p };
            (ScpAuthModeState::Key, resolved)
        }
        AuthMethod::Password => (ScpAuthModeState::Password, default_key_path()),
        AuthMethod::Agent => (ScpAuthModeState::Agent, default_key_path()),
    };

    app.dialogs.scp.open = true;
    app.dialogs.scp.name = conn.display_name.clone();
    app.dialogs.scp.host = conn.host.clone();
    app.dialogs.scp.port = conn.port.to_string();
    app.dialogs.scp.username = conn.username.clone();
    app.dialogs.scp.auth_mode = auth_mode;
    app.dialogs.scp.key_path = key_path;
    app.dialogs.scp.passphrase.clear();
    app.dialogs.scp.password =
        decrypted_saved_password.unwrap_or_else(|| Zeroizing::new(String::new()));
    app.dialogs.scp.direction = ScpDirectionState::LocalToRemote;
    app.dialogs.scp.local_sources.clear();
    app.dialogs.scp.remote_sources.clear();
}

fn open_scp_for_tailscale(app: &mut App, device: &TailscaleDevice) {
    app.dialogs.scp.clear_secrets();
    app.dialogs.scp.open = true;
    app.dialogs.scp.name = device.hostname.clone();
    app.dialogs.scp.host = device.addresses.first().cloned().unwrap_or_default();
    app.dialogs.scp.port = "22".into();
    app.dialogs.scp.username.clear();
    app.dialogs.scp.auth_mode = ScpAuthModeState::Key;
    app.dialogs.scp.key_path = default_key_path();
    app.dialogs.scp.passphrase.clear();
    app.dialogs.scp.password.clear();
    app.dialogs.scp.direction = ScpDirectionState::LocalToRemote;
    app.dialogs.scp.local_sources.clear();
    app.dialogs.scp.remote_sources.clear();
}

fn custom_group_section_id(group_id: &str) -> String {
    format!("custom:{}", group_id)
}
