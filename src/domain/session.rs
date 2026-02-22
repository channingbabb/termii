use crate::domain::connection::AuthMethod;
use crate::domain::connection::SavedConnection;
use crate::services::ssh::{AuthConfig, PortForwardRule, SshCommand, SshEvent};
use crate::terminal::Terminal;
use tokio::sync::mpsc;

pub struct Session {
    pub id: String,
    pub name: String,
    pub launch: SessionLaunchConfig,
    pub terminal: Terminal,
    pub cmd_tx: mpsc::Sender<SshCommand>,
    pub evt_rx: mpsc::Receiver<SshEvent>,
    pub connected: bool,
    pub status: String,
    pub selection_start: Option<(usize, usize)>,
    pub selection_end: Option<(usize, usize)>,
    pub selection_drag_active: bool,
}

pub struct SessionLaunchConfig {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_method: AuthMethod,
    pub forwards: Vec<PortForwardRule>,
}

pub enum SessionConnectSource {
    Saved(SavedConnection),
    Raw {
        name: String,
        host: String,
        port: u16,
        username: String,
        auth: AuthConfig,
        forwards: Vec<PortForwardRule>,
    },
}
