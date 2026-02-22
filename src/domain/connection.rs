use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::services::crypto::EncryptedSecret;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedConnection {
    pub id: String,
    pub display_name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthMethod,
    #[serde(default)]
    pub encrypted_password: Option<EncryptedSecret>,
    #[serde(default, alias = "background_tint_rgba")]
    pub text_tint_rgba: Option<[u8; 4]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthMethod {
    Key {
        path: PathBuf,
        passphrase_required: bool,
    },
    Password,
    Agent,
}
