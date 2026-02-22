use serde::Deserialize;
use std::collections::HashMap;
use std::io::ErrorKind;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct TailscaleDevice {
    pub hostname: String,
    pub dns_name: String,
    pub addresses: Vec<String>,
    pub os: String,
    pub online: bool,
    pub last_seen: Option<String>,
    pub is_self: bool,
    pub owner: String,
}

#[derive(Deserialize)]
pub struct TsStatus {
    #[serde(rename = "Self")]
    self_node: Option<TsNode>,
    #[serde(rename = "Peer")]
    peer: Option<HashMap<String, TsNode>>,
    #[serde(rename = "User", default)]
    user: HashMap<String, TsUser>,
}

#[derive(Deserialize)]
struct TsUser {
    #[serde(rename = "LoginName", default)]
    login_name: String,
}

#[derive(Deserialize)]
struct TsNode {
    #[serde(rename = "HostName", default)]
    hostname: String,
    #[serde(rename = "DNSName", default)]
    dns_name: String,
    #[serde(rename = "TailscaleIPs", default)]
    tailscale_ips: Vec<String>,
    #[serde(rename = "OS", default)]
    os: String,
    #[serde(rename = "Online", default)]
    online: bool,
    #[serde(rename = "LastSeen", default)]
    last_seen: Option<String>,
    #[serde(rename = "UserID", default)]
    user_id: i64,
}

impl TsNode {
    fn into_device(self, is_self: bool, users: &HashMap<String, TsUser>) -> TailscaleDevice {
        let owner = users
            .get(&self.user_id.to_string())
            .map(|u| u.login_name.clone())
            .unwrap_or_default();
        TailscaleDevice {
            hostname: self.hostname,
            dns_name: self.dns_name.trim_end_matches('.').to_string(),
            addresses: self.tailscale_ips,
            os: self.os,
            online: self.online,
            last_seen: self.last_seen,
            is_self,
            owner,
        }
    }
}

pub fn parse_status(status: TsStatus) -> Vec<TailscaleDevice> {
    let mut devices = Vec::new();
    if let Some(self_node) = status.self_node {
        devices.push(self_node.into_device(true, &status.user));
    }
    if let Some(peers) = status.peer {
        for (_key, node) in peers {
            devices.push(node.into_device(false, &status.user));
        }
    }
    devices.sort_by(|a, b| a.hostname.to_lowercase().cmp(&b.hostname.to_lowercase()));
    devices
}
async fn discover_via_api() -> anyhow::Result<Vec<TailscaleDevice>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?;
    let resp = client
        .get("http://100.100.100.100/localapi/v0/status")
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("Local API returned status {}", resp.status());
    }
    let status: TsStatus = resp.json().await?;
    Ok(parse_status(status))
}
async fn discover_via_cli() -> anyhow::Result<Vec<TailscaleDevice>> {
    let mut errors = Vec::new();
    for candidate in tailscale_cli_candidates() {
        match tokio::process::Command::new(candidate)
            .args(["status", "--json"])
            .output()
            .await
        {
            Ok(output) => {
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    errors.push(format!("{}: {}", candidate, stderr.trim()));
                    continue;
                }
                match serde_json::from_slice::<TsStatus>(&output.stdout) {
                    Ok(status) => {
                        log::debug!("Tailscale CLI discovery using {}", candidate);
                        return Ok(parse_status(status));
                    }
                    Err(e) => {
                        errors.push(format!("{}: invalid JSON ({})", candidate, e));
                        continue;
                    }
                }
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {
                errors.push(format!("{}: command not found", candidate));
            }
            Err(e) => {
                errors.push(format!("{}: {}", candidate, e));
            }
        }
    }
    anyhow::bail!(
        "tailscale status failed for all candidates: {}",
        errors.join("; ")
    );
}

fn tailscale_cli_candidates() -> &'static [&'static str] {
    #[cfg(target_os = "macos")]
    //testing on mac, just need to have some options to check for whenever on the mac app.
    {
        &[
            "tailscale",
            "/opt/homebrew/bin/tailscale",
            "/usr/local/bin/tailscale",
            "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
        ]
    }
    #[cfg(not(target_os = "macos"))]
    {
        &["tailscale"]
    }
}
pub async fn discover_devices() -> anyhow::Result<Vec<TailscaleDevice>> {
    match discover_via_api().await {
        Ok(devices) => Ok(devices),
        Err(api_err) => {
            log::debug!("Tailscale local API failed: {}. Trying CLI.", api_err);
            discover_via_cli().await
        }
    }
}

pub fn start_discovery_loop(
    handle: &tokio::runtime::Handle,
    devices: Arc<Mutex<Vec<TailscaleDevice>>>,
    enabled: Arc<AtomicBool>,
    interval_secs: u64,
) {
    let devices = devices.clone();
    let enabled = enabled.clone();
    handle.spawn(async move {
        loop {
            if !enabled.load(Ordering::Relaxed) {
                {
                    let mut lock = devices.lock().unwrap();
                    if !lock.is_empty() {
                        lock.clear();
                    }
                }
                tokio::time::sleep(Duration::from_secs(interval_secs)).await;
                continue;
            }

            match discover_devices().await {
                Ok(new_devices) => {
                    let mut lock = devices.lock().unwrap();
                    *lock = new_devices;
                }
                Err(e) => {
                    log::warn!("Tailscale discovery error: {}", e);
                }
            }
            tokio::time::sleep(Duration::from_secs(interval_secs)).await;
        }
    });
}

pub fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern = pattern.to_lowercase();
    let text = text.to_lowercase();
    let p_chars: Vec<char> = pattern.chars().collect();
    let t_chars: Vec<char> = text.chars().collect();
    let pn = p_chars.len();
    let tn = t_chars.len();
    let mut dp = vec![vec![false; tn + 1]; pn + 1];
    dp[0][0] = true;
    for i in 1..=pn {
        if p_chars[i - 1] == '*' {
            dp[i][0] = dp[i - 1][0];
        }
    }

    for i in 1..=pn {
        for j in 1..=tn {
            if p_chars[i - 1] == '*' {
                dp[i][j] = dp[i - 1][j] || dp[i][j - 1];
            } else if p_chars[i - 1] == '?' || p_chars[i - 1] == t_chars[j - 1] {
                dp[i][j] = dp[i - 1][j - 1];
            }
        }
    }

    dp[pn][tn]
}
