use std::path::PathBuf;

pub fn ssh_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".ssh"))
}

pub fn default_key_path() -> String {
    let Some(dir) = ssh_dir() else {
        return String::new();
    };

    for name in ["id_ed25519", "id_rsa", "id_ecdsa"] {
        let p = dir.join(name);
        if p.is_file() {
            return p.to_string_lossy().into_owned();
        }
    }

    String::new()
}

// i need to go back and make sure that this is opt-in only
pub fn discover_private_key_paths() -> Vec<PathBuf> {
    let Some(dir) = ssh_dir() else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for preferred in ["id_ed25519", "id_rsa", "id_ecdsa"] {
        let p = dir.join(preferred);
        if p.is_file() {
            out.push(p);
        }
    }

    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if name.ends_with(".pub")
                || name.eq_ignore_ascii_case("known_hosts")
                || name.eq_ignore_ascii_case("known_hosts.old")
                || name.eq_ignore_ascii_case("config")
                || name.eq_ignore_ascii_case("authorized_keys")
            {
                continue;
            }
            if !out.iter().any(|p| p == &path) {
                out.push(path);
            }
        }
    }

    out
}
