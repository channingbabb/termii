use crate::app::App;
use crate::domain::connection::{AuthMethod, SavedConnection};
use crate::domain::folder_tree;
use crate::domain::session::{Session, SessionLaunchConfig};
use crate::services::config;
use crate::services::crypto;
use crate::services::scp::{self, ScpTransferRequest};
use crate::services::ssh::{self, AuthConfig, PortForwardRule, SshCommand, SshEvent};
use crate::services::ssh_keys::default_key_path;
use crate::services::tailscale::TailscaleDevice;
use crate::state::{ConnectPromptState, ConnectionEditorState};
use crate::terminal::Terminal;
use std::path::PathBuf;
use tokio::sync::mpsc;
use zeroize::Zeroize;
use zeroize::Zeroizing;

pub enum PendingAction {
    ConnectSaved(SavedConnection),
    ConnectRaw {
        name: String,
        host: String,
        port: u16,
        username: String,
        auth: AuthConfig,
        forwards: Vec<PortForwardRule>,
    },
    CloseTab(usize),
    RenameSession {
        idx: usize,
        new_name: String,
    },
    RestartSession(usize),
    CloneSession(usize),
    OpenSessionSettings(usize),
    SaveSessionSettings {
        idx: usize,
        rules: Vec<PortForwardRule>,
    },
    SaveConfig,
    ExportServers(std::path::PathBuf),
    ImportServers(std::path::PathBuf),
    OpenEditorNew(String),
    OpenEditorEdit(SavedConnection, String),
    MoveConnectionToFolder {
        conn_id: String,
        target_folder_id: String,
    },
    DeleteConnection(String),
    DeleteFolder(String),
    NewFolder(String),
    AddTailscaleAsSaved(TailscaleDevice),
    OpenConnectPrompt(TailscaleDevice),
    OpenConnectPromptForSaved(SavedConnection),
    AddTailscaleGroup {
        name: String,
        pattern: String,
    },
    DeleteTailscaleGroup(String),
    EditTailscaleGroup {
        id: String,
        name: String,
        pattern: String,
    },
    HideTailscaleDevice(String),
    UnhideAllTailscaleDevices,
    HideTailscaleSection(String),
    UnhideAllTailscaleSections,
    RunScp(ScpTransferRequest),
}

impl PendingAction {
    fn clear_sensitive(&mut self) {
        match self {
            PendingAction::ConnectRaw { auth, .. } => match auth {
                AuthConfig::Password(password) => {
                    password.zeroize();
                }
                AuthConfig::Key { passphrase, .. } => {
                    if let Some(passphrase) = passphrase {
                        passphrase.zeroize();
                    }
                }
                AuthConfig::Agent { .. } => {}
            },
            PendingAction::RunScp(request) => match &mut request.auth {
                scp::ScpAuth::Password(password) => {
                    password.zeroize();
                }
                scp::ScpAuth::Key { passphrase, .. } => {
                    if let Some(passphrase) = passphrase {
                        passphrase.zeroize();
                    }
                }
                scp::ScpAuth::Agent => {}
            },
            _ => {}
        }
    }
}

impl App {
    pub(crate) fn drain_ssh_events(&mut self) {
        for session in &mut self.state.sessions {
            loop {
                match session.evt_rx.try_recv() {
                    Ok(SshEvent::Connected) => {
                        session.connected = true;
                        session.status = self.state.messages.get("status.connected");
                    }
                    Ok(SshEvent::Data(data)) => {
                        session.terminal.process(&data);
                    }
                    Ok(SshEvent::Error(msg)) => {
                        let error_display = format!(
                            "\r\n\x1b[1;31m--- SSH Error ---\r\n{}\r\n-----------------\x1b[0m\r\n",
                            msg
                        );
                        session.terminal.process(error_display.as_bytes());
                        session.status = self
                            .state
                            .messages
                            .format("status.error_with_message", &[("message", msg.as_str())]);
                        log::error!("SSH error in {}: {}", session.name, msg);
                    }
                    Ok(SshEvent::Disconnected) => {
                        session.connected = false;
                        if !session
                            .status
                            .starts_with(self.state.messages.get("status.error_prefix").as_str())
                        {
                            session.status = self.state.messages.get("status.disconnected");
                        }
                    }
                    Err(mpsc::error::TryRecvError::Empty) => break,
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        session.connected = false;
                        if !session
                            .status
                            .starts_with(self.state.messages.get("status.error_prefix").as_str())
                        {
                            session.status = self.state.messages.get("status.session_ended");
                        }
                        break;
                    }
                }
            }
        }
    }

    fn open_connection(
        &mut self,
        name: String,
        host: String,
        port: u16,
        username: String,
        auth_method: AuthMethod,
        auth: AuthConfig,
        forwards: Vec<PortForwardRule>,
    ) {
        let cols = 80u32;
        let rows = 24u32;

        let (cmd_tx, evt_rx) = ssh::start_session(
            &self.state.runtime_handle,
            host.clone(),
            port,
            username.clone(),
            auth,
            cols,
            rows,
        );

        let launch = SessionLaunchConfig {
            name: name.clone(),
            host,
            port,
            username,
            auth_method,
            forwards: forwards.clone(),
        };
        self.state.sessions.push(Session {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            launch,
            terminal: Terminal::new(cols as usize, rows as usize),
            cmd_tx: cmd_tx.clone(),
            evt_rx,
            connected: false,
            status: self.state.messages.get("status.connecting"),
            selection_start: None,
            selection_end: None,
            selection_drag_active: false,
        });
        self.state.active_tab = self.state.sessions.len() - 1;

        for rule in forwards {
            if rule.enabled {
                let _ = cmd_tx.try_send(SshCommand::AddPortForward(rule));
            }
        }
    }

    pub(crate) fn process_pending_actions(&mut self) {
        let mut actions: Vec<PendingAction> = self.state.pending.drain(..).collect();
        for action in &mut actions {
            self.handle_pending_action(action);
            action.clear_sensitive();
        }
    }

    fn handle_pending_action(&mut self, action: &mut PendingAction) {
        match action {
            PendingAction::ConnectSaved(conn) => self.handle_connect_saved(conn.clone()),
            PendingAction::ConnectRaw {
                name,
                host,
                port,
                username,
                auth,
                forwards,
            } => {
                let auth_method = match &auth {
                    AuthConfig::Key { path, .. } => AuthMethod::Key {
                        path: path.clone(),
                        passphrase_required: false,
                    },
                    AuthConfig::Password(_) => AuthMethod::Password,
                    AuthConfig::Agent { .. } => AuthMethod::Agent,
                };
                self.open_connection(
                    name.clone(),
                    host.clone(),
                    *port,
                    username.clone(),
                    auth_method,
                    auth.clone(),
                    forwards.clone(),
                )
            }
            PendingAction::CloseTab(idx) => self.handle_close_tab(*idx),
            PendingAction::RenameSession { idx, new_name } => {
                self.handle_rename_session(*idx, new_name.clone())
            }
            PendingAction::RestartSession(idx) => self.handle_restart_session(*idx),
            PendingAction::CloneSession(idx) => self.handle_clone_session(*idx),
            PendingAction::OpenSessionSettings(idx) => self.handle_open_session_settings(*idx),
            PendingAction::SaveSessionSettings { idx, rules } => {
                self.handle_save_session_settings(*idx, rules.clone())
            }
            PendingAction::SaveConfig => self.save_config_logged(),
            PendingAction::ExportServers(path) => self.handle_export_servers(path.clone()),
            PendingAction::ImportServers(path) => self.handle_import_servers(path.clone()),
            PendingAction::OpenEditorNew(folder_id) => {
                self.handle_open_editor_new(folder_id.clone())
            }
            PendingAction::OpenEditorEdit(conn, folder_id) => {
                self.handle_open_editor_edit(conn.clone(), folder_id.clone())
            }
            PendingAction::MoveConnectionToFolder {
                conn_id,
                target_folder_id,
            } => self.handle_move_connection(conn_id.clone(), target_folder_id.clone()),
            PendingAction::DeleteConnection(id) => self.handle_delete_connection(id.clone()),
            PendingAction::DeleteFolder(id) => self.handle_delete_folder(id.clone()),
            PendingAction::NewFolder(parent_id) => self.handle_new_folder(parent_id.clone()),
            PendingAction::AddTailscaleAsSaved(device) => {
                self.handle_add_tailscale_as_saved(device.clone())
            }
            PendingAction::OpenConnectPrompt(device) => {
                self.handle_open_connect_prompt(device.clone())
            }
            PendingAction::OpenConnectPromptForSaved(conn) => {
                self.handle_open_connect_prompt_for_saved(conn.clone())
            }
            PendingAction::AddTailscaleGroup { name, pattern } => {
                self.handle_add_tailscale_group(name.clone(), pattern.clone())
            }
            PendingAction::DeleteTailscaleGroup(id) => {
                self.handle_delete_tailscale_group(id.clone())
            }
            PendingAction::EditTailscaleGroup { id, name, pattern } => {
                self.handle_edit_tailscale_group(id.clone(), name.clone(), pattern.clone())
            }
            PendingAction::HideTailscaleDevice(hostname) => {
                self.handle_hide_tailscale_device(hostname.clone())
            }
            PendingAction::UnhideAllTailscaleDevices => self.handle_unhide_all_tailscale_devices(),
            PendingAction::HideTailscaleSection(section_id) => {
                self.handle_hide_tailscale_section(section_id.clone())
            }
            PendingAction::UnhideAllTailscaleSections => {
                self.handle_unhide_all_tailscale_sections()
            }
            PendingAction::RunScp(request) => self.handle_run_scp(request.clone()),
        }
    }

    fn handle_connect_saved(&mut self, conn: SavedConnection) {
        let mut decrypted_saved_password: Option<Zeroizing<String>> =
            if let (Some(key), Some(enc)) = (
                self.state.master_key.as_ref(),
                conn.encrypted_password.as_ref(),
            ) {
                crypto::decrypt_secret(key, enc).ok().map(Zeroizing::new)
            } else {
                None
            };

        let can_direct = !conn.username.is_empty()
            && match &conn.auth {
                AuthMethod::Key { path, .. } => {
                    let p = path.to_string_lossy();
                    !p.is_empty() || !default_key_path().is_empty()
                }
                AuthMethod::Password => decrypted_saved_password.is_some(),
                AuthMethod::Agent => true,
            };

        if can_direct {
            let auth = match &conn.auth {
                AuthMethod::Key { path, .. } => {
                    let p = path.to_string_lossy().into_owned();
                    let resolved = if p.is_empty() { default_key_path() } else { p };
                    AuthConfig::Key {
                        path: PathBuf::from(resolved),
                        passphrase: None,
                    }
                }
                AuthMethod::Agent => AuthConfig::Agent {
                    auto_discover_keys: self.state.config.auto_discover_private_keys,
                },
                AuthMethod::Password => AuthConfig::Password(
                    decrypted_saved_password
                        .take()
                        .unwrap_or_else(|| Zeroizing::new(String::new())),
                ),
            };
            self.open_connection(
                conn.display_name.clone(),
                conn.host.clone(),
                conn.port,
                conn.username.clone(),
                conn.auth.clone(),
                auth,
                Vec::new(),
            );
        } else {
            let (use_key, key_path) = match &conn.auth {
                AuthMethod::Key { path, .. } => {
                    let p = path.to_string_lossy().into_owned();
                    if p.is_empty() {
                        (true, default_key_path())
                    } else {
                        (true, p)
                    }
                }
                _ => (false, default_key_path()),
            };
            self.dialogs.connect_prompt = ConnectPromptState {
                open: true,
                host: conn.host.clone(),
                dns_name: String::new(),
                device_name: conn.display_name.clone(),
                username: conn.username.clone(),
                port: conn.port.to_string(),
                use_key,
                key_path,
                passphrase: Zeroizing::new(String::new()),
                password: decrypted_saved_password.unwrap_or_else(|| Zeroizing::new(String::new())),
                use_dns: false,
            };
        }
    }

    fn handle_close_tab(&mut self, idx: usize) {
        if idx < self.state.sessions.len() {
            let session = &self.state.sessions[idx];
            let _ = session.cmd_tx.try_send(SshCommand::Disconnect);
            self.state.sessions.remove(idx);
            if self.state.active_tab >= self.state.sessions.len() && !self.state.sessions.is_empty()
            {
                self.state.active_tab = self.state.sessions.len() - 1;
            }
        }
    }

    fn handle_rename_session(&mut self, idx: usize, new_name: String) {
        if let Some(session) = self.state.sessions.get_mut(idx) {
            session.name = new_name.clone();
            session.launch.name = new_name;
        }
    }

    fn handle_restart_session(&mut self, idx: usize) {
        if idx < self.state.sessions.len() {
            let launch = &self.state.sessions[idx].launch;
            let auth = self.auth_config_from_auth_method(&launch.auth_method);
            let name = launch.name.clone();
            let host = launch.host.clone();
            let port = launch.port;
            let username = launch.username.clone();
            let auth_method = launch.auth_method.clone();
            let forwards = launch.forwards.clone();
            let _ = self.state.sessions[idx]
                .cmd_tx
                .try_send(SshCommand::Disconnect);
            self.state.sessions.remove(idx);
            if let Some(auth) = auth {
                self.open_connection(name, host, port, username, auth_method, auth, forwards);
            } else {
                self.open_connect_prompt_for_password_session(name, host, port, username);
            }
        }
    }

    fn handle_clone_session(&mut self, idx: usize) {
        if idx < self.state.sessions.len() {
            let launch = &self.state.sessions[idx].launch;
            let auth = self.auth_config_from_auth_method(&launch.auth_method);
            let name = self
                .state
                .messages
                .format("actions.clone_session_name", &[("name", &launch.name)]);
            let host = launch.host.clone();
            let port = launch.port;
            let username = launch.username.clone();
            let auth_method = launch.auth_method.clone();
            let forwards = launch.forwards.clone();
            if let Some(auth) = auth {
                self.open_connection(name, host, port, username, auth_method, auth, forwards);
            } else {
                self.open_connect_prompt_for_password_session(name, host, port, username);
            }
        }
    }

    fn auth_config_from_auth_method(&self, auth_method: &AuthMethod) -> Option<AuthConfig> {
        match auth_method {
            AuthMethod::Key { path, .. } => {
                let p = path.to_string_lossy().into_owned();
                let resolved = if p.is_empty() { default_key_path() } else { p };
                Some(AuthConfig::Key {
                    path: PathBuf::from(resolved),
                    passphrase: None,
                })
            }
            AuthMethod::Agent => Some(AuthConfig::Agent {
                auto_discover_keys: self.state.config.auto_discover_private_keys,
            }),
            AuthMethod::Password => None,
        }
    }

    fn open_connect_prompt_for_password_session(
        &mut self,
        name: String,
        host: String,
        port: u16,
        username: String,
    ) {
        self.dialogs.connect_prompt.clear_secrets();
        self.dialogs.connect_prompt = ConnectPromptState {
            open: true,
            host,
            dns_name: String::new(),
            device_name: name,
            username,
            port: port.to_string(),
            use_key: false,
            key_path: default_key_path(),
            passphrase: Zeroizing::new(String::new()),
            password: Zeroizing::new(String::new()),
            use_dns: false,
        };
    }

    fn handle_open_session_settings(&mut self, idx: usize) {
        if let Some(session) = self.state.sessions.get(idx) {
            self.dialogs.session_settings.open = true;
            self.dialogs.session_settings.tab_idx = Some(idx);
            self.dialogs.session_settings.rules = session.launch.forwards.clone();
        }
    }

    fn handle_save_session_settings(&mut self, idx: usize, rules: Vec<PortForwardRule>) {
        if let Some(session) = self.state.sessions.get_mut(idx) {
            session.launch.forwards = rules.clone();
            let _ = session.cmd_tx.try_send(SshCommand::ClearPortForwards);
            for rule in rules {
                if rule.enabled {
                    let _ = session.cmd_tx.try_send(SshCommand::AddPortForward(rule));
                }
            }
        }
    }

    fn save_config_logged(&self) {
        if let Err(e) = config::save_config(&self.state.config) {
            log::error!("Failed to save config: {}", e);
        }
    }

    fn handle_export_servers(&self, path: PathBuf) {
        match config::export_servers(&self.state.config, &path) {
            Ok(()) => {
                log::info!("Exported servers to {:?}", path);
                let _ = rfd::MessageDialog::new()
                    .set_title(self.state.messages.get("actions.export_complete_title"))
                    .set_description(self.state.messages.format(
                        "actions.export_complete_description",
                        &[("path", &path.display().to_string())],
                    ))
                    .set_level(rfd::MessageLevel::Info)
                    .show();
            }
            Err(e) => {
                log::error!("Failed to export servers: {}", e);
                let _ = rfd::MessageDialog::new()
                    .set_title(self.state.messages.get("actions.export_failed_title"))
                    .set_description(self.state.messages.format(
                        "actions.export_failed_description",
                        &[("error", &e.to_string())],
                    ))
                    .set_level(rfd::MessageLevel::Error)
                    .show();
            }
        }
    }

    fn handle_import_servers(&mut self, path: PathBuf) {
        match config::import_servers(&path) {
            Ok(imported) => {
                let imported_has_encrypted = config::has_encrypted_passwords(&imported.folders);
                let imported_master = imported.master_password.clone();

                self.state.config.folders = imported.folders;

                if imported_has_encrypted {
                    self.state.config.master_password = imported_master;
                    self.clear_master_key();
                    let unlock_message = self.state.messages.get("actions.import_unlock_required");
                    self.open_master_password_unlock_dialog(unlock_message.as_str());
                }

                if let Err(e) = config::save_config(&self.state.config) {
                    log::error!("Imported servers but failed to save config: {}", e);
                    let _ = rfd::MessageDialog::new()
                        .set_title(self.state.messages.get("actions.import_partial_title"))
                        .set_description(self.state.messages.format(
                            "actions.import_partial_description",
                            &[("error", &e.to_string())],
                        ))
                        .set_level(rfd::MessageLevel::Warning)
                        .show();
                } else {
                    log::info!("Imported servers from {:?}", path);
                    let _ = rfd::MessageDialog::new()
                        .set_title(self.state.messages.get("actions.import_complete_title"))
                        .set_description(self.state.messages.format(
                            "actions.import_complete_description",
                            &[("path", &path.display().to_string())],
                        ))
                        .set_level(rfd::MessageLevel::Info)
                        .show();
                }
            }
            Err(e) => {
                log::error!("Failed to import servers: {}", e);
                let _ = rfd::MessageDialog::new()
                    .set_title(self.state.messages.get("actions.import_failed_title"))
                    .set_description(self.state.messages.format(
                        "actions.import_failed_description",
                        &[("error", &e.to_string())],
                    ))
                    .set_level(rfd::MessageLevel::Error)
                    .show();
            }
        }
    }

    fn handle_open_editor_new(&mut self, folder_id: String) {
        self.dialogs.editor.clear_secrets();
        self.dialogs.editor = ConnectionEditorState {
            open: true,
            target_folder_id: folder_id,
            ..Default::default()
        };
    }

    fn handle_open_editor_edit(&mut self, conn: SavedConnection, folder_id: String) {
        self.dialogs.editor.clear_secrets();
        let (use_key, key_path) = match &conn.auth {
            AuthMethod::Key { path, .. } => (true, path.to_string_lossy().into_owned()),
            _ => (false, String::new()),
        };
        self.dialogs.editor = ConnectionEditorState {
            open: true,
            display_name: conn.display_name.clone(),
            host: conn.host.clone(),
            port: conn.port.to_string(),
            username: conn.username.clone(),
            use_key,
            key_path,
            passphrase: Zeroizing::new(String::new()),
            password: Zeroizing::new(String::new()),
            existing_encrypted_password: conn.encrypted_password.clone(),
            text_tint_rgba: conn.text_tint_rgba,
            editing_id: Some(conn.id.clone()),
            target_folder_id: folder_id,
        };
    }

    fn handle_move_connection(&mut self, conn_id: String, target_folder_id: String) {
        if let Some(conn) = folder_tree::remove_connection(&mut self.state.config.folders, &conn_id)
        {
            if let Some(folder) =
                folder_tree::find_folder_mut(&mut self.state.config.folders, &target_folder_id)
            {
                folder.connections.push(conn);
                let _ = config::save_config(&self.state.config);
            }
        }
    }

    fn handle_delete_connection(&mut self, id: String) {
        folder_tree::remove_connection(&mut self.state.config.folders, &id);
        let _ = config::save_config(&self.state.config);
    }

    fn handle_delete_folder(&mut self, id: String) {
        folder_tree::remove_folder(&mut self.state.config.folders, &id);
        let _ = config::save_config(&self.state.config);
    }

    fn handle_new_folder(&mut self, parent_id: String) {
        let new_folder_id = self.state.config.generate_id();
        let new_folder = config::Folder {
            id: new_folder_id.clone(),
            name: self.state.messages.get("actions.new_folder_name"),
            connections: Vec::new(),
            subfolders: Vec::new(),
            expanded: true,
            text_tint_rgba: None,
        };
        if let Some(parent) =
            folder_tree::find_folder_mut(&mut self.state.config.folders, &parent_id)
        {
            parent.subfolders.push(new_folder);
        } else {
            self.state.config.folders.push(new_folder);
        }
        self.dialogs.rename_folder = Some((
            new_folder_id,
            self.state
                .messages
                .get("actions.new_folder_name")
                .to_string(),
        ));
        let _ = config::save_config(&self.state.config);
    }

    fn handle_add_tailscale_as_saved(&mut self, device: TailscaleDevice) {
        let conn = SavedConnection {
            id: self.state.config.generate_id(),
            display_name: device.hostname.clone(),
            host: device
                .addresses
                .first()
                .cloned()
                .unwrap_or(device.dns_name.clone()),
            port: 22,
            username: String::new(),
            auth: AuthMethod::Agent,
            encrypted_password: None,
            text_tint_rgba: None,
        };
        if let Some(folder) = self.state.config.folders.first_mut() {
            folder.connections.push(conn);
        }
        let _ = config::save_config(&self.state.config);
    }

    fn handle_open_connect_prompt(&mut self, device: TailscaleDevice) {
        self.dialogs.connect_prompt.clear_secrets();
        self.dialogs.connect_prompt = ConnectPromptState {
            open: true,
            host: device.addresses.first().cloned().unwrap_or_default(),
            dns_name: device.dns_name.clone(),
            device_name: device.hostname.clone(),
            username: String::new(),
            port: "22".into(),
            use_key: true,
            key_path: default_key_path(),
            passphrase: Zeroizing::new(String::new()),
            password: Zeroizing::new(String::new()),
            use_dns: false,
        };
    }

    fn handle_open_connect_prompt_for_saved(&mut self, conn: SavedConnection) {
        self.dialogs.connect_prompt.clear_secrets();
        let decrypted_saved_password: Option<Zeroizing<String>> = if let (Some(key), Some(enc)) = (
            self.state.master_key.as_ref(),
            conn.encrypted_password.as_ref(),
        ) {
            crypto::decrypt_secret(key, enc).ok().map(Zeroizing::new)
        } else {
            None
        };
        let (use_key, key_path) = match &conn.auth {
            AuthMethod::Key { path, .. } => {
                let p = path.to_string_lossy().into_owned();
                if p.is_empty() {
                    (true, default_key_path())
                } else {
                    (true, p)
                }
            }
            _ => (false, default_key_path()),
        };
        self.dialogs.connect_prompt = ConnectPromptState {
            open: true,
            host: conn.host.clone(),
            dns_name: String::new(),
            device_name: conn.display_name.clone(),
            username: conn.username.clone(),
            port: conn.port.to_string(),
            use_key,
            key_path,
            passphrase: Zeroizing::new(String::new()),
            password: decrypted_saved_password.unwrap_or_else(|| Zeroizing::new(String::new())),
            use_dns: false,
        };
    }

    fn handle_add_tailscale_group(&mut self, name: String, pattern: String) {
        let id = self.state.config.generate_id();
        self.state
            .config
            .tailscale_groups
            .push(config::TailscaleGroupRule { id, name, pattern });
        let _ = config::save_config(&self.state.config);
    }

    fn handle_delete_tailscale_group(&mut self, id: String) {
        self.state.config.tailscale_groups.retain(|g| g.id != id);
        let _ = config::save_config(&self.state.config);
    }

    fn handle_edit_tailscale_group(&mut self, id: String, name: String, pattern: String) {
        if let Some(g) = self
            .state
            .config
            .tailscale_groups
            .iter_mut()
            .find(|g| g.id == id)
        {
            g.name = name;
            g.pattern = pattern;
        }
        let _ = config::save_config(&self.state.config);
    }

    fn handle_hide_tailscale_device(&mut self, hostname: String) {
        if !self
            .state
            .config
            .hidden_tailscale_devices
            .iter()
            .any(|h| h == &hostname)
        {
            self.state.config.hidden_tailscale_devices.push(hostname);
            let _ = config::save_config(&self.state.config);
        }
    }

    fn handle_unhide_all_tailscale_devices(&mut self) {
        self.state.config.hidden_tailscale_devices.clear();
        let _ = config::save_config(&self.state.config);
    }

    fn handle_hide_tailscale_section(&mut self, section_id: String) {
        if !self
            .state
            .config
            .hidden_tailscale_sections
            .iter()
            .any(|s| s == &section_id)
        {
            self.state.config.hidden_tailscale_sections.push(section_id);
            let _ = config::save_config(&self.state.config);
        }
    }

    fn handle_unhide_all_tailscale_sections(&mut self) {
        self.state.config.hidden_tailscale_sections.clear();
        let _ = config::save_config(&self.state.config);
    }

    fn handle_run_scp(&mut self, request: ScpTransferRequest) {
        let messages = self.state.messages.clone();
        std::thread::spawn(move || {
            let name = request.name.clone();
            match scp::run_transfer(request) {
                Ok(summary) => {
                    let _ = rfd::MessageDialog::new()
                        .set_title(messages.get("actions.scp_complete_title"))
                        .set_description(messages.format(
                            "actions.scp_complete_description",
                            &[
                                ("name", &name),
                                ("files", &summary.files_transferred.to_string()),
                                ("workers", &summary.workers_used.to_string()),
                            ],
                        ))
                        .set_level(rfd::MessageLevel::Info)
                        .show();
                }
                Err(err) => {
                    log::error!("SCP transfer failed for {}: {:#}", name, err);
                    let _ = rfd::MessageDialog::new()
                        .set_title(messages.get("actions.scp_failed_title"))
                        .set_description(messages.format(
                            "actions.scp_failed_description",
                            &[("name", &name), ("error", &err.to_string())],
                        ))
                        .set_level(rfd::MessageLevel::Error)
                        .show();
                }
            }
        });
    }
}
