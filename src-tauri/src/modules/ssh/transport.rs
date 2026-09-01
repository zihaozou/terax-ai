use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{self, Handle};
use russh::keys::PublicKeyOrCertificate;
use russh::{Channel, Disconnect};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, Mutex as AsyncMutex};

use super::errors::{SshError, SshErrorCode};
use super::known_hosts::KnownHosts;
use super::types::{HostKeyDecision, PresentedHostKey, ResolvedSshConfig};

#[derive(Clone)]
pub(crate) struct ClientHandler {
    host: String,
    port: u16,
    known_hosts: KnownHosts,
    trust_unknown: bool,
    verification_error: Arc<Mutex<Option<SshError>>>,
    trust_bridge: Option<HostTrustBridge>,
}

#[derive(Clone)]
pub(crate) struct HostTrustBridge {
    pub presented: mpsc::Sender<PresentedHostKey>,
    pub decisions: Arc<AsyncMutex<mpsc::Receiver<bool>>>,
}

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let key = server_key.public_key();
        let presented = match key.to_bytes() {
            Ok(blob) => PresentedHostKey {
                algorithm: key.algorithm().as_str().to_owned(),
                blob,
            },
            Err(_) => return Ok(false),
        };
        match self.known_hosts.check(&self.host, self.port, &presented) {
            Ok(HostKeyDecision::Match) => Ok(true),
            Ok(HostKeyDecision::Unknown) if self.trust_unknown => Ok(true),
            Ok(HostKeyDecision::Unknown) => {
                if let Some(bridge) = &self.trust_bridge {
                    if bridge.presented.send(presented).await.is_ok()
                        && bridge.decisions.lock().await.recv().await == Some(true)
                    {
                        return Ok(true);
                    }
                }
                self.record_error(SshError::new(
                    SshErrorCode::HostUnknown,
                    "verifying host key",
                    &self.host,
                    "host key requires explicit trust",
                ));
                Ok(false)
            }
            Ok(HostKeyDecision::Mismatch { .. }) => {
                self.record_error(SshError::new(
                    SshErrorCode::HostKeyMismatch,
                    "verifying host key",
                    &self.host,
                    "host key differs from the trusted key",
                ));
                Ok(false)
            }
            Err(error) => {
                self.record_error(error);
                Ok(false)
            }
        }
    }
}

impl ClientHandler {
    fn record_error(&self, error: SshError) {
        *self
            .verification_error
            .lock()
            .expect("SSH verification error mutex poisoned") = Some(error);
    }
}

pub struct AuthenticatedTransport {
    handle: Handle<ClientHandler>,
    proxy_child: Option<Child>,
}

impl AuthenticatedTransport {
    pub(crate) async fn connect_unauthenticated(
        config: &ResolvedSshConfig,
        trust_unknown: bool,
        proxy_command_approved: bool,
        trust_bridge: Option<HostTrustBridge>,
    ) -> Result<Self, SshError> {
        let verification_error = Arc::new(Mutex::new(None));
        let handler = ClientHandler {
            host: config.host.clone(),
            port: config.port,
            known_hosts: KnownHosts::with_system_hosts(
                config.known_hosts_files.iter().map(PathBuf::from).collect(),
            ),
            trust_unknown,
            verification_error: verification_error.clone(),
            trust_bridge,
        };
        let client_config = Arc::new(client::Config {
            inactivity_timeout: Some(Duration::from_secs(30)),
            keepalive_interval: Some(Duration::from_secs(15)),
            keepalive_max: 3,
            ..Default::default()
        });

        let (handle, proxy_child) = if let Some(command) = config.proxy_command.as_deref() {
            if !proxy_command_approved {
                return Err(SshError::new(
                    SshErrorCode::ProxyCommandDenied,
                    "opening proxy command",
                    &config.profile_id,
                    "ProxyCommand requires explicit consent",
                ));
            }
            let mut child = proxy_process(command)?;
            let stdout = child.stdout.take().ok_or_else(|| proxy_error(config))?;
            let stdin = child.stdin.take().ok_or_else(|| proxy_error(config))?;
            let stream = tokio::io::join(stdout, stdin);
            let handle = client::connect_stream(client_config, stream, handler)
                .await
                .map_err(|error| connect_error(config, &verification_error, error))?;
            (handle, Some(child))
        } else {
            let handle =
                client::connect(client_config, (config.host.as_str(), config.port), handler)
                    .await
                    .map_err(|error| connect_error(config, &verification_error, error))?;
            (handle, None)
        };

        Ok(Self {
            handle,
            proxy_child,
        })
    }

    pub(crate) fn handle_mut(&mut self) -> &mut Handle<ClientHandler> {
        &mut self.handle
    }

    pub async fn open_session(&self) -> Result<Channel<client::Msg>, SshError> {
        self.handle
            .channel_open_session()
            .await
            .map_err(|error| transport_error("opening PTY channel", error))
    }

    pub async fn open_direct_tcpip(
        &self,
        host: &str,
        port: u32,
        originator_host: &str,
        originator_port: u32,
    ) -> Result<Channel<client::Msg>, SshError> {
        self.handle
            .channel_open_direct_tcpip(host, port, originator_host, originator_port)
            .await
            .map_err(|error| transport_error("opening ProxyJump channel", error))
    }

    pub async fn disconnect(&self) {
        let _ = self
            .handle
            .disconnect(Disconnect::ByApplication, "Terax disconnect", "en")
            .await;
    }
}

impl Drop for AuthenticatedTransport {
    fn drop(&mut self) {
        if let Some(child) = self.proxy_child.as_mut() {
            let _ = child.start_kill();
        }
    }
}

fn proxy_process(command: &str) -> Result<Child, SshError> {
    #[cfg(windows)]
    let mut process = {
        let mut process = Command::new("cmd.exe");
        process.args(["/C", command]);
        process
    };
    #[cfg(not(windows))]
    let mut process = {
        let mut process = Command::new("sh");
        process.args(["-c", command]);
        process
    };
    process
        .kill_on_drop(true)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|error| {
            SshError::new(
                SshErrorCode::ProxyCommandFailed,
                "opening proxy command",
                "ProxyCommand",
                error.to_string(),
            )
        })
}

fn proxy_error(config: &ResolvedSshConfig) -> SshError {
    SshError::new(
        SshErrorCode::ProxyCommandFailed,
        "opening proxy command",
        &config.profile_id,
        "ProxyCommand did not provide piped input and output",
    )
}

fn connect_error(
    config: &ResolvedSshConfig,
    verification_error: &Mutex<Option<SshError>>,
    error: russh::Error,
) -> SshError {
    if let Some(error) = verification_error
        .lock()
        .expect("SSH verification error mutex poisoned")
        .take()
    {
        return error;
    }
    SshError::new(
        SshErrorCode::ConnectionRefused,
        "connecting",
        format!("{}:{}", config.host, config.port),
        error.to_string(),
    )
}

fn transport_error(stage: &str, error: russh::Error) -> SshError {
    SshError::new(
        SshErrorCode::TransportLost,
        stage,
        "SSH transport",
        error.to_string(),
    )
}
