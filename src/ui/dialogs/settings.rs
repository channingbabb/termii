use std::sync::atomic::Ordering;

use eframe::egui;

use crate::app::App;
use crate::domain::connection::SavedConnection;
use crate::services::config::Folder;
use crate::services::config::ThemePreference;
use crate::state::PendingAction;

pub fn render(ctx: &egui::Context, app: &mut App) {
    if !app.dialogs.settings.open {
        return;
    }

    let m = app.state.messages.clone();
    let mut open = true;
    egui::Window::new(m.get("settings.title"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .min_width(460.0)
        .show(ctx, |ui| {
            ui.heading(m.get("settings.servers"));
            ui.horizontal(|ui| {
                if ui.button(m.get("settings.export_servers")).clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_title(m.get("settings.export_servers"))
                        .add_filter(m.get("settings.filter_json"), &["json"])
                        .set_file_name(m.get("settings.export_default_filename"))
                        .save_file()
                    {
                        app.state.pending.push(PendingAction::ExportServers(path));
                    }
                }

                if ui.button(m.get("settings.import_servers")).clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_title(m.get("settings.import_servers"))
                        .add_filter(m.get("settings.filter_supported"), &["json", "xml"])
                        .add_filter(m.get("settings.filter_json"), &["json"])
                        .add_filter(m.get("settings.filter_superputty_xml"), &["xml"])
                        .pick_file()
                    {
                        app.state.pending.push(PendingAction::ImportServers(path));
                    }
                }
            });

            ui.separator();
            ui.heading(m.get("settings.tailscale"));
            let mut enable_discovery = !app.state.config.disable_tailscale_auto_discovery;
            if ui
                .checkbox(
                    &mut enable_discovery,
                    m.get("settings.enable_tailscale_discovery"),
                )
                .changed()
            {
                app.state.config.disable_tailscale_auto_discovery = !enable_discovery;
                app.state
                    .tailscale_discovery_enabled
                    .store(enable_discovery, Ordering::Relaxed);
                app.state.pending.push(PendingAction::SaveConfig);
            }

            ui.separator();
            ui.heading(m.get("settings.connections"));
            if ui
                .checkbox(
                    &mut app.state.config.connect_instances_on_open,
                    m.get("settings.connect_on_open"),
                )
                .changed()
            {
                app.state.pending.push(PendingAction::SaveConfig);
            }
            if ui
                .checkbox(
                    &mut app.state.config.auto_discover_private_keys,
                    m.get("settings.auto_discover_keys"),
                )
                .changed()
            {
                app.state.pending.push(PendingAction::SaveConfig);
            }

            let mut saved = Vec::new();
            collect_saved_connections(&app.state.config.folders, &mut saved);
            saved.sort_by(|a, b| {
                a.display_name
                    .to_lowercase()
                    .cmp(&b.display_name.to_lowercase())
                    .then_with(|| a.host.cmp(&b.host))
            });

            ui.horizontal(|ui| {
                if ui.button(m.get("settings.select_all")).clicked() {
                    app.state.config.startup_connection_ids =
                        saved.iter().map(|c| c.id.clone()).collect();
                    app.state.config.connect_instances_on_open = true;
                    app.state.pending.push(PendingAction::SaveConfig);
                }
                if ui.button(m.get("common.clear")).clicked() {
                    app.state.config.startup_connection_ids.clear();
                    app.state.pending.push(PendingAction::SaveConfig);
                }
            });

            egui::ScrollArea::vertical()
                .max_height(220.0)
                .show(ui, |ui| {
                    if saved.is_empty() {
                        ui.weak(m.get("settings.no_saved_instances"));
                    } else {
                        for conn in &saved {
                            ui.add_space(3.0);
                            let mut selected = app
                                .state
                                .config
                                .startup_connection_ids
                                .iter()
                                .any(|id| id == &conn.id);
                            let label = format!(
                                "{} ({}@{}:{})",
                                conn.display_name, conn.username, conn.host, conn.port
                            );
                            if ui.checkbox(&mut selected, label).changed() {
                                if selected {
                                    app.state.config.connect_instances_on_open = true;
                                    if !app
                                        .state
                                        .config
                                        .startup_connection_ids
                                        .iter()
                                        .any(|id| id == &conn.id)
                                    {
                                        app.state
                                            .config
                                            .startup_connection_ids
                                            .push(conn.id.clone());
                                    }
                                } else {
                                    app.state
                                        .config
                                        .startup_connection_ids
                                        .retain(|id| id != &conn.id);
                                }
                                app.state.pending.push(PendingAction::SaveConfig);
                            }
                            ui.add_space(3.0);
                        }
                    }
                });

            ui.separator();
            ui.heading(m.get("settings.theme"));
            let mut theme_changed = false;
            theme_changed |= ui
                .radio_value(
                    &mut app.state.config.theme,
                    ThemePreference::System,
                    m.get("settings.theme_system"),
                )
                .changed();
            theme_changed |= ui
                .radio_value(
                    &mut app.state.config.theme,
                    ThemePreference::Dark,
                    m.get("settings.theme_dark"),
                )
                .changed();
            theme_changed |= ui
                .radio_value(
                    &mut app.state.config.theme,
                    ThemePreference::Light,
                    m.get("settings.theme_light"),
                )
                .changed();

            if theme_changed {
                app.state.pending.push(PendingAction::SaveConfig);
            }
        });

    if !open {
        app.dialogs.settings.open = false;
    }
}

fn collect_saved_connections(folders: &[Folder], out: &mut Vec<SavedConnection>) {
    for folder in folders {
        out.extend(folder.connections.iter().cloned());
        collect_saved_connections(&folder.subfolders, out);
    }
}
