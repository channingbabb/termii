use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use zeroize::Zeroizing;

use crate::domain::session::Session;
use crate::languages::messages::Messages;
use crate::services::config::AppConfig;
use crate::services::tailscale::TailscaleDevice;
use crate::state::actions::PendingAction;

pub struct AppState {
    pub config: AppConfig,
    pub messages: Messages,
    pub sessions: Vec<Session>,
    pub active_tab: usize,
    pub search_query: String,
    pub tailscale_devices: Arc<Mutex<Vec<TailscaleDevice>>>,
    pub tailscale_discovery_enabled: Arc<AtomicBool>,
    pub runtime_handle: tokio::runtime::Handle,
    pub master_key: Option<Zeroizing<[u8; 32]>>,
    pub pending: Vec<PendingAction>,
}
