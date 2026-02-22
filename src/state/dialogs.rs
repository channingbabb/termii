use crate::services::crypto::EncryptedSecret;
use crate::services::ssh::PortForwardRule;
use crate::services::ssh_keys::default_key_path;
use zeroize::Zeroize;
use zeroize::Zeroizing;

pub struct ConnectionEditorState {
    pub open: bool,
    pub display_name: String,
    pub host: String,
    pub port: String,
    pub username: String,
    pub use_key: bool,
    pub key_path: String,
    pub passphrase: Zeroizing<String>,
    pub password: Zeroizing<String>,
    pub existing_encrypted_password: Option<EncryptedSecret>,
    pub text_tint_rgba: Option<[u8; 4]>,
    pub editing_id: Option<String>,
    pub target_folder_id: String,
}

impl Default for ConnectionEditorState {
    fn default() -> Self {
        Self {
            open: false,
            display_name: String::new(),
            host: String::new(),
            port: "22".into(),
            username: String::new(),
            use_key: true,
            key_path: default_key_path(),
            passphrase: Zeroizing::new(String::new()),
            password: Zeroizing::new(String::new()),
            existing_encrypted_password: None,
            text_tint_rgba: None,
            editing_id: None,
            target_folder_id: String::new(),
        }
    }
}

impl ConnectionEditorState {
    pub fn clear_secrets(&mut self) {
        self.passphrase.zeroize();
        self.password.zeroize();
    }
}

pub struct ConnectPromptState {
    pub open: bool,
    pub host: String,
    pub dns_name: String,
    pub device_name: String,
    pub username: String,
    pub port: String,
    pub use_key: bool,
    pub key_path: String,
    pub passphrase: Zeroizing<String>,
    pub password: Zeroizing<String>,
    pub use_dns: bool,
}

impl Default for ConnectPromptState {
    fn default() -> Self {
        Self {
            open: false,
            host: String::new(),
            dns_name: String::new(),
            device_name: String::new(),
            username: String::new(),
            port: "22".into(),
            use_key: true,
            key_path: default_key_path(),
            passphrase: Zeroizing::new(String::new()),
            password: Zeroizing::new(String::new()),
            use_dns: false,
        }
    }
}

impl ConnectPromptState {
    pub fn clear_secrets(&mut self) {
        self.passphrase.zeroize();
        self.password.zeroize();
    }
}

#[derive(Clone, Default)]
pub struct SessionRenameState {
    pub open: bool,
    pub tab_idx: Option<usize>,
    pub new_name: String,
}

#[derive(Clone)]
pub struct SessionSettingsState {
    pub open: bool,
    pub tab_idx: Option<usize>,
    pub rules: Vec<PortForwardRule>,
    pub local_listen_host: String,
    pub local_listen_port: String,
    pub local_target_host: String,
    pub local_target_port: String,
    pub remote_listen_host: String,
    pub remote_listen_port: String,
    pub remote_target_host: String,
    pub remote_target_port: String,
}

impl Default for SessionSettingsState {
    fn default() -> Self {
        Self {
            open: false,
            tab_idx: None,
            rules: Vec::new(),
            local_listen_host: "127.0.0.1".into(),
            local_listen_port: "8080".into(),
            local_target_host: "127.0.0.1".into(),
            local_target_port: "80".into(),
            remote_listen_host: "127.0.0.1".into(),
            remote_listen_port: "8080".into(),
            remote_target_host: "127.0.0.1".into(),
            remote_target_port: "80".into(),
        }
    }
}

#[derive(Clone, Default)]
pub struct TsGroupEditorState {
    pub open: bool,
    pub name: String,
    pub pattern: String,
    pub editing_id: Option<String>,
}

#[derive(Clone, Default)]
pub struct AppSettingsState {
    pub open: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum ScpDirectionState {
    #[default]
    LocalToRemote,
    RemoteToLocal,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum ScpAuthModeState {
    #[default]
    Key,
    Password,
    Agent,
}

pub struct ScpDialogState {
    pub open: bool,
    pub name: String,
    pub host: String,
    pub port: String,
    pub username: String,
    pub auth_mode: ScpAuthModeState,
    pub key_path: String,
    pub passphrase: Zeroizing<String>,
    pub password: Zeroizing<String>,
    pub direction: ScpDirectionState,
    pub local_sources: Vec<String>,
    pub remote_sources: String,
    pub remote_destination: String,
    pub local_destination: String,
}

impl Default for ScpDialogState {
    fn default() -> Self {
        Self {
            open: false,
            name: String::new(),
            host: String::new(),
            port: "22".into(),
            username: String::new(),
            auth_mode: ScpAuthModeState::Key,
            key_path: default_key_path(),
            passphrase: Zeroizing::new(String::new()),
            password: Zeroizing::new(String::new()),
            direction: ScpDirectionState::LocalToRemote,
            local_sources: Vec::new(),
            remote_sources: String::new(),
            remote_destination: "/tmp".into(),
            local_destination: dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .to_string_lossy()
                .into_owned(),
        }
    }
}

impl ScpDialogState {
    pub fn clear_secrets(&mut self) {
        self.passphrase.zeroize();
        self.password.zeroize();
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MasterPasswordMode {
    Create,
    Unlock,
}

pub struct MasterPasswordDialogState {
    pub open: bool,
    pub mode: MasterPasswordMode,
    pub password: Zeroizing<String>,
    pub confirm: Zeroizing<String>,
    pub error: String,
}

impl Default for MasterPasswordDialogState {
    fn default() -> Self {
        Self {
            open: false,
            mode: MasterPasswordMode::Unlock,
            password: Zeroizing::new(String::new()),
            confirm: Zeroizing::new(String::new()),
            error: String::new(),
        }
    }
}

impl MasterPasswordDialogState {
    pub fn clear_secrets(&mut self) {
        self.password.zeroize();
        self.confirm.zeroize();
    }
}

#[derive(Default)]
pub struct DialogState {
    pub editor: ConnectionEditorState,
    pub connect_prompt: ConnectPromptState,
    pub ts_group_editor: TsGroupEditorState,
    pub master_password_dialog: MasterPasswordDialogState,
    pub rename_folder: Option<(String, String)>,
    pub dragging_connection: Option<(String, String)>,
    pub rename_session: SessionRenameState,
    pub session_settings: SessionSettingsState,
    pub settings: AppSettingsState,
    pub scp: ScpDialogState,
}

impl DialogState {
    pub fn clear_all_secrets(&mut self) {
        self.editor.clear_secrets();
        self.connect_prompt.clear_secrets();
        self.scp.clear_secrets();
        self.master_password_dialog.clear_secrets();
    }
}
