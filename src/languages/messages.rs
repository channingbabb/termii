use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

const DEFAULT_MESSAGES_JSON: &str = include_str!("messages_en.json");
const DEFAULT_MESSAGES_PATH: &str = "src/languages/messages_en.json";

#[derive(Clone, Default)]
pub struct Messages {
    values: Arc<HashMap<String, String>>,
}

#[derive(Deserialize)]
struct MessagesFile(HashMap<String, String>);

impl Messages {
    pub fn load() -> Self {
        if let Some(messages) = Self::load_from_path_candidates() {
            return messages;
        }
        Self::from_json(DEFAULT_MESSAGES_JSON).unwrap_or_default()
    }

    pub fn get(&self, key: &str) -> String {
        self.values
            .get(key)
            .cloned()
            .unwrap_or_else(|| key.to_string())
    }

    pub fn format(&self, key: &str, args: &[(&str, &str)]) -> String {
        let mut out = self.get(key);
        for (name, value) in args {
            out = out.replace(&format!("{{{}}}", name), value);
        }
        out
    }

    fn load_from_path_candidates() -> Option<Self> {
        let mut paths = Vec::new();
        if let Ok(custom) = std::env::var("TERMII_MESSAGES_PATH") {
            paths.push(PathBuf::from(custom));
        }
        paths.push(PathBuf::from(DEFAULT_MESSAGES_PATH));
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                paths.push(dir.join(DEFAULT_MESSAGES_PATH));
            }
        }
        for path in paths {
            if path.exists() {
                match std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|raw| Self::from_json(&raw))
                {
                    Some(messages) => {
                        log::info!("Loaded UI messages from {}", path.display());
                        return Some(messages);
                    }
                    None => {
                        log::error!("Failed to parse UI messages from {}", path.display());
                    }
                }
            }
        }
        None
    }

    fn from_json(raw: &str) -> Option<Self> {
        let parsed: HashMap<String, String> = serde_json::from_str(raw)
            .or_else(|_| serde_json::from_str::<MessagesFile>(raw).map(|v| v.0))
            .ok()?;
        Some(Self {
            values: Arc::new(parsed),
        })
    }
}
