use termii::services::tailscale::{glob_match, parse_status, TsStatus};

#[test]
fn test_parse_status_empty() {
    let status: TsStatus =
        serde_json::from_str(r#"{"Self": null, "Peer": null, "User": {}}"#).expect("parse status");
    let devices = parse_status(status);
    assert!(devices.is_empty());
}

#[test]
fn test_parse_status_with_peers_and_owners() {
    let json = r#"{
        "Self": {
            "HostName": "my-pc",
            "DNSName": "my-pc.tail1234.ts.net.",
            "TailscaleIPs": ["100.64.0.1"],
            "OS": "windows",
            "Online": true,
            "UserID": 1001
        },
        "Peer": {
            "nodekey:abc123": {
                "HostName": "server1",
                "DNSName": "server1.tail1234.ts.net.",
                "TailscaleIPs": ["100.64.0.2", "fd7a::2"],
                "OS": "linux",
                "Online": true,
                "LastSeen": "2025-01-15T10:30:00Z",
                "UserID": 1002
            },
            "nodekey:def456": {
                "HostName": "nas",
                "DNSName": "nas.tail1234.ts.net.",
                "TailscaleIPs": ["100.64.0.3"],
                "OS": "linux",
                "Online": false,
                "LastSeen": "2025-01-14T08:00:00Z",
                "UserID": 1001
            }
        },
        "User": {
            "1001": { "LoginName": "alice@example.com" },
            "1002": { "LoginName": "bob@corp.com" }
        }
    }"#;
    let status: TsStatus = serde_json::from_str(json).expect("parse status");
    let devices = parse_status(status);
    assert_eq!(devices.len(), 3);

    let self_dev = devices.iter().find(|d| d.is_self).expect("self device");
    assert_eq!(self_dev.owner, "alice@example.com");

    let server = devices
        .iter()
        .find(|d| d.hostname == "server1")
        .expect("server1 device");
    assert_eq!(server.owner, "bob@corp.com");

    let nas = devices
        .iter()
        .find(|d| d.hostname == "nas")
        .expect("nas device");
    assert_eq!(nas.owner, "alice@example.com");
}

#[test]
fn test_glob_match() {
    assert!(glob_match("test-*", "test-server1"));
    assert!(glob_match("test-*", "test-"));
    assert!(!glob_match("test-*", "prod-server1"));
    assert!(glob_match("*-prod", "web-prod"));
    assert!(glob_match("*-prod", "db-prod"));
    assert!(!glob_match("*-prod", "db-staging"));
    assert!(glob_match("web-*-eu", "web-app-eu"));
    assert!(!glob_match("web-*-eu", "web-app-us"));
    assert!(glob_match("*", "anything"));
    assert!(glob_match("exact", "exact"));
    assert!(!glob_match("exact", "other"));
    assert!(glob_match("Test-*", "test-SERVER"));
}
