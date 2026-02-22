use eframe::egui;
use std::collections::HashSet;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use zeroize::Zeroizing;

use crate::domain::connection::{AuthMethod, SavedConnection};
use crate::services::config;
use crate::services::tailscale::TailscaleDevice;
use crate::state::{
    AppState, DialogState, MasterPasswordDialogState, MasterPasswordMode, PendingAction,
};
use crate::ui;

pub struct App {
    pub state: AppState,
    pub dialogs: DialogState,
}

impl App {
    pub fn new(_cc: &eframe::CreationContext<'_>, handle: tokio::runtime::Handle) -> Self {
        let messages = crate::languages::messages::Messages::load();
        let config = config::load_config();
        let needs_unlock = config.master_password.is_some();
        let devices: Arc<Mutex<Vec<TailscaleDevice>>> = Arc::new(Mutex::new(Vec::new()));
        let discovery_enabled = Arc::new(AtomicBool::new(!config.disable_tailscale_auto_discovery));

        crate::services::tailscale::start_discovery_loop(
            &handle,
            devices.clone(),
            discovery_enabled.clone(),
            15,
        );

        let dialogs = DialogState {
            master_password_dialog: MasterPasswordDialogState {
                open: needs_unlock,
                mode: MasterPasswordMode::Unlock,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut app = Self {
            state: AppState {
                config,
                messages,
                sessions: Vec::new(),
                active_tab: 0,
                search_query: String::new(),
                tailscale_devices: devices,
                tailscale_discovery_enabled: discovery_enabled,
                runtime_handle: handle,
                master_key: None,
                pending: Vec::new(),
            },
            dialogs,
        };

        if app.state.config.connect_instances_on_open {
            app.enqueue_saved_connections_on_open();
        }

        app
    }

    pub fn open_master_password_create_dialog(&mut self, error: &str) {
        self.dialogs.master_password_dialog.open = true;
        self.dialogs.master_password_dialog.mode = MasterPasswordMode::Create;
        self.dialogs.master_password_dialog.clear_secrets();
        self.dialogs.master_password_dialog.error = error.to_string();
    }

    pub fn open_master_password_unlock_dialog(&mut self, error: &str) {
        self.dialogs.master_password_dialog.open = true;
        self.dialogs.master_password_dialog.mode = MasterPasswordMode::Unlock;
        self.dialogs.master_password_dialog.clear_secrets();
        self.dialogs.master_password_dialog.error = error.to_string();
    }

    pub fn replace_master_key(&mut self, key: [u8; 32]) {
        self.clear_master_key();
        self.state.master_key = Some(Zeroizing::new(key));
    }

    pub fn clear_master_key(&mut self) {
        let _ = self.state.master_key.take();
    }

    fn enqueue_saved_connections_on_open(&mut self) {
        let mut connections = Vec::new();
        collect_saved_connections(&self.state.config.folders, &mut connections);
        let selected_ids: HashSet<&str> = self
            .state
            .config
            .startup_connection_ids
            .iter()
            .map(|s| s.as_str())
            .collect();
        let use_selection = !selected_ids.is_empty();
        for conn in connections {
            if should_autoconnect(&conn)
                && (!use_selection || selected_ids.contains(conn.id.as_str()))
            {
                self.state.pending.push(PendingAction::ConnectSaved(conn));
            }
        }
    }
}

fn collect_saved_connections(folders: &[config::Folder], out: &mut Vec<SavedConnection>) {
    for folder in folders {
        out.extend(folder.connections.iter().cloned());
        collect_saved_connections(&folder.subfolders, out);
    }
}

fn should_autoconnect(conn: &SavedConnection) -> bool {
    if conn.username.trim().is_empty() || conn.host.trim().is_empty() {
        return false;
    }
    match &conn.auth {
        AuthMethod::Agent => true,
        AuthMethod::Key { .. } => true,
        AuthMethod::Password => conn.encrypted_password.is_some(),
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.state.config.master_password.is_some()
            && self.state.master_key.is_none()
            && !self.dialogs.master_password_dialog.open
        {
            self.dialogs.master_password_dialog.open = true;
            self.dialogs.master_password_dialog.mode = MasterPasswordMode::Unlock;
        }

        crate::app::theme::apply_theme(&self.state, ctx);
        self.drain_ssh_events();

        if self.state.sessions.iter().any(|s| s.connected) {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }

        ui::panels::top_bar::render(ctx, self);
        ui::panels::status_bar::render(ctx, self);
        ui::panels::tree_panel::render(ctx, self);
        ui::panels::terminal_panel::render(ctx, self);

        ui::dialogs::connection_editor::render(ctx, self);
        ui::dialogs::connect_prompt::render(ctx, self);
        ui::dialogs::ts_group_editor::render(ctx, self);
        ui::dialogs::master_password::render(ctx, self);
        ui::dialogs::session_rename::render(ctx, self);
        ui::dialogs::session_settings::render(ctx, self);
        ui::dialogs::settings::render(ctx, self);
        ui::dialogs::scp::render(ctx, self);

        self.process_pending_actions();
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.clear_master_key();
        self.dialogs.clear_all_secrets();
    }
}
