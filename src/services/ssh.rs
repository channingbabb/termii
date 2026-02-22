use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use zeroize::Zeroizing;

use crate::domain::types::ForwardType;
use crate::services::ssh_keys;

#[derive(Debug, Clone)]
pub struct PortForwardRule {
    pub id: String,
    pub kind: ForwardType,
    pub listen_host: String,
    pub listen_port: u16,
    pub target_host: String,
    pub target_port: u16,
    pub enabled: bool,
}

pub enum SshCommand {
    Input(Vec<u8>),
    Resize { cols: u32, rows: u32 },
    AddPortForward(PortForwardRule),
    RemovePortForward(String),
    ClearPortForwards,
    Disconnect,
}

#[derive(Debug)]
pub enum SshEvent {
    Connected,
    Data(Vec<u8>),
    Error(String),
    Disconnected,
}

#[derive(Debug, Clone)]
pub enum AuthConfig {
    Key {
        path: PathBuf,
        passphrase: Option<Zeroizing<String>>,
    },
    Password(Zeroizing<String>),
    Agent {
        auto_discover_keys: bool,
    },
}

struct SshHandler {
    host: String,
    port: u16,
    remote_forward_targets: Arc<Mutex<HashMap<(String, u16), (String, u16)>>>,
    evt_tx: mpsc::Sender<SshEvent>,
}

async fn bridge_remote_channel_to_local(
    channel: russh::Channel<russh::client::Msg>,
    target_host: String,
    target_port: u16,
    evt_tx: mpsc::Sender<SshEvent>,
) {
    let tcp = match TcpStream::connect((target_host.as_str(), target_port)).await {
        Ok(s) => s,
        Err(e) => {
            let _ = evt_tx
                .send(SshEvent::Error(format!(
                    "Reverse forward connect failed ({}:{}): {}",
                    target_host, target_port, e
                )))
                .await;
            return;
        }
    };

    let mut ssh_stream = channel.into_stream();
    let mut tcp_stream = tcp;
    if let Err(e) = copy_bidirectional(&mut ssh_stream, &mut tcp_stream).await {
        let _ = evt_tx
            .send(SshEvent::Error(format!(
                "Reverse forward stream error: {}",
                e
            )))
            .await;
    }
}

// using russh for ssh connections
// SCP will ideally use russh as well whenever it is implemented.  i dont like having ssh2 in here
// TODO: Look for alternative to ssh2
#[async_trait::async_trait]
impl russh::client::Handler for SshHandler {
    type Error = anyhow::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        let fingerprint = server_public_key.fingerprint(ssh_key::HashAlg::Sha256);
        match russh_keys::check_known_hosts(&self.host, self.port, server_public_key) {
            Ok(true) => Ok(true),
            Ok(false) => {
                let decision = rfd::MessageDialog::new()
                    .set_title("Unknown SSH Host Key")
                    .set_description(format!(
                        "Host: {}:{}\nFingerprint: {}\n\nTrust this host key and add it to known_hosts?",
                        self.host, self.port, fingerprint
                    ))
                    .set_level(rfd::MessageLevel::Warning)
                    .set_buttons(rfd::MessageButtons::YesNo)
                    .show();

                if decision == rfd::MessageDialogResult::Yes {
                    russh_keys::known_hosts::learn_known_hosts(
                        &self.host,
                        self.port,
                        server_public_key,
                    )
                    .map_err(|e| anyhow::anyhow!("Failed to update known_hosts: {}", e))?;
                    Ok(true)
                } else {
                    anyhow::bail!(
                        "Host key verification failed for {}:{} (unknown host key {}).",
                        self.host,
                        self.port,
                        fingerprint
                    )
                }
            }
            Err(russh_keys::Error::KeyChanged { line }) => anyhow::bail!(
                "Host key verification failed for {}:{} ({}): key changed (known_hosts line {}).",
                self.host,
                self.port,
                fingerprint,
                line
            ),
            Err(err) => anyhow::bail!(
                "Host key verification failed for {}:{} ({}): {}",
                self.host,
                self.port,
                fingerprint,
                err
            ),
        }
    }

    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: russh::Channel<russh::client::Msg>,
        connected_address: &str,
        connected_port: u32,
        _originator_address: &str,
        _originator_port: u32,
        _session: &mut russh::client::Session,
    ) -> Result<(), Self::Error> {
        let connected_port = connected_port as u16;
        let key = (connected_address.to_string(), connected_port);
        let target = {
            let map = self.remote_forward_targets.lock().await;
            map.get(&key).cloned().or_else(|| {
                map.iter()
                    .find(|((_, port), _)| *port == connected_port)
                    .map(|(_, target)| target.clone())
            })
        };

        if let Some((target_host, target_port)) = target {
            let evt_tx = self.evt_tx.clone();
            tokio::spawn(async move {
                bridge_remote_channel_to_local(channel, target_host, target_port, evt_tx).await;
            });
        } else {
            let _ = self
                .evt_tx
                .send(SshEvent::Error(format!(
                    "Reverse forward has no target mapping for {}:{}",
                    connected_address, connected_port
                )))
                .await;
        }
        Ok(())
    }
}

pub fn start_session(
    runtime: &tokio::runtime::Handle,
    host: String,
    port: u16,
    username: String,
    auth: AuthConfig,
    initial_cols: u32,
    initial_rows: u32,
) -> (mpsc::Sender<SshCommand>, mpsc::Receiver<SshEvent>) {
    let (cmd_tx, cmd_rx) = mpsc::channel::<SshCommand>(256);
    let (evt_tx, evt_rx) = mpsc::channel::<SshEvent>(256);

    runtime.spawn(async move {
        if let Err(e) = session_task(
            host,
            port,
            username,
            auth,
            initial_cols,
            initial_rows,
            cmd_rx,
            evt_tx.clone(),
        )
        .await
        {
            let _ = evt_tx.send(SshEvent::Error(format!("{:#}", e))).await;
        }
        let _ = evt_tx.send(SshEvent::Disconnected).await;
    });

    (cmd_tx, evt_rx)
}

async fn session_task(
    host: String,
    port: u16,
    username: String,
    auth: AuthConfig,
    cols: u32,
    rows: u32,
    mut cmd_rx: mpsc::Receiver<SshCommand>,
    evt_tx: mpsc::Sender<SshEvent>,
) -> anyhow::Result<()> {
    let remote_forward_targets: Arc<Mutex<HashMap<(String, u16), (String, u16)>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let config = Arc::new(russh::client::Config::default());
    let addr = format!("{}:{}", host, port);
    let handler = SshHandler {
        host: host.clone(),
        port,
        remote_forward_targets: remote_forward_targets.clone(),
        evt_tx: evt_tx.clone(),
    };
    let handle = russh::client::connect(config, addr.as_str(), handler).await?;
    let handle = Arc::new(Mutex::new(handle));

    match auth {
        AuthConfig::Key { path, passphrase } => {
            let key =
                russh_keys::load_secret_key(&path, passphrase.as_deref().map(|p| p.as_str()))?;
            let accepted = handle
                .lock()
                .await
                .authenticate_publickey(&username, Arc::new(key))
                .await?;
            if !accepted {
                anyhow::bail!("Server rejected public key");
            }
        }
        AuthConfig::Password(password) => {
            let accepted = handle
                .lock()
                .await
                .authenticate_password(&username, password.as_str())
                .await?;
            if !accepted {
                anyhow::bail!("Server rejected password");
            }
        }
        AuthConfig::Agent { auto_discover_keys } => {
            if !auto_discover_keys {
                anyhow::bail!("SSH agent auth not implemented yet");
            }

            let mut attempted = 0usize;
            let mut accepted_any = false;
            for path in ssh_keys::discover_private_key_paths() {
                let key = match russh_keys::load_secret_key(&path, None) {
                    Ok(k) => k,
                    Err(_) => continue,
                };
                attempted += 1;
                let accepted = handle
                    .lock()
                    .await
                    .authenticate_publickey(&username, Arc::new(key))
                    .await?;
                if accepted {
                    accepted_any = true;
                    break;
                }
            }

            if attempted == 0 {
                anyhow::bail!(
                    "No usable SSH private keys found in ~/.ssh. Select a key manually or disable agent auth."
                );
            }

            if !accepted_any {
                anyhow::bail!(
                    "Server rejected all discovered SSH private keys. Select a key manually."
                );
            }
        }
    }

    evt_tx.send(SshEvent::Connected).await?;

    let mut channel = handle.lock().await.channel_open_session().await?;
    channel
        .request_pty(false, "xterm-256color", cols, rows, 0, 0, &[])
        .await?;
    channel.request_shell(false).await?;

    let mut local_forward_tasks: HashMap<String, JoinHandle<()>> = HashMap::new();
    let mut remote_forward_bindings: HashMap<String, (String, u16)> = HashMap::new();

    loop {
        tokio::select! {
            msg = channel.wait() => {
                match msg {
                    Some(russh::ChannelMsg::Data { ref data }) => {
                        evt_tx.send(SshEvent::Data(data.to_vec())).await?;
                    }
                    Some(russh::ChannelMsg::ExtendedData { ref data, .. }) => {
                        evt_tx.send(SshEvent::Data(data.to_vec())).await?;
                    }
                    Some(russh::ChannelMsg::Eof) | None => {
                        break;
                    }
                    Some(_) => {}
                }
            }
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(SshCommand::Input(data)) => {
                        channel.data(&data[..]).await?;
                    }
                    Some(SshCommand::Resize { cols, rows }) => {
                        channel.window_change(cols, rows, 0, 0).await?;
                    }
                    Some(SshCommand::AddPortForward(rule)) => {
                        if !rule.enabled {
                            continue;
                        }
                        match rule.kind {
                            ForwardType::Local => {
                                if let Some(task) = local_forward_tasks.remove(&rule.id) {
                                    task.abort();
                                }

                                let listener = TcpListener::bind((rule.listen_host.as_str(), rule.listen_port)).await?;
                                let listen_host = rule.listen_host.clone();
                                let target_host = rule.target_host.clone();
                                let target_port = rule.target_port;
                                let handle_for_accept = handle.clone();
                                let evt_for_accept = evt_tx.clone();
                                let task = tokio::spawn(async move {
                                    loop {
                                        let accepted = listener.accept().await;
                                        let (socket, peer_addr) = match accepted {
                                            Ok(v) => v,
                                            Err(e) => {
                                                let _ = evt_for_accept
                                                    .send(SshEvent::Error(format!(
                                                        "Local forward accept failed ({}:{}): {}",
                                                        listen_host, rule.listen_port, e
                                                    )))
                                                    .await;
                                                break;
                                            }
                                        };

                                        let handle_for_conn = handle_for_accept.clone();
                                        let evt_for_conn = evt_for_accept.clone();
                                        let target_host_conn = target_host.clone();
                                        tokio::spawn(async move {
                                            let channel_res = handle_for_conn
                                                .lock()
                                                .await
                                                .channel_open_direct_tcpip(
                                                    target_host_conn.clone(),
                                                    target_port as u32,
                                                    peer_addr.ip().to_string(),
                                                    peer_addr.port() as u32,
                                                )
                                                .await;

                                            let channel = match channel_res {
                                                Ok(c) => c,
                                                Err(e) => {
                                                    let _ = evt_for_conn
                                                        .send(SshEvent::Error(format!(
                                                            "Local forward open channel failed ({}:{} -> {}:{}): {}",
                                                            peer_addr.ip(), peer_addr.port(), target_host_conn, target_port, e
                                                        )))
                                                        .await;
                                                    return;
                                                }
                                            };

                                            let mut ssh_stream = channel.into_stream();
                                            let mut local_socket = socket;
                                            if let Err(e) = copy_bidirectional(&mut local_socket, &mut ssh_stream).await {
                                                let _ = evt_for_conn
                                                    .send(SshEvent::Error(format!(
                                                        "Local forward stream error ({}:{} -> {}:{}): {}",
                                                        peer_addr.ip(), peer_addr.port(), target_host_conn, target_port, e
                                                    )))
                                                    .await;
                                            }
                                        });
                                    }
                                });
                                local_forward_tasks.insert(rule.id.clone(), task);
                            }
                            ForwardType::Remote => {
                                if let Some((addr, bound_port)) = remote_forward_bindings.remove(&rule.id) {
                                    let _ = handle
                                        .lock()
                                        .await
                                        .cancel_tcpip_forward(addr.clone(), bound_port as u32)
                                        .await;
                                    remote_forward_targets.lock().await.remove(&(addr, bound_port));
                                }

                                let bound_port = handle
                                    .lock()
                                    .await
                                    .tcpip_forward(rule.listen_host.clone(), rule.listen_port as u32)
                                    .await? as u16;

                                remote_forward_bindings.insert(rule.id.clone(), (rule.listen_host.clone(), bound_port));
                                remote_forward_targets.lock().await.insert(
                                    (rule.listen_host.clone(), bound_port),
                                    (rule.target_host.clone(), rule.target_port),
                                );
                            }
                        }
                    }
                    Some(SshCommand::RemovePortForward(forward_id)) => {
                        if let Some(task) = local_forward_tasks.remove(&forward_id) {
                            task.abort();
                        }
                        if let Some((addr, bound_port)) = remote_forward_bindings.remove(&forward_id) {
                            let _ = handle
                                .lock()
                                .await
                                .cancel_tcpip_forward(addr.clone(), bound_port as u32)
                                .await;
                            remote_forward_targets.lock().await.remove(&(addr, bound_port));
                        }
                    }
                    Some(SshCommand::ClearPortForwards) => {
                        for (_, task) in local_forward_tasks.drain() {
                            task.abort();
                        }
                        for (_, (addr, bound_port)) in remote_forward_bindings.drain() {
                            let _ = handle
                                .lock()
                                .await
                                .cancel_tcpip_forward(addr.clone(), bound_port as u32)
                                .await;
                        }
                        remote_forward_targets.lock().await.clear();
                    }
                    Some(SshCommand::Disconnect) | None => {
                        break;
                    }
                }
            }
        }
    }

    for (_, task) in local_forward_tasks.drain() {
        task.abort();
    }
    for (_, (addr, bound_port)) in remote_forward_bindings.drain() {
        let _ = handle
            .lock()
            .await
            .cancel_tcpip_forward(addr, bound_port as u32)
            .await;
    }
    let _ = channel.eof().await;
    Ok(())
}
