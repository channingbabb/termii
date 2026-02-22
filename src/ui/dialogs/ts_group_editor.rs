use eframe::egui;

use crate::app::App;
use crate::state::PendingAction;

pub fn render(ctx: &egui::Context, app: &mut App) {
    if !app.dialogs.ts_group_editor.open {
        return;
    }

    let m = app.state.messages.clone();
    let mut open = true;
    let is_edit = app.dialogs.ts_group_editor.editing_id.is_some();
    let title = if is_edit {
        m.get("ts_group_editor.edit_title")
    } else {
        m.get("ts_group_editor.new_title")
    };

    egui::Window::new(title)
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .min_width(350.0)
        .show(ctx, |ui| {
            egui::Grid::new("ts_group_grid")
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.label(m.get("ts_group_editor.group_name"));
                    ui.text_edit_singleline(&mut app.dialogs.ts_group_editor.name);
                    ui.end_row();

                    ui.label(m.get("ts_group_editor.pattern"));
                    ui.text_edit_singleline(&mut app.dialogs.ts_group_editor.pattern);
                    ui.end_row();
                });

            ui.separator();
            ui.label(m.get("ts_group_editor.pattern_help_1"));
            ui.label(m.get("ts_group_editor.pattern_help_2"));
            ui.label(m.get("ts_group_editor.pattern_help_3"));

            let devices = app.state.tailscale_devices.lock().unwrap().clone();
            let pat = &app.dialogs.ts_group_editor.pattern;
            if !pat.is_empty() {
                let matched: Vec<&str> = devices
                    .iter()
                    .filter(|d| !d.is_self)
                    .filter(|d| crate::services::tailscale::glob_match(pat, &d.hostname))
                    .map(|d| d.hostname.as_str())
                    .collect();
                ui.separator();
                if matched.is_empty() {
                    ui.colored_label(egui::Color32::GRAY, m.get("ts_group_editor.no_matches"));
                } else {
                    let matched_count = matched.len().to_string();
                    ui.label(m.format(
                        "ts_group_editor.matches_count",
                        &[("count", &matched_count)],
                    ));
                    for name in matched.iter().take(10) {
                        ui.label(format!("  {}", name));
                    }
                    if matched.len() > 10 {
                        let more = (matched.len() - 10).to_string();
                        ui.label(m.format("ts_group_editor.matches_more", &[("count", &more)]));
                    }
                }
            }

            ui.separator();
            ui.horizontal(|ui| {
                let can_save = !app.dialogs.ts_group_editor.name.is_empty()
                    && !app.dialogs.ts_group_editor.pattern.is_empty();

                if ui
                    .add_enabled(can_save, egui::Button::new(m.get("common.save")))
                    .clicked()
                {
                    if let Some(ref id) = app.dialogs.ts_group_editor.editing_id {
                        app.state.pending.push(PendingAction::EditTailscaleGroup {
                            id: id.clone(),
                            name: app.dialogs.ts_group_editor.name.clone(),
                            pattern: app.dialogs.ts_group_editor.pattern.clone(),
                        });
                    } else {
                        app.state.pending.push(PendingAction::AddTailscaleGroup {
                            name: app.dialogs.ts_group_editor.name.clone(),
                            pattern: app.dialogs.ts_group_editor.pattern.clone(),
                        });
                    }
                    app.dialogs.ts_group_editor.open = false;
                }
                if ui.button(m.get("common.cancel")).clicked() {
                    app.dialogs.ts_group_editor.open = false;
                }
            });
        });

    if !open {
        app.dialogs.ts_group_editor.open = false;
    }
}
