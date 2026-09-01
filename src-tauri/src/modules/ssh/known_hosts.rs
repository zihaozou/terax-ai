use std::{
    collections::HashMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use tokio::sync::Mutex as AsyncMutex;

use super::types::{HostKeyDecision, PresentedHostKey};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnownHostEntry {
    pub host: String,
    pub port: u16,
    pub key: PresentedHostKey,
}

#[derive(Clone, Debug)]
pub struct KnownHosts {
    paths: Vec<PathBuf>,
}

impl KnownHosts {
    pub fn new(paths: Vec<PathBuf>) -> Self {
        Self { paths }
    }

    pub fn with_system_hosts(user_paths: Vec<PathBuf>) -> Self {
        let mut paths = user_paths;
        paths.extend(system_known_hosts_files());
        Self::new(paths)
    }

    pub fn check(&self, host: &str, port: u16, key: &PresentedHostKey) -> HostKeyDecision {
        for path in &self.paths {
            let Ok(recorded_keys) =
                russh::keys::known_hosts::known_host_keys_path(host, port, path)
            else {
                continue;
            };
            for (line, recorded) in recorded_keys {
                if recorded.algorithm().as_str() != key.algorithm {
                    continue;
                }
                let Ok(recorded_blob) = recorded.to_bytes() else {
                    continue;
                };
                if recorded_blob == key.blob {
                    return HostKeyDecision::Match;
                }
                return HostKeyDecision::Mismatch {
                    old_fingerprint: host_fingerprint(&recorded_blob),
                    new_fingerprint: host_fingerprint(&key.blob),
                    file: path.clone(),
                    line,
                };
            }
        }
        HostKeyDecision::Unknown
    }
}

pub fn check_host_key(
    host: &str,
    port: u16,
    key: &PresentedHostKey,
    user_known_hosts_files: Vec<PathBuf>,
) -> HostKeyDecision {
    KnownHosts::with_system_hosts(user_known_hosts_files).check(host, port, key)
}

pub fn host_fingerprint(blob: &[u8]) -> String {
    let digest = Sha256::digest(blob);
    format!("SHA256:{}", STANDARD_NO_PAD.encode(digest))
}

pub fn known_host_name(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_owned()
    } else {
        format!("[{host}]:{port}")
    }
}

pub async fn save_host_key(path: &Path, entry: KnownHostEntry) -> io::Result<()> {
    let path = canonical_write_target(path)?;
    let lock = write_lock(&path);
    let _guard = lock.lock().await;
    save_host_key_locked(&path, &entry)
}

fn canonical_write_target(path: &Path) -> io::Result<PathBuf> {
    if path.exists() {
        return path.canonicalize();
    }
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "known_hosts path has no parent",
        )
    })?;
    let name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "known_hosts path has no file name",
        )
    })?;
    Ok(parent.canonicalize()?.join(name))
}

fn write_locks() -> &'static Mutex<HashMap<PathBuf, Arc<AsyncMutex<()>>>> {
    static LOCKS: OnceLock<Mutex<HashMap<PathBuf, Arc<AsyncMutex<()>>>>> = OnceLock::new();
    LOCKS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn write_lock(path: &Path) -> Arc<AsyncMutex<()>> {
    let mut locks = write_locks()
        .lock()
        .expect("known_hosts write locks poisoned");
    locks
        .entry(path.to_owned())
        .or_insert_with(|| Arc::new(AsyncMutex::new(())))
        .clone()
}

fn save_host_key_locked(path: &Path, entry: &KnownHostEntry) -> io::Result<()> {
    match KnownHosts::new(vec![path.to_owned()]).check(&entry.host, entry.port, &entry.key) {
        HostKeyDecision::Match => return Ok(()),
        HostKeyDecision::Mismatch { .. } => {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "refusing to replace a known host key",
            ));
        }
        HostKeyDecision::Unknown => {}
    }
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "known_hosts path has no parent",
        )
    })?;
    let existing = fs::read(path).unwrap_or_default();
    let permissions = fs::metadata(path)
        .ok()
        .map(|metadata| metadata.permissions());
    let mut contents = existing;
    if !contents.is_empty() && !contents.ends_with(b"\n") {
        contents.push(b'\n');
    }
    contents.extend_from_slice(format_known_host_entry(entry).as_bytes());

    let mut temporary = NamedTempFile::new_in(parent)?;
    if let Some(permissions) = permissions {
        temporary.as_file().set_permissions(permissions)?;
    }
    temporary.write_all(&contents)?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    sync_parent(parent);
    Ok(())
}

fn format_known_host_entry(entry: &KnownHostEntry) -> String {
    format!(
        "{} {} {}\n",
        known_host_name(&entry.host, entry.port),
        entry.key.algorithm,
        STANDARD_NO_PAD.encode(&entry.key.blob)
    )
}

#[cfg(unix)]
fn sync_parent(parent: &Path) {
    if let Ok(directory) = fs::File::open(parent) {
        let _ = directory.sync_all();
    }
}

#[cfg(not(unix))]
fn sync_parent(_: &Path) {}

fn system_known_hosts_files() -> Vec<PathBuf> {
    #[cfg(unix)]
    {
        vec![
            PathBuf::from("/etc/ssh/ssh_known_hosts"),
            PathBuf::from("/etc/ssh/ssh_known_hosts2"),
        ]
    }
    #[cfg(windows)]
    {
        std::env::var_os("ProgramData")
            .map(PathBuf::from)
            .map(|directory| {
                vec![
                    directory.join("ssh").join("ssh_known_hosts"),
                    directory.join("ssh").join("ssh_known_hosts2"),
                ]
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use base64::engine::general_purpose::STANDARD;
    use tempfile::TempDir;

    use super::*;

    const KEY_ONE: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ";
    const KEY_TWO: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIA6rWI3G1sz07DnfFlrouTcysQlj2P+jpNSOEWD9OJ3X";

    #[test]
    fn known_unknown_and_changed_keys_are_distinct() {
        let directory = TempDir::new().unwrap();
        let fixture = directory.path().join("known_hosts");
        fs::write(&fixture, format!("prod.example ssh-ed25519 {KEY_ONE}\n")).unwrap();
        let store = KnownHosts::new(vec![fixture]);
        assert_eq!(
            store.check("prod.example", 22, &key(KEY_ONE)),
            HostKeyDecision::Match
        );
        assert_eq!(
            store.check("new.example", 22, &key(KEY_TWO)),
            HostKeyDecision::Unknown
        );
        assert!(matches!(
            store.check("prod.example", 22, &key(KEY_TWO)),
            HostKeyDecision::Mismatch { .. }
        ));
    }

    #[test]
    fn nonstandard_ports_use_bracketed_host_form() {
        assert_eq!(known_host_name("prod.example", 2202), "[prod.example]:2202");
    }

    #[test]
    #[cfg(unix)]
    fn concurrent_save_preserves_unrelated_entries_and_mode() {
        use std::os::unix::fs::PermissionsExt;

        let directory = TempDir::new().unwrap();
        let fixture = directory.path().join("known_hosts");
        fs::write(&fixture, b"").unwrap();
        fs::set_permissions(&fixture, fs::Permissions::from_mode(0o600)).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            tokio::try_join!(
                save_host_key(&fixture, entry("one.example", 22, key(KEY_ONE))),
                save_host_key(&fixture, entry("two.example", 22, key(KEY_TWO))),
            )
            .unwrap();
        });
        assert_eq!(fixture_mode(&fixture), 0o600);
        let text = fs::read_to_string(&fixture).unwrap();
        assert!(text.contains("one.example"));
        assert!(text.contains("two.example"));
    }

    #[test]
    fn saving_changed_key_is_rejected() {
        let directory = TempDir::new().unwrap();
        let fixture = directory.path().join("known_hosts");
        fs::write(&fixture, format!("prod.example ssh-ed25519 {KEY_ONE}\n")).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        let error = runtime
            .block_on(save_host_key(
                &fixture,
                entry("prod.example", 22, key(KEY_TWO)),
            ))
            .unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(
            fs::read_to_string(&fixture).unwrap(),
            format!("prod.example ssh-ed25519 {KEY_ONE}\n")
        );
    }

    #[test]
    fn fingerprint_uses_openssh_sha256_format() {
        assert_eq!(
            host_fingerprint(b"hello"),
            "SHA256:LPJNul+wow4m6DsqxbninhsWHlwfp0JecwQzYpOLmCQ"
        );
    }

    fn key(encoded: &str) -> PresentedHostKey {
        PresentedHostKey {
            algorithm: "ssh-ed25519".to_owned(),
            blob: STANDARD.decode(encoded).unwrap(),
        }
    }

    fn entry(host: &str, port: u16, key: PresentedHostKey) -> KnownHostEntry {
        KnownHostEntry {
            host: host.to_owned(),
            port,
            key,
        }
    }

    #[cfg(unix)]
    fn fixture_mode(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }
}
