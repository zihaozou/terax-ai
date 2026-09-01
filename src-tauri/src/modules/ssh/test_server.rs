use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use russh::keys::PrivateKey;
use russh::server::{self, Auth, Msg, Response, Server as _, Session};
use russh::{Channel, ChannelId};
use tokio::net::TcpListener;

use super::known_hosts::{save_host_key, KnownHostEntry};
use super::types::{
    AddKeysToAgent, AuthAnswer, AuthMethod, PresentedHostKey, ResolvedSshConfig, SshConnectRequest,
};

const TEST_HOST_KEY: &str = "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW\nQyNTUxOQAAACBgDNH4hM1CYxR49wyoO7fJVwTLJgLSy0lEVKdOCxdZxgAAAKiSByPskgcj\n7AAAAAtzc2gtZWQyNTUxOQAAACBgDNH4hM1CYxR49wyoO7fJVwTLJgLSy0lEVKdOCxdZxg\nAAAEB53dSlBZiXS/JrjhTKIeJM/9NvJT6rumImMLPiw18IPmAM0fiEzUJjFHj3DKg7t8lX\nBMsmAtLLSURUp04LF1nGAAAAIXppaGFvem91QFppaGFvcy1NYWNCb29rLVByby5sb2NhbA\nECAwQ=\n-----END OPENSSH PRIVATE KEY-----\n";

#[derive(Default)]
struct TestServerState {
    shell_open_count: AtomicU32,
    columns: AtomicU32,
    rows: AtomicU32,
    fail_home: AtomicBool,
    burst_output: AtomicBool,
}

#[derive(Clone)]
enum TestAuth {
    Password(Arc<str>),
    PrivateKey(russh::keys::ssh_key::PublicKey),
    KeyboardInteractive(Arc<str>),
}

#[derive(Clone)]
struct Handler {
    user: Arc<str>,
    auth: TestAuth,
    state: Arc<TestServerState>,
}

impl server::Server for Handler {
    type Handler = Self;

    fn new_client(&mut self, _peer_addr: Option<std::net::SocketAddr>) -> Self::Handler {
        self.clone()
    }
}

impl server::Handler for Handler {
    type Error = russh::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        Ok(
            if user == &*self.user
                && matches!(&self.auth, TestAuth::Password(expected) if password == &**expected)
            {
                Auth::Accept
            } else {
                Auth::reject()
            },
        )
    }

    async fn auth_publickey(
        &mut self,
        user: &str,
        public_key: &russh::keys::ssh_key::PublicKey,
    ) -> Result<Auth, Self::Error> {
        Ok(
            if user == &*self.user
                && matches!(&self.auth, TestAuth::PrivateKey(expected) if expected.key_data() == public_key.key_data())
            {
                Auth::Accept
            } else {
                Auth::reject()
            },
        )
    }

    async fn auth_keyboard_interactive<'a>(
        &'a mut self,
        user: &str,
        _submethods: &str,
        mut response: Option<Response<'a>>,
    ) -> Result<Auth, Self::Error> {
        let TestAuth::KeyboardInteractive(expected) = &self.auth else {
            return Ok(Auth::reject());
        };
        if user != &*self.user {
            return Ok(Auth::reject());
        }
        if let Some(response) = response.as_mut() {
            return Ok(if response.next().as_deref() == Some(expected.as_bytes()) {
                Auth::Accept
            } else {
                Auth::reject()
            });
        }
        Ok(Auth::Partial {
            name: Cow::Borrowed("Terax test"),
            instructions: Cow::Borrowed("Enter the response"),
            prompts: Cow::Owned(vec![(Cow::Borrowed("Code:"), false)]),
        })
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        reply: server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        _channel: ChannelId,
        _term: &str,
        columns: u32,
        rows: u32,
        _pix_width: u32,
        _pix_height: u32,
        _terminal_modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.state.columns.store(columns, Ordering::Relaxed);
        self.state.rows.store(rows, Ordering::Relaxed);
        session.request_success();
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.state.shell_open_count.fetch_add(1, Ordering::Relaxed);
        session.request_success();
        if self.state.burst_output.swap(false, Ordering::Relaxed) {
            for _ in 0..65 {
                session.data(channel, vec![b'x'])?;
            }
        }
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.data(channel, data.to_vec())?;
        Ok(())
    }

    async fn window_change_request(
        &mut self,
        _channel: ChannelId,
        columns: u32,
        rows: u32,
        _pix_width: u32,
        _pix_height: u32,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.state.columns.store(columns, Ordering::Relaxed);
        self.state.rows.store(rows, Ordering::Relaxed);
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        _data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.request_success();
        if self.state.fail_home.load(Ordering::Relaxed) {
            session.exit_status_request(channel, 1)?;
        } else {
            session.data(channel, b"/home/user".to_vec())?;
            session.exit_status_request(channel, 0)?;
        }
        session.close(channel)?;
        Ok(())
    }
}

pub struct TestSshServer {
    address: std::net::SocketAddr,
    user: String,
    auth: TestAuth,
    identity_dir: Option<tempfile::TempDir>,
    known_hosts_dir: tempfile::TempDir,
    state: Arc<TestServerState>,
    handle: server::RunningServerHandle,
}

impl TestSshServer {
    pub async fn password(user: &str, password: &str) -> Self {
        Self::start(user, TestAuth::Password(Arc::from(password)), None).await
    }

    pub async fn private_key(user: &str) -> Self {
        let key = PrivateKey::from_openssh(TEST_HOST_KEY).unwrap();
        let identity_dir = tempfile::tempdir().unwrap();
        std::fs::write(identity_dir.path().join("id_ed25519"), TEST_HOST_KEY).unwrap();
        Self::start(
            user,
            TestAuth::PrivateKey(key.public_key().clone()),
            Some(identity_dir),
        )
        .await
    }

    pub async fn keyboard_interactive(user: &str, response: &str) -> Self {
        Self::start(
            user,
            TestAuth::KeyboardInteractive(Arc::from(response)),
            None,
        )
        .await
    }

    async fn start(user: &str, auth: TestAuth, identity_dir: Option<tempfile::TempDir>) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let state = Arc::new(TestServerState::default());
        let mut handler = Handler {
            user: Arc::from(user),
            auth: auth.clone(),
            state: state.clone(),
        };
        let key = PrivateKey::from_openssh(TEST_HOST_KEY).unwrap();
        let known_hosts_dir = tempfile::tempdir().unwrap();
        let known_hosts_file = known_hosts_dir.path().join("known_hosts");
        save_host_key(
            &known_hosts_file,
            KnownHostEntry {
                host: address.ip().to_string(),
                port: address.port(),
                key: PresentedHostKey {
                    algorithm: key.public_key().algorithm().as_str().to_owned(),
                    blob: key.public_key().to_bytes().unwrap(),
                },
            },
        )
        .await
        .unwrap();
        let config = Arc::new(server::Config {
            auth_rejection_time: Duration::ZERO,
            auth_rejection_time_initial: Some(Duration::ZERO),
            keys: vec![key],
            ..Default::default()
        });
        let (handle_tx, handle_rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let server = handler.run_on_socket(config, &listener);
            let _ = handle_tx.send(server.handle());
            let _ = server.await;
        });
        let handle = handle_rx.await.unwrap();
        Self {
            address,
            user: user.to_owned(),
            auth,
            identity_dir,
            known_hosts_dir,
            state,
            handle,
        }
    }

    pub fn connect_request(&self, space_id: &str) -> SshConnectRequest {
        SshConnectRequest {
            space_id: space_id.to_owned(),
            config: ResolvedSshConfig {
                profile_id: format!("test-{space_id}"),
                host: self.address.ip().to_string(),
                port: self.address.port(),
                user: self.user.clone(),
                identity_files: Vec::new(),
                auth_order: vec![AuthMethod::Password],
                proxy_command: None,
                proxy_jump: None,
                add_keys_to_agent: AddKeysToAgent::No,
                known_hosts_files: vec![self
                    .known_hosts_dir
                    .path()
                    .join("known_hosts")
                    .to_string_lossy()
                    .into_owned()],
                warnings: Vec::new(),
                proxy_consent_hash: None,
            },
            password: match &self.auth {
                TestAuth::Password(password) => Some(AuthAnswer::new(password.to_string())),
                _ => None,
            },
            proxy_command_approved: false,
        }
    }

    pub fn unknown_connect_request(&self, space_id: &str) -> SshConnectRequest {
        let mut request = self.connect_request(space_id);
        request.config.known_hosts_files = vec![self
            .known_hosts_dir
            .path()
            .join(format!("unknown-{space_id}"))
            .to_string_lossy()
            .into_owned()];
        request
    }

    pub fn private_key_request(&self, space_id: &str) -> SshConnectRequest {
        let mut request = self.connect_request(space_id);
        request.config.auth_order = vec![AuthMethod::PrivateKey];
        request.config.identity_files = vec![self
            .identity_dir
            .as_ref()
            .unwrap()
            .path()
            .join("id_ed25519")
            .to_string_lossy()
            .into_owned()];
        request
    }

    pub fn keyboard_interactive_request(&self, space_id: &str) -> SshConnectRequest {
        let mut request = self.connect_request(space_id);
        request.config.auth_order = vec![AuthMethod::KeyboardInteractive];
        request
    }

    pub fn last_window_size(&self) -> (u32, u32) {
        (
            self.state.columns.load(Ordering::Relaxed),
            self.state.rows.load(Ordering::Relaxed),
        )
    }

    pub fn shell_open_count(&self) -> u32 {
        self.state.shell_open_count.load(Ordering::Relaxed)
    }

    pub fn fail_home_command(&self) {
        self.state.fail_home.store(true, Ordering::Relaxed);
    }

    pub fn burst_output(&self) {
        self.state.burst_output.store(true, Ordering::Relaxed);
    }

    pub async fn drop_transport(&self) {
        self.handle.shutdown("test transport loss".to_owned());
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

impl Drop for TestSshServer {
    fn drop(&mut self) {
        self.handle.shutdown("test complete".to_owned());
    }
}
