use termii::domain::connection::{AuthMethod, SavedConnection};
use termii::domain::folder_tree::{find_folder_mut, remove_connection};
use termii::services::config::{has_encrypted_passwords, import_servers, AppConfig, ServerExport};
use termii::services::crypto::EncryptedSecret;

#[test]
fn test_default_config_serialization() {
    let cfg = AppConfig::default();
    let json = serde_json::to_string(&cfg).expect("serialize default config");
    let parsed: AppConfig = serde_json::from_str(&json).expect("parse serialized config");
    assert_eq!(parsed.version, 1);
    assert_eq!(parsed.folders.len(), 1);
    assert!(parsed.disable_tailscale_auto_discovery);
    assert!(!parsed.connect_instances_on_open);
    assert!(!parsed.auto_discover_private_keys);
}

#[test]
fn test_find_folder_mut() {
    let mut cfg = AppConfig::default();
    let id = cfg.folders[0].id.clone();
    assert!(find_folder_mut(&mut cfg.folders, &id).is_some());
    assert!(find_folder_mut(&mut cfg.folders, "nonexistent").is_none());
}

#[test]
fn test_remove_connection() {
    let mut cfg = AppConfig::default();
    cfg.folders[0].connections.push(SavedConnection {
        id: "c1".into(),
        display_name: "Test".into(),
        host: "1.2.3.4".into(),
        port: 22,
        username: "root".into(),
        auth: AuthMethod::Agent,
        encrypted_password: None,
        text_tint_rgba: None,
    });
    assert!(remove_connection(&mut cfg.folders, "c1").is_some());
    assert!(cfg.folders[0].connections.is_empty());
}

#[test]
fn test_export_omits_master_password_when_not_needed() {
    let cfg = AppConfig::default();
    let export = ServerExport {
        version: 1,
        exported_at_utc: "2026-01-01T00:00:00Z".into(),
        folders: cfg.folders.clone(),
        master_password: if has_encrypted_passwords(&cfg.folders) {
            cfg.master_password.clone()
        } else {
            None
        },
    };
    assert!(export.master_password.is_none());
}

#[test]
fn test_validate_rejects_encrypted_without_master_password() {
    let mut cfg = AppConfig::default();
    cfg.folders[0].connections.push(SavedConnection {
        id: "c1".into(),
        display_name: "Test".into(),
        host: "1.2.3.4".into(),
        port: 22,
        username: "root".into(),
        auth: AuthMethod::Password,
        encrypted_password: Some(EncryptedSecret {
            nonce_b64: "abc".into(),
            ciphertext_b64: "def".into(),
        }),
        text_tint_rgba: None,
    });

    let export = ServerExport {
        version: 1,
        exported_at_utc: "2026-01-01T00:00:00Z".into(),
        folders: cfg.folders.clone(),
        master_password: None,
    };

    let path =
        std::env::temp_dir().join(format!("termii_test_export_{}.json", uuid::Uuid::new_v4()));
    std::fs::write(
        &path,
        serde_json::to_string(&export).expect("serialize server export"),
    )
    .expect("write export file");

    let result = import_servers(&path);
    let _ = std::fs::remove_file(&path);
    assert!(result.is_err());
}

#[test]
fn test_import_superputty_xml() {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<ArrayOfSessionData xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
  <SessionData SessionId="Channing/chan-immich" SessionName="chan-immich" Host="192.168.1.205" Port="22" Proto="SSH" Username="ubuntu" />
  <SessionData SessionId="Velbit/prod" SessionName="prod" Host="100.64.0.42" Port="22" Proto="SSH" Username="ubuntu" />
</ArrayOfSessionData>"#;

    let path = std::env::temp_dir().join(format!("termii_superputty_{}.xml", uuid::Uuid::new_v4()));
    std::fs::write(&path, xml).expect("write xml");

    let imported = import_servers(&path).expect("import xml");
    let _ = std::fs::remove_file(&path);

    assert_eq!(imported.version, 1);
    assert!(imported.master_password.is_none());

    let mut total = 0usize;
    let mut has_channing = false;
    let mut has_velbit = false;
    for folder in &imported.folders {
        if folder.name == "Channing" {
            has_channing = true;
        }
        if folder.name == "Velbit" {
            has_velbit = true;
        }
        total += folder.connections.len();
    }

    assert!(has_channing);
    assert!(has_velbit);
    assert_eq!(total, 2);
}
