//! Manages the `anemll-serverd` sidecar: the local ANE code-completion
//! daemon backing the editor's "OpenAI compatible" autocomplete provider.
//!
//! Terax spawns it on launch, if enabled in settings, and kills it on quit.
//! If a server is already listening on the configured port — a dev
//! instance's own sidecar, a manually started one, whatever — that is
//! detected via a TCP probe and we deliberately do NOT spawn or own it:
//! killing a process we didn't start would surprise whoever did.

use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Manager};

const CONNECT_PROBE_TIMEOUT: Duration = Duration::from_millis(300);
const DEFAULT_MAX_TOKENS: u32 = 48;
const DEFAULT_MAX_NEWLINES: u32 = 5;

#[derive(Default)]
pub struct SidecarState {
    child: Mutex<Option<Child>>,
}

impl SidecarState {
    fn replace(&self, child: Child) {
        let mut guard = self.child.lock().expect("SidecarState mutex poisoned");
        if let Some(mut previous) = guard.take() {
            let _ = previous.kill();
            let _ = previous.wait();
        }
        *guard = Some(child);
    }

    /// Kills the owned child, if any. No-op if we never spawned one (e.g.
    /// we found a foreign server already listening and left it alone).
    pub fn kill_owned(&self) {
        if let Some(mut child) = self
            .child
            .lock()
            .expect("SidecarState mutex poisoned")
            .take()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn port_is_listening(port: u16) -> bool {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    TcpStream::connect_timeout(&addr, CONNECT_PROBE_TIMEOUT).is_ok()
}

/// Resolve the bundled `anemll-serverd` binary next to the running app in a
/// release build, or under `src-tauri/binaries/` (produced by
/// `scripts/build-sidecar.mjs`) in dev. Mirrors
/// `control::find_bundled_cli`.
fn find_bundled_serverd() -> Option<PathBuf> {
    let filename = "anemll-serverd";
    if let Some(path) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|parent| parent.join(filename)))
        .filter(|path| is_executable_candidate(path))
    {
        return Some(path);
    }

    if cfg!(debug_assertions) {
        let binaries = Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries");
        let target = option_env!("TAURI_ENV_TARGET_TRIPLE")?;
        let candidate = binaries.join(format!(
            "{filename}-{target}{}",
            std::env::consts::EXE_SUFFIX
        ));
        return is_executable_candidate(&candidate).then_some(candidate);
    }
    None
}

fn is_executable_candidate(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
}

#[tauri::command]
pub async fn sidecar_start(
    app: AppHandle,
    state: tauri::State<'_, SidecarState>,
    model_dir: String,
    port: u16,
    max_tokens: u32,
    max_newlines: u32,
) -> Result<String, String> {
    if port_is_listening(port) {
        log::info!("anemll-serverd: port {port} already serving, not spawning a new instance");
        return Ok("already-running".to_string());
    }

    let Some(binary) = find_bundled_serverd() else {
        log::warn!("bundled anemll-serverd executable not found; local completion server disabled");
        return Err("anemll-serverd binary not found".to_string());
    };

    let log_dir = app.path().app_log_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&log_dir).map_err(|e| e.to_string())?;
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("sidecar.log"))
        .map_err(|e| e.to_string())?;
    let log_file_err = log_file.try_clone().map_err(|e| e.to_string())?;

    let max_tokens = if max_tokens == 0 {
        DEFAULT_MAX_TOKENS
    } else {
        max_tokens
    };
    let max_newlines = if max_newlines == 0 {
        DEFAULT_MAX_NEWLINES
    } else {
        max_newlines
    };

    let child = Command::new(&binary)
        .args([
            "--model-dir",
            &model_dir,
            "--port",
            &port.to_string(),
            "--max-tokens",
            &max_tokens.to_string(),
            "--max-newlines",
            &max_newlines.to_string(),
        ])
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_file_err))
        .spawn()
        .map_err(|e| e.to_string())?;

    state.replace(child);
    log::info!("anemll-serverd: spawned on port {port} (model_dir={model_dir})");
    Ok("spawned".to_string())
}

#[tauri::command]
pub fn sidecar_stop(state: tauri::State<'_, SidecarState>) {
    state.kill_owned();
}

#[cfg(test)]
mod tests {
    use super::port_is_listening;
    use std::net::TcpListener;

    #[test]
    fn detects_a_listening_port() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let port = listener.local_addr().expect("local addr").port();
        assert!(port_is_listening(port));
    }

    #[test]
    fn reports_false_once_nothing_is_listening() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let port = listener.local_addr().expect("local addr").port();
        drop(listener);
        assert!(!port_is_listening(port));
    }
}
