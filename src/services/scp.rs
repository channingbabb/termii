use anyhow::{Context, Result};

// using ssh2 for scp...
// https://github.com/Eugeny/russh/issues/287
// currently does not support scp but there is an open issue to add it
use ssh2::{FileStat, Session, Sftp};
use std::fs::{self, File};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;
use zeroize::Zeroizing;

#[derive(Debug, Clone)]
pub enum ScpAuth {
    Key {
        path: PathBuf,
        passphrase: Option<Zeroizing<String>>,
    },
    Password(Zeroizing<String>),
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScpDirection {
    LocalToRemote,
    RemoteToLocal,
}

#[derive(Debug, Clone)]
pub struct ScpTransferRequest {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: ScpAuth,
    pub direction: ScpDirection,
    pub sources: Vec<String>,
    pub destination: String,
}

#[derive(Debug, Clone)]
pub struct ScpTransferSummary {
    pub files_transferred: usize,
    pub workers_used: usize,
}

#[derive(Clone)]
struct ManifestItem {
    local_path: PathBuf,
    remote_path: String,
}

pub fn run_transfer(request: ScpTransferRequest) -> Result<ScpTransferSummary> {
    if request.sources.is_empty() {
        anyhow::bail!("No transfer sources selected");
    }

    let items = match request.direction {
        ScpDirection::LocalToRemote => build_upload_manifest(&request)?,
        ScpDirection::RemoteToLocal => build_download_manifest(&request)?,
    };

    if items.is_empty() {
        anyhow::bail!("No files found to transfer");
    }

    let cpu_count = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .max(1);
    let workers = cpu_count.min(items.len()).max(1);

    let (tx, rx) = mpsc::channel::<ManifestItem>();
    for item in items {
        tx.send(item)
            .context("Failed to queue transfer item for workers")?;
    }
    drop(tx);

    let rx = Arc::new(Mutex::new(rx));
    let mut handles = Vec::with_capacity(workers);

    for _ in 0..workers {
        let req = request.clone();
        let rx = rx.clone();
        let handle = thread::spawn(move || worker_loop(req, rx));
        handles.push(handle);
    }

    let mut total = 0usize;
    for handle in handles {
        let count = handle
            .join()
            .map_err(|_| anyhow::anyhow!("SCP worker thread panicked"))??;
        total += count;
    }

    Ok(ScpTransferSummary {
        files_transferred: total,
        workers_used: workers,
    })
}

fn worker_loop(
    request: ScpTransferRequest,
    rx: Arc<Mutex<mpsc::Receiver<ManifestItem>>>,
) -> Result<usize> {
    let sess = connect(&request)?;
    let sftp = sess.sftp().context("Failed to open SFTP channel")?;

    let mut transferred = 0usize;
    loop {
        let next = {
            let lock = rx.lock().map_err(|_| anyhow::anyhow!("Queue poisoned"))?;
            lock.recv().ok()
        };

        let Some(item) = next else {
            break;
        };

        match request.direction {
            ScpDirection::LocalToRemote => {
                upload_file(&sess, &sftp, &item.local_path, &item.remote_path)?
            }
            ScpDirection::RemoteToLocal => {
                download_file(&sess, &item.remote_path, &item.local_path)?
            }
        }
        transferred += 1;
    }

    Ok(transferred)
}

fn connect(request: &ScpTransferRequest) -> Result<Session> {
    let addr = format!("{}:{}", request.host, request.port);
    let tcp =
        TcpStream::connect(&addr).with_context(|| format!("Failed to connect to {}", addr))?;
    tcp.set_read_timeout(Some(Duration::from_secs(120))).ok();
    tcp.set_write_timeout(Some(Duration::from_secs(120))).ok();

    let mut session = Session::new().context("Failed to create SSH session")?;
    session.set_tcp_stream(tcp);
    session.handshake().context("SSH handshake failed")?;

    match &request.auth {
        ScpAuth::Key { path, passphrase } => {
            session
                .userauth_pubkey_file(
                    &request.username,
                    None,
                    path,
                    passphrase.as_deref().map(|p| p.as_str()),
                )
                .with_context(|| format!("Key auth failed for {}", request.username))?;
        }
        ScpAuth::Password(password) => {
            session
                .userauth_password(&request.username, password)
                .with_context(|| format!("Password auth failed for {}", request.username))?;
        }
        ScpAuth::Agent => {
            session
                .userauth_agent(&request.username)
                .with_context(|| format!("SSH agent auth failed for {}", request.username))?;
        }
    }

    if !session.authenticated() {
        anyhow::bail!("Authentication failed");
    }

    Ok(session)
}

fn build_upload_manifest(request: &ScpTransferRequest) -> Result<Vec<ManifestItem>> {
    let mut manifest = Vec::new();
    let destination_root = normalize_remote_path(&request.destination);

    for src in &request.sources {
        let source_path = PathBuf::from(src);
        if !source_path.exists() {
            anyhow::bail!("Source path not found: {}", src);
        }
        let base_name = source_path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .ok_or_else(|| anyhow::anyhow!("Invalid source path: {}", src))?;

        if source_path.is_file() {
            manifest.push(ManifestItem {
                local_path: source_path.clone(),
                remote_path: remote_join(&destination_root, &base_name),
            });
        } else {
            gather_local_dir_files(
                &source_path,
                &source_path,
                &remote_join(&destination_root, &base_name),
                &mut manifest,
            )?;
        }
    }

    Ok(manifest)
}

fn gather_local_dir_files(
    root: &Path,
    current: &Path,
    remote_root: &str,
    manifest: &mut Vec<ManifestItem>,
) -> Result<()> {
    for entry in fs::read_dir(current)
        .with_context(|| format!("Failed to read directory {}", current.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            gather_local_dir_files(root, &path, remote_root, manifest)?;
            continue;
        }
        if !path.is_file() {
            continue;
        }

        let rel = path
            .strip_prefix(root)
            .with_context(|| format!("Failed to compute relative path for {}", path.display()))?;
        let rel_remote = rel.to_string_lossy().replace('\\', "/");
        manifest.push(ManifestItem {
            local_path: path,
            remote_path: remote_join(remote_root, &rel_remote),
        });
    }
    Ok(())
}

fn build_download_manifest(request: &ScpTransferRequest) -> Result<Vec<ManifestItem>> {
    let sess = connect(request)?;
    let sftp = sess.sftp().context("Failed to open SFTP channel")?;

    let mut manifest = Vec::new();
    let destination_root = PathBuf::from(request.destination.trim());
    if request.sources.is_empty() {
        anyhow::bail!("No remote sources selected");
    }

    for source in &request.sources {
        let remote_source = normalize_remote_path(source);
        let stat = sftp
            .stat(Path::new(&remote_source))
            .with_context(|| format!("Remote path not found: {}", remote_source))?;

        let base_name = basename_remote(&remote_source)
            .ok_or_else(|| anyhow::anyhow!("Invalid remote source path: {}", remote_source))?;

        if is_remote_dir(&stat) {
            gather_remote_dir_files(
                &sftp,
                &remote_source,
                &remote_source,
                &destination_root.join(base_name),
                &mut manifest,
            )?;
        } else {
            manifest.push(ManifestItem {
                local_path: destination_root.join(base_name),
                remote_path: remote_source,
            });
        }
    }

    Ok(manifest)
}

fn gather_remote_dir_files(
    sftp: &Sftp,
    root: &str,
    current: &str,
    local_root: &Path,
    manifest: &mut Vec<ManifestItem>,
) -> Result<()> {
    let entries = sftp
        .readdir(Path::new(current))
        .with_context(|| format!("Failed to read remote directory {}", current))?;

    for (entry_path, stat) in entries {
        let name = match entry_path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if name == "." || name == ".." {
            continue;
        }

        let entry_remote = remote_join(current, name);
        if is_remote_dir(&stat) {
            gather_remote_dir_files(sftp, root, &entry_remote, local_root, manifest)?;
            continue;
        }
        if !is_remote_file(&stat) {
            continue;
        }

        let rel = entry_remote
            .strip_prefix(root)
            .unwrap_or(name)
            .trim_start_matches('/');
        manifest.push(ManifestItem {
            local_path: local_root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR)),
            remote_path: entry_remote,
        });
    }

    Ok(())
}

fn upload_file(session: &Session, sftp: &Sftp, local_path: &Path, remote_path: &str) -> Result<()> {
    if let Some(parent) = remote_parent(remote_path) {
        ensure_remote_dir_all(sftp, parent)?;
    }

    let mut input = File::open(local_path)
        .with_context(|| format!("Failed to open local file {}", local_path.display()))?;
    let size = input.metadata()?.len();

    let mut remote = session
        .scp_send(Path::new(remote_path), 0o644, size, None)
        .with_context(|| format!("Failed to open remote destination {}", remote_path))?;

    std::io::copy(&mut input, &mut remote)
        .with_context(|| format!("Failed to upload {}", local_path.display()))?;
    remote.send_eof().ok();
    remote.wait_eof().ok();
    remote.close().ok();
    remote.wait_close().ok();
    Ok(())
}

fn download_file(session: &Session, remote_path: &str, local_path: &Path) -> Result<()> {
    if let Some(parent) = local_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create local directory {}", parent.display()))?;
    }

    let (mut remote, _) = session
        .scp_recv(Path::new(remote_path))
        .with_context(|| format!("Failed to open remote file {}", remote_path))?;

    let mut output = File::create(local_path)
        .with_context(|| format!("Failed to create local file {}", local_path.display()))?;
    std::io::copy(&mut remote, &mut output)
        .with_context(|| format!("Failed to download {}", remote_path))?;

    remote.send_eof().ok();
    remote.wait_eof().ok();
    remote.close().ok();
    remote.wait_close().ok();
    Ok(())
}

fn ensure_remote_dir_all(sftp: &Sftp, remote_dir: &str) -> Result<()> {
    let normalized = normalize_remote_path(remote_dir);
    if normalized == "/" {
        return Ok(());
    }

    let mut current = String::new();
    let is_absolute = normalized.starts_with('/');
    if is_absolute {
        current.push('/');
    }

    for part in normalized.split('/').filter(|p| !p.is_empty()) {
        if !current.ends_with('/') && !current.is_empty() {
            current.push('/');
        }
        current.push_str(part);

        let path = Path::new(&current);
        if sftp.stat(path).is_ok() {
            continue;
        }

        if let Err(err) = sftp.mkdir(path, 0o755) {
            if sftp.stat(path).is_err() {
                return Err(err)
                    .with_context(|| format!("Failed to create remote directory {}", current));
            }
        }
    }

    Ok(())
}

fn normalize_remote_path(path: &str) -> String {
    let trimmed = path.trim().replace('\\', "/");
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed
    }
}

fn remote_join(base: &str, child: &str) -> String {
    let b = normalize_remote_path(base);
    let c = child.trim_matches('/');
    if b == "/" {
        format!("/{}", c)
    } else {
        format!("{}/{}", b.trim_end_matches('/'), c)
    }
}

fn remote_parent(path: &str) -> Option<&str> {
    path.rsplit_once('/')
        .map(|(parent, _)| if parent.is_empty() { "/" } else { parent })
}

fn basename_remote(path: &str) -> Option<String> {
    let normalized = normalize_remote_path(path);
    let trimmed = normalized.trim_end_matches('/');
    if trimmed.is_empty() || trimmed == "/" {
        return None;
    }
    trimmed.rsplit('/').next().map(|s| s.to_string())
}

fn is_remote_dir(stat: &FileStat) -> bool {
    const S_IFMT: u32 = 0o170000;
    const S_IFDIR: u32 = 0o040000;
    stat.perm.map(|p| (p & S_IFMT) == S_IFDIR).unwrap_or(false)
}

fn is_remote_file(stat: &FileStat) -> bool {
    const S_IFMT: u32 = 0o170000;
    const S_IFREG: u32 = 0o100000;
    stat.perm.map(|p| (p & S_IFMT) == S_IFREG).unwrap_or(false)
}
