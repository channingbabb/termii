use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::domain::connection::{AuthMethod, SavedConnection};
use crate::services::crypto::MasterPasswordConfig;

const CONFIG_VERSION: u32 = 1;
const CONFIG_FILE: &str = "config.json";
const SERVER_EXPORT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub version: u32,
    pub theme: ThemePreference,
    pub folders: Vec<Folder>,
    pub next_id: u64,
    #[serde(default = "default_disable_tailscale_auto_discovery")]
    pub disable_tailscale_auto_discovery: bool,
    #[serde(default)]
    pub connect_instances_on_open: bool,
    #[serde(default)]
    pub auto_discover_private_keys: bool,
    #[serde(default)]
    pub tailscale_groups: Vec<TailscaleGroupRule>,
    #[serde(default)]
    pub hidden_tailscale_devices: Vec<String>,
    #[serde(default)]
    pub hidden_tailscale_sections: Vec<String>,
    #[serde(default)]
    pub startup_connection_ids: Vec<String>,
    #[serde(default)]
    pub master_password: Option<MasterPasswordConfig>,
}

fn default_disable_tailscale_auto_discovery() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TailscaleGroupRule {
    pub id: String,
    pub name: String,
    pub pattern: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ThemePreference {
    System,
    Dark,
    Light,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub connections: Vec<SavedConnection>,
    pub subfolders: Vec<Folder>,
    pub expanded: bool,
    #[serde(default, alias = "background_tint_rgba")]
    pub text_tint_rgba: Option<[u8; 4]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerExport {
    pub version: u32,
    pub exported_at_utc: String,
    pub folders: Vec<Folder>,
    pub master_password: Option<MasterPasswordConfig>,
}

impl Default for AppConfig {
    fn default() -> Self {
        let default_folder_name = crate::languages::messages::Messages::load()
            .get("config.default_folder_name")
            .to_string();
        Self {
            version: CONFIG_VERSION,
            theme: ThemePreference::System,
            folders: vec![Folder {
                id: uuid::Uuid::new_v4().to_string(),
                name: default_folder_name,
                connections: Vec::new(),
                subfolders: Vec::new(),
                expanded: true,
                text_tint_rgba: None,
            }],
            next_id: 1,
            disable_tailscale_auto_discovery: true,
            connect_instances_on_open: false,
            auto_discover_private_keys: false,
            tailscale_groups: Vec::new(),
            hidden_tailscale_devices: Vec::new(),
            hidden_tailscale_sections: Vec::new(),
            startup_connection_ids: Vec::new(),
            master_password: None,
        }
    }
}

impl AppConfig {
    pub fn generate_id(&mut self) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        self.next_id += 1;
        id
    }
}

pub fn config_dir() -> PathBuf {
    if let Some(base) = dirs::config_dir() {
        return base.join("termii");
    }
    if let Some(home) = dirs::home_dir() {
        return home.join(".termii");
    }
    PathBuf::from(".termii")
}

pub fn config_path() -> PathBuf {
    config_dir().join(CONFIG_FILE)
}

pub fn load_config() -> AppConfig {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(contents) => match serde_json::from_str::<AppConfig>(&contents) {
            Ok(mut cfg) => {
                if cfg.version < CONFIG_VERSION {
                    cfg = migrate_config(cfg);
                }
                cfg
            }
            Err(e) => {
                log::error!("Failed to parse config: {}. Using defaults.", e);
                AppConfig::default()
            }
        },
        Err(_) => {
            log::info!("No config found at {:?}. Creating defaults.", path);
            let cfg = AppConfig::default();
            let _ = save_config(&cfg);
            cfg
        }
    }
}

pub fn save_config(config: &AppConfig) -> anyhow::Result<()> {
    validate_password_auth_storage(&config.folders)?;
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;
    set_secure_dir_permissions(&dir)?;
    let json = serde_json::to_string_pretty(config)?;
    let path = config_path();
    write_file_secure(&path, json.as_bytes())?;
    log::info!("Config saved to {:?}", path);
    Ok(())
}

fn write_file_secure(path: &Path, contents: &[u8]) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(contents)?;
        file.flush()?;
        set_secure_file_permissions(path)?;
        Ok(())
    }

    #[cfg(windows)]
    {
        std::fs::write(path, contents)?;
        set_secure_file_permissions(path)?;
        Ok(())
    }

    #[cfg(not(any(unix, windows)))]
    {
        std::fs::write(path, contents)?;
        Ok(())
    }
}

fn set_secure_dir_permissions(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    #[cfg(windows)]
    {
        set_secure_windows_acl(path, true)?;
    }
    #[cfg(not(any(unix, windows)))]
    let _ = path;
    Ok(())
}

#[cfg(unix)]
fn set_secure_file_permissions(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(windows)]
fn set_secure_file_permissions(path: &Path) -> anyhow::Result<()> {
    set_secure_windows_acl(path, false)
}

#[cfg(windows)]
fn set_secure_windows_acl(path: &Path, is_dir: bool) -> anyhow::Result<()> {
    use anyhow::{anyhow, Context};
    use std::process::Command;

    let output = Command::new("whoami")
        .output()
        .context("failed to resolve current Windows user with whoami")?;
    if !output.status.success() {
        return Err(anyhow!(
            "failed to resolve current Windows user with whoami"
        ));
    }
    let principal = String::from_utf8(output.stdout)
        .context("whoami output was not valid UTF-8")?
        .trim()
        .to_string();
    if principal.is_empty() {
        return Err(anyhow!("whoami returned an empty user principal"));
    }

    let grant = if is_dir {
        format!("{principal}:(OI)(CI)F")
    } else {
        format!("{principal}:F")
    };

    let status = Command::new("icacls")
        .arg(path)
        .arg("/inheritance:r")
        .arg("/grant:r")
        .arg(grant)
        .arg("/remove:g")
        .arg("Users")
        .arg("/remove:g")
        .arg("Authenticated Users")
        .arg("/remove:g")
        .arg("Everyone")
        .status()
        .with_context(|| format!("failed to run icacls for {}", path.display()))?;

    if !status.success() {
        return Err(anyhow!(
            "icacls failed while setting secure ACLs on {}",
            path.display()
        ));
    }
    Ok(())
}

pub fn export_servers(config: &AppConfig, path: &Path) -> anyhow::Result<()> {
    let export = ServerExport {
        version: SERVER_EXPORT_VERSION,
        exported_at_utc: chrono::Utc::now().to_rfc3339(),
        folders: config.folders.clone(),
        master_password: if has_encrypted_passwords(&config.folders) {
            config.master_password.clone()
        } else {
            None
        },
    };
    validate_server_export(&export)?;
    let json = serde_json::to_string_pretty(&export)?;
    write_file_secure(path, json.as_bytes())?;
    Ok(())
}

pub fn import_servers(path: &Path) -> anyhow::Result<ServerExport> {
    let contents = std::fs::read_to_string(path)?;
    if looks_like_superputty_xml(&contents) {
        let export = import_superputty_xml(&contents)?;
        validate_server_export(&export)?;
        return Ok(export);
    }
    let export: ServerExport = serde_json::from_str(&contents)?;
    validate_server_export(&export)?;
    Ok(export)
}

fn migrate_config(mut config: AppConfig) -> AppConfig {
    config.version = CONFIG_VERSION;
    config
}

pub fn has_encrypted_passwords(folders: &[Folder]) -> bool {
    folders.iter().any(|folder| {
        folder
            .connections
            .iter()
            .any(|conn| conn.encrypted_password.is_some())
            || has_encrypted_passwords(&folder.subfolders)
    })
}

fn validate_server_export(export: &ServerExport) -> anyhow::Result<()> {
    if export.version != SERVER_EXPORT_VERSION {
        return Err(anyhow::anyhow!(
            "Unsupported export version {} (expected {}).",
            export.version,
            SERVER_EXPORT_VERSION
        ));
    }

    if has_encrypted_passwords(&export.folders) && export.master_password.is_none() {
        return Err(anyhow::anyhow!(
            "Import includes encrypted saved passwords but no master-password metadata."
        ));
    }
    validate_password_auth_storage(&export.folders)?;

    Ok(())
}

fn validate_password_auth_storage(folders: &[Folder]) -> anyhow::Result<()> {
    fn recurse(folders: &[Folder]) -> bool {
        folders.iter().any(|folder| {
            folder.connections.iter().any(|conn| {
                matches!(conn.auth, AuthMethod::Password) && conn.encrypted_password.is_none()
            }) || recurse(&folder.subfolders)
        })
    }

    if recurse(folders) {
        return Err(anyhow::anyhow!(
            "Password-auth connections must store an encrypted password protected by a master password."
        ));
    }
    Ok(())
}

fn looks_like_superputty_xml(contents: &str) -> bool {
    let trimmed = contents.trim_start();
    trimmed.starts_with("<?xml")
        || trimmed.starts_with("<ArrayOfSessionData")
        || trimmed.contains("<ArrayOfSessionData")
}

fn import_superputty_xml(contents: &str) -> anyhow::Result<ServerExport> {
    let doc = roxmltree::Document::parse(contents)?;
    let mut grouped: std::collections::BTreeMap<String, Vec<SavedConnection>> =
        std::collections::BTreeMap::new();

    for node in doc.descendants().filter(|n| n.has_tag_name("SessionData")) {
        let host = node.attribute("Host").unwrap_or("").trim();
        if host.is_empty() {
            continue;
        }

        let session_name = node.attribute("SessionName").unwrap_or(host).trim();
        let session_id = node.attribute("SessionId").unwrap_or(session_name).trim();
        let username = node.attribute("Username").unwrap_or("").trim().to_string();
        let port = node
            .attribute("Port")
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(22);

        let folder_name = session_id
            .split_once('/')
            .map(|(prefix, _)| prefix)
            .filter(|s| !s.is_empty())
            .unwrap_or("Imported");

        grouped
            .entry(folder_name.to_string())
            .or_default()
            .push(SavedConnection {
                id: uuid::Uuid::new_v4().to_string(),
                display_name: session_name.to_string(),
                host: host.to_string(),
                port,
                username,
                auth: AuthMethod::Agent,
                encrypted_password: None,
                text_tint_rgba: None,
            });
    }

    if grouped.is_empty() {
        return Err(anyhow::anyhow!(
            "No SessionData entries were found in SuperPuTTY XML."
        ));
    }

    let folders = grouped
        .into_iter()
        .map(|(name, connections)| Folder {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            connections,
            subfolders: Vec::new(),
            expanded: true,
            text_tint_rgba: None,
        })
        .collect();

    Ok(ServerExport {
        version: SERVER_EXPORT_VERSION,
        exported_at_utc: chrono::Utc::now().to_rfc3339(),
        folders,
        master_password: None,
    })
}
