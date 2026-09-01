use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use tokio::sync::{mpsc, oneshot, watch, Mutex, RwLock};
use tokio_util::sync::CancellationToken;

use crate::modules::ssh::auth::{
    AuthenticationBackend, AuthenticationDriver, AuthenticationStep, PlatformAgentEndpoint,
    RusshAuthenticationBackend,
};
use crate::modules::ssh::channel::{discover_remote_home, RemotePty};
use crate::modules::ssh::errors::{SshError, SshErrorCode};
use crate::modules::ssh::known_hosts::host_fingerprint;
use crate::modules::ssh::transport::{AuthenticatedTransport, HostTrustBridge};
use crate::modules::ssh::types::{
    AuthChallenge, AuthChallengeKind, AuthResponse, ChallengeId, ChannelId, ChannelPhase,
    ConnectionId, ConnectionPhase, PresentedHostKey, PtyEvent, PtySize, SshChallenge,
    SshConnectRequest, SshConnectionEvent,
};

const MAX_PTY_CHANNELS: usize = 32;

pub type SshEventSink = mpsc::Sender<SshConnectionEvent>;

#[derive(Clone)]
pub struct SshState {
    next_connection_id: Arc<AtomicU64>,
    connections: Arc<RwLock<HashMap<String, Arc<SpaceConnection>>>>,
    attempts: Arc<RwLock<HashMap<ConnectionId, ConnectAttempt>>>,
    connect_lock: Arc<Mutex<()>>,
    shutdown: CancellationToken,
}

impl Default for SshState {
    fn default() -> Self {
        Self {
            next_connection_id: Arc::new(AtomicU64::new(0)),
            connections: Arc::new(RwLock::new(HashMap::new())),
            attempts: Arc::new(RwLock::new(HashMap::new())),
            connect_lock: Arc::new(Mutex::new(())),
            shutdown: CancellationToken::new(),
        }
    }
}

pub struct SpaceConnection {
    pub id: ConnectionId,
    pub profile_id: String,
    pub phase: watch::Sender<ConnectionPhase>,
    pub transport: AuthenticatedTransport,
    pub channels: RwLock<HashMap<ChannelId, Arc<RemotePty>>>,
    pub cancel: CancellationToken,
    next_channel_id: AtomicU32,
}

struct ConnectAttempt {
    space_id: String,
    commands: mpsc::Sender<AttemptCommand>,
    cancel: CancellationToken,
}

enum AttemptCommand {
    Respond {
        challenge_id: ChallengeId,
        response: AuthResponse,
        reply: oneshot::Sender<Result<(), SshError>>,
    },
    Trust {
        challenge_id: ChallengeId,
        trusted: bool,
        reply: oneshot::Sender<Result<(), SshError>>,
    },
    Cancel,
}

impl SshState {
    pub async fn start_connect(
        &self,
        request: SshConnectRequest,
        events: SshEventSink,
    ) -> Result<ConnectionId, SshError> {
        if request.space_id.trim().is_empty() {
            return Err(SshError::new(
                SshErrorCode::ProfileInvalid,
                "connecting",
                "Space",
                "Space id must not be empty",
            ));
        }
        let _connect_guard = self.connect_lock.lock().await;
        if let Some(connection) = self.connections.read().await.get(&request.space_id) {
            return Ok(connection.id);
        }
        if self
            .attempts
            .read()
            .await
            .values()
            .any(|attempt| attempt.space_id == request.space_id)
        {
            return Err(SshError::new(
                SshErrorCode::ProtocolLimitExceeded,
                "connecting",
                &request.space_id,
                "a connection attempt is already active for this Space",
            ));
        }
        let connection_id = self.allocate_connection_id()?;
        let cancel = self.shutdown.child_token();
        let (commands, command_rx) = mpsc::channel(1);
        self.attempts.write().await.insert(
            connection_id,
            ConnectAttempt {
                space_id: request.space_id.clone(),
                commands,
                cancel: cancel.clone(),
            },
        );
        let state = self.clone();
        tokio::spawn(async move {
            let space_id = request.space_id.clone();
            if let Err(error) = state
                .run_connect(connection_id, request, events.clone(), command_rx, cancel)
                .await
            {
                let _ = emit_event(
                    &events,
                    SshConnectionEvent::Error {
                        connection_id: Some(connection_id),
                        space_id,
                        error,
                    },
                );
            }
            state.attempts.write().await.remove(&connection_id);
        });
        Ok(connection_id)
    }

    pub async fn respond_challenge(
        &self,
        connection_id: ConnectionId,
        challenge_id: ChallengeId,
        response: AuthResponse,
    ) -> Result<(), SshError> {
        if response.challenge_id != challenge_id {
            return Err(challenge_cancelled(connection_id));
        }
        let commands = self
            .attempts
            .read()
            .await
            .get(&connection_id)
            .map(|attempt| attempt.commands.clone())
            .ok_or_else(|| missing_connection(connection_id))?;
        let (reply, result) = oneshot::channel();
        commands
            .send(AttemptCommand::Respond {
                challenge_id,
                response,
                reply,
            })
            .await
            .map_err(|_| missing_connection(connection_id))?;
        result
            .await
            .map_err(|_| missing_connection(connection_id))?
    }

    pub async fn respond_trust(
        &self,
        connection_id: ConnectionId,
        challenge_id: ChallengeId,
        trusted: bool,
    ) -> Result<(), SshError> {
        let commands = self
            .attempts
            .read()
            .await
            .get(&connection_id)
            .map(|attempt| attempt.commands.clone())
            .ok_or_else(|| missing_connection(connection_id))?;
        let (reply, result) = oneshot::channel();
        commands
            .send(AttemptCommand::Trust {
                challenge_id,
                trusted,
                reply,
            })
            .await
            .map_err(|_| missing_connection(connection_id))?;
        result
            .await
            .map_err(|_| missing_connection(connection_id))?
    }

    pub async fn cancel_connect(&self, connection_id: ConnectionId) -> Result<(), SshError> {
        let attempt = self.attempts.read().await;
        let attempt = attempt
            .get(&connection_id)
            .ok_or_else(|| missing_connection(connection_id))?;
        attempt.cancel.cancel();
        let _ = attempt.commands.try_send(AttemptCommand::Cancel);
        Ok(())
    }

    pub async fn connect(&self, mut request: SshConnectRequest) -> Result<ConnectionId, SshError> {
        let mut password = request.password.take();
        let (events, mut event_rx) = mpsc::channel(16);
        let connection_id = self.start_connect(request, events).await?;
        while let Some(event) = event_rx.recv().await {
            match event {
                SshConnectionEvent::Challenge {
                    challenge: SshChallenge::Password { challenge_id, .. },
                } => {
                    let Some(password) = password.take() else {
                        self.cancel_connect(connection_id).await?;
                        return Err(SshError::new(
                            SshErrorCode::ChallengeCancelled,
                            "authenticating",
                            connection_id.to_string(),
                            "password response was not provided",
                        ));
                    };
                    self.respond_challenge(
                        connection_id,
                        challenge_id,
                        AuthResponse {
                            challenge_id,
                            answers: vec![password],
                            remember: false,
                        },
                    )
                    .await?;
                }
                SshConnectionEvent::Ready { .. } => return Ok(connection_id),
                SshConnectionEvent::Error { error, .. } => return Err(error),
                _ => {}
            }
        }
        Err(missing_connection(connection_id))
    }

    async fn run_connect(
        &self,
        connection_id: ConnectionId,
        request: SshConnectRequest,
        events: SshEventSink,
        mut commands: mpsc::Receiver<AttemptCommand>,
        cancel: CancellationToken,
    ) -> Result<(), SshError> {
        emit_phase(
            &events,
            connection_id,
            &request.space_id,
            ConnectionPhase::Connecting,
        )?;
        let (presented_tx, mut presented_rx) = mpsc::channel(1);
        let (decision_tx, decision_rx) = mpsc::channel(1);
        let trust_bridge = (!request.trust_unknown_host).then_some(HostTrustBridge {
            presented: presented_tx,
            decisions: Arc::new(Mutex::new(decision_rx)),
        });
        let connect = AuthenticatedTransport::connect_unauthenticated(
            &request.config,
            request.trust_unknown_host,
            request.proxy_command_approved,
            trust_bridge,
        );
        tokio::pin!(connect);
        let mut trust_prompted = false;
        let mut transport = loop {
            tokio::select! {
                result = &mut connect => break result?,
                () = cancel.cancelled() => return Err(challenge_cancelled(connection_id)),
                presented = presented_rx.recv(), if !request.trust_unknown_host && !trust_prompted => {
                    let presented = presented.ok_or_else(|| challenge_cancelled(connection_id))?;
                    trust_prompted = true;
                    emit_host_challenge(
                        &events,
                        connection_id,
                        &request,
                        &presented,
                    )?;
                    let command = tokio::select! {
                        () = cancel.cancelled() => return Err(challenge_cancelled(connection_id)),
                        command = commands.recv() => command.ok_or_else(|| challenge_cancelled(connection_id))?,
                    };
                    match command {
                        AttemptCommand::Trust { challenge_id: 0, trusted, reply } => {
                            decision_tx.send(trusted).await.map_err(|_| challenge_cancelled(connection_id))?;
                            let _ = reply.send(Ok(()));
                        }
                        AttemptCommand::Trust { reply, .. } | AttemptCommand::Respond { reply, .. } => {
                            let error = challenge_cancelled(connection_id);
                            let _ = reply.send(Err(error.clone()));
                            let _ = decision_tx.send(false).await;
                            return Err(error);
                        }
                        AttemptCommand::Cancel => return Err(challenge_cancelled(connection_id)),
                    }
                }
            }
        };
        emit_phase(
            &events,
            connection_id,
            &request.space_id,
            ConnectionPhase::Authenticating,
        )?;
        let mut driver = AuthenticationDriver::new(connection_id, request.config.clone());
        {
            let mut backend =
                RusshAuthenticationBackend::new(transport.handle_mut(), PlatformAgentEndpoint);
            drive_authentication(
                &mut driver,
                &mut backend,
                connection_id,
                commands,
                &events,
                &request.space_id,
                cancel.clone(),
            )
            .await?;
        }
        let (phase, _) = watch::channel(ConnectionPhase::Ready);
        let connection = Arc::new(SpaceConnection {
            id: connection_id,
            profile_id: request.config.profile_id.clone(),
            phase,
            transport,
            channels: RwLock::new(HashMap::new()),
            cancel,
            next_channel_id: AtomicU32::new(1),
        });
        let cancellation_connection = connection.clone();
        let cancellation = connection.cancel.clone();
        tokio::spawn(async move {
            cancellation.cancelled().await;
            cancellation_connection.transport.disconnect().await;
        });
        self.connections
            .write()
            .await
            .insert(request.space_id.clone(), connection);
        emit_event(
            &events,
            SshConnectionEvent::Ready {
                connection_id,
                space_id: request.space_id,
                remote_home: None,
            },
        )
    }

    fn allocate_connection_id(&self) -> Result<ConnectionId, SshError> {
        self.next_connection_id
            .fetch_add(1, Ordering::Relaxed)
            .checked_add(1)
            .ok_or_else(|| identifier_limit("connection"))
    }

    pub async fn disconnect_space(&self, connection_id: ConnectionId) -> Result<(), SshError> {
        let (space_id, connection) = self.connection_entry(connection_id).await?;
        connection
            .phase
            .send_replace(ConnectionPhase::Disconnecting);
        connection.cancel.cancel();
        let channels = {
            let mut channels = connection.channels.write().await;
            channels
                .drain()
                .map(|(_, channel)| channel)
                .collect::<Vec<_>>()
        };
        for channel in channels {
            let _ = channel.close().await;
        }
        connection.transport.disconnect().await;
        self.connections.write().await.remove(&space_id);
        Ok(())
    }

    pub async fn open_pty(
        &self,
        connection_id: ConnectionId,
        size: PtySize,
    ) -> Result<ChannelId, SshError> {
        let connection = self.connection(connection_id).await?;
        if *connection.phase.borrow() != ConnectionPhase::Ready {
            return Err(transport_lost("connection is not ready"));
        }
        let mut channels = connection.channels.write().await;
        if channels.len() >= MAX_PTY_CHANNELS {
            return Err(SshError::new(
                SshErrorCode::ChannelLimitReached,
                "opening PTY channel",
                &connection.profile_id,
                "a Space may own at most 32 live PTY channels",
            ));
        }
        let channel_id = connection.next_channel_id.fetch_add(1, Ordering::Relaxed);
        if channel_id == 0 {
            return Err(identifier_limit("channel"));
        }
        let channel = Arc::new(RemotePty::open(&connection.transport, size).await?);
        channels.insert(channel_id, channel);
        Ok(channel_id)
    }

    pub async fn write_pty(
        &self,
        connection_id: ConnectionId,
        channel_id: ChannelId,
        data: &[u8],
    ) -> Result<(), SshError> {
        self.channel(connection_id, channel_id)
            .await?
            .write(data)
            .await
    }

    pub async fn resize_pty(
        &self,
        connection_id: ConnectionId,
        channel_id: ChannelId,
        size: PtySize,
    ) -> Result<(), SshError> {
        self.channel(connection_id, channel_id)
            .await?
            .resize(size)
            .await
    }

    pub async fn read_pty(
        &self,
        connection_id: ConnectionId,
        channel_id: ChannelId,
    ) -> Result<Option<PtyEvent>, SshError> {
        Ok(self
            .channel(connection_id, channel_id)
            .await?
            .next_event()
            .await)
    }

    pub async fn close_pty(
        &self,
        connection_id: ConnectionId,
        channel_id: ChannelId,
    ) -> Result<(), SshError> {
        let connection = self.connection(connection_id).await?;
        let channel = connection
            .channels
            .write()
            .await
            .remove(&channel_id)
            .ok_or_else(|| missing_channel(channel_id))?;
        channel.close().await
    }

    pub async fn discover_remote_home(
        &self,
        connection_id: ConnectionId,
    ) -> Result<Option<String>, SshError> {
        let connection = self.connection(connection_id).await?;
        if *connection.phase.borrow() != ConnectionPhase::Ready {
            return Err(transport_lost("connection is not ready"));
        }
        discover_remote_home(&connection.transport).await
    }

    pub async fn phase(&self, connection_id: ConnectionId) -> Option<ConnectionPhase> {
        self.connection(connection_id)
            .await
            .ok()
            .map(|connection| *connection.phase.borrow())
    }

    pub async fn channel_phase(
        &self,
        connection_id: ConnectionId,
        channel_id: ChannelId,
    ) -> Option<ChannelPhase> {
        let connection = self.connection(connection_id).await.ok()?;
        let channel = connection.channels.read().await.get(&channel_id).cloned()?;
        let phase = channel.phase();
        if phase == ChannelPhase::Lost {
            connection.phase.send_replace(ConnectionPhase::Lost);
        }
        Some(phase)
    }

    pub async fn connection_count(&self) -> usize {
        self.connections.read().await.len()
    }

    pub async fn channel_count(&self, connection_id: ConnectionId) -> usize {
        match self.connection(connection_id).await {
            Ok(connection) => connection.channels.read().await.len(),
            Err(_) => 0,
        }
    }

    pub fn close_all(&self) {
        self.shutdown.cancel();
        if let Ok(mut connections) = self.connections.try_write() {
            connections.clear();
        }
    }

    async fn connection(
        &self,
        connection_id: ConnectionId,
    ) -> Result<Arc<SpaceConnection>, SshError> {
        self.connections
            .read()
            .await
            .values()
            .find(|connection| connection.id == connection_id)
            .cloned()
            .ok_or_else(|| missing_connection(connection_id))
    }

    async fn connection_entry(
        &self,
        connection_id: ConnectionId,
    ) -> Result<(String, Arc<SpaceConnection>), SshError> {
        self.connections
            .read()
            .await
            .iter()
            .find(|(_, connection)| connection.id == connection_id)
            .map(|(space_id, connection)| (space_id.clone(), connection.clone()))
            .ok_or_else(|| missing_connection(connection_id))
    }

    async fn channel(
        &self,
        connection_id: ConnectionId,
        channel_id: ChannelId,
    ) -> Result<Arc<RemotePty>, SshError> {
        self.connection(connection_id)
            .await?
            .channels
            .read()
            .await
            .get(&channel_id)
            .cloned()
            .ok_or_else(|| missing_channel(channel_id))
    }
}

async fn drive_authentication<B: AuthenticationBackend>(
    driver: &mut AuthenticationDriver,
    backend: &mut B,
    connection_id: ConnectionId,
    mut commands: mpsc::Receiver<AttemptCommand>,
    events: &SshEventSink,
    space_id: &str,
    cancel: CancellationToken,
) -> Result<(), SshError> {
    let mut step = tokio::select! {
        result = driver.start(backend) => result?,
        () = cancel.cancelled() => return Err(challenge_cancelled(connection_id)),
    };
    loop {
        match step {
            AuthenticationStep::Success => return Ok(()),
            AuthenticationStep::Rejected => {
                return Err(SshError::new(
                    SshErrorCode::AuthenticationRejected,
                    "authenticating",
                    space_id,
                    "server rejected authentication",
                ));
            }
            AuthenticationStep::Challenge(challenge) => {
                emit_phase(
                    events,
                    challenge.connection_id,
                    space_id,
                    ConnectionPhase::AwaitingAuthResponse,
                )?;
                emit_event(
                    events,
                    SshConnectionEvent::Challenge {
                        challenge: auth_challenge_event(challenge.clone()),
                    },
                )?;
                let command = tokio::select! {
                    () = cancel.cancelled() => return Err(challenge_cancelled(challenge.connection_id)),
                    command = commands.recv() => command.ok_or_else(|| challenge_cancelled(challenge.connection_id))?,
                };
                match command {
                    AttemptCommand::Respond {
                        challenge_id,
                        response,
                        reply,
                    } => {
                        let result = tokio::select! {
                            result = driver.respond(
                                backend,
                                challenge_id,
                                response.answers,
                                response.remember,
                            ) => result,
                            () = cancel.cancelled() => {
                                return Err(challenge_cancelled(challenge.connection_id));
                            }
                        };
                        match result {
                            Ok(next) => {
                                let _ = reply.send(Ok(()));
                                step = next;
                            }
                            Err(error) => {
                                let _ = reply.send(Err(error.clone()));
                                return Err(error);
                            }
                        }
                    }
                    AttemptCommand::Trust { reply, .. } => {
                        let error = challenge_cancelled(challenge.connection_id);
                        let _ = reply.send(Err(error.clone()));
                        return Err(error);
                    }
                    AttemptCommand::Cancel => {
                        return Err(challenge_cancelled(challenge.connection_id))
                    }
                }
            }
        }
    }
}

fn emit_host_challenge(
    events: &SshEventSink,
    connection_id: ConnectionId,
    request: &SshConnectRequest,
    presented: &PresentedHostKey,
) -> Result<(), SshError> {
    emit_phase(
        events,
        connection_id,
        &request.space_id,
        ConnectionPhase::AwaitingTrust,
    )?;
    emit_event(
        events,
        SshConnectionEvent::Challenge {
            challenge: SshChallenge::UnknownHost {
                connection_id,
                challenge_id: 0,
                host: request.config.host.clone(),
                port: request.config.port,
                algorithm: presented.algorithm.clone(),
                fingerprint: host_fingerprint(&presented.blob),
                known_hosts_file: request.config.known_hosts_files.first().cloned(),
            },
        },
    )
}

fn auth_challenge_event(challenge: AuthChallenge) -> SshChallenge {
    match challenge.challenge {
        AuthChallengeKind::Password { prompt } => SshChallenge::Password {
            connection_id: challenge.connection_id,
            challenge_id: challenge.id,
            prompt,
        },
        AuthChallengeKind::PrivateKeyPassphrase {
            identity_file,
            prompt,
        } => SshChallenge::PrivateKeyPassphrase {
            connection_id: challenge.connection_id,
            challenge_id: challenge.id,
            identity_file,
            prompt,
        },
        AuthChallengeKind::KeyboardInteractive {
            name,
            instruction,
            prompts,
        } => SshChallenge::KeyboardInteractive {
            connection_id: challenge.connection_id,
            challenge_id: challenge.id,
            name,
            instruction,
            prompts,
        },
    }
}

fn emit_phase(
    events: &SshEventSink,
    connection_id: ConnectionId,
    space_id: &str,
    phase: ConnectionPhase,
) -> Result<(), SshError> {
    emit_event(
        events,
        SshConnectionEvent::PhaseChanged {
            connection_id,
            space_id: space_id.to_owned(),
            phase,
        },
    )
}

fn emit_event(events: &SshEventSink, event: SshConnectionEvent) -> Result<(), SshError> {
    events.try_send(event).map_err(|_| {
        SshError::new(
            SshErrorCode::ProtocolLimitExceeded,
            "streaming connection events",
            "SSH connection",
            "connection event consumer exceeded the bounded queue",
        )
    })
}

fn challenge_cancelled(connection_id: ConnectionId) -> SshError {
    SshError::new(
        SshErrorCode::ChallengeCancelled,
        "authenticating",
        connection_id.to_string(),
        "connection attempt was cancelled",
    )
}

fn missing_connection(connection_id: ConnectionId) -> SshError {
    SshError::new(
        SshErrorCode::TransportLost,
        "using connection",
        connection_id.to_string(),
        "SSH connection does not exist",
    )
}

fn missing_channel(channel_id: ChannelId) -> SshError {
    SshError::new(
        SshErrorCode::TransportLost,
        "using PTY channel",
        channel_id.to_string(),
        "SSH channel does not exist",
    )
}

fn identifier_limit(kind: &str) -> SshError {
    SshError::new(
        SshErrorCode::ProtocolLimitExceeded,
        "allocating SSH state",
        kind,
        format!("{kind} identifier limit reached"),
    )
}

fn transport_lost(message: &str) -> SshError {
    SshError::new(
        SshErrorCode::TransportLost,
        "using connection",
        "SSH transport",
        message,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::ssh::auth::{AuthOutcome, KeyboardInteractiveStep};
    use crate::modules::ssh::test_server::TestSshServer;
    use futures_util::future::BoxFuture;
    use std::path::Path;

    #[tokio::test]
    async fn private_key_authentication_reaches_ready_through_manager() {
        let server = TestSshServer::private_key("user").await;
        let state = SshState::default();
        let connection = state
            .connect(server.private_key_request("space-key"))
            .await
            .unwrap();
        assert_eq!(state.phase(connection).await, Some(ConnectionPhase::Ready));
    }

    #[tokio::test]
    async fn keyboard_interactive_response_reaches_ready_through_manager() {
        let server = TestSshServer::keyboard_interactive("user", "123456").await;
        let state = SshState::default();
        let (events, mut event_rx) = mpsc::channel(16);
        let connection = state
            .start_connect(server.keyboard_interactive_request("space-kbi"), events)
            .await
            .unwrap();
        let challenge_id = loop {
            if let SshConnectionEvent::Challenge {
                challenge: SshChallenge::KeyboardInteractive { challenge_id, .. },
            } = event_rx.recv().await.unwrap()
            {
                break challenge_id;
            }
        };
        state
            .respond_challenge(
                connection,
                challenge_id,
                AuthResponse {
                    challenge_id,
                    answers: vec![crate::modules::ssh::AuthAnswer::new("123456")],
                    remember: false,
                },
            )
            .await
            .unwrap();
        loop {
            if matches!(
                event_rx.recv().await.unwrap(),
                SshConnectionEvent::Ready { .. }
            ) {
                break;
            }
        }
        assert_eq!(state.phase(connection).await, Some(ConnectionPhase::Ready));
    }

    #[tokio::test]
    async fn stale_challenge_is_rejected_and_connect_can_be_cancelled() {
        let server = TestSshServer::keyboard_interactive("user", "123456").await;
        let state = SshState::default();
        let (events, mut event_rx) = mpsc::channel(16);
        let connection = state
            .start_connect(server.keyboard_interactive_request("space-kbi"), events)
            .await
            .unwrap();
        let challenge_id = loop {
            if let SshConnectionEvent::Challenge {
                challenge: SshChallenge::KeyboardInteractive { challenge_id, .. },
            } = event_rx.recv().await.unwrap()
            {
                break challenge_id;
            }
        };
        let stale = challenge_id + 1;
        assert_eq!(
            state
                .respond_challenge(
                    connection,
                    stale,
                    AuthResponse {
                        challenge_id: stale,
                        answers: vec![crate::modules::ssh::AuthAnswer::new("123456")],
                        remember: false,
                    },
                )
                .await
                .unwrap_err()
                .code,
            SshErrorCode::ChallengeCancelled
        );

        let (events, mut event_rx) = mpsc::channel(16);
        let connection = state
            .start_connect(server.keyboard_interactive_request("space-cancel"), events)
            .await
            .unwrap();
        while !matches!(
            event_rx.recv().await.unwrap(),
            SshConnectionEvent::Challenge { .. }
        ) {}
        state.cancel_connect(connection).await.unwrap();
        loop {
            if let SshConnectionEvent::Error { error, .. } = event_rx.recv().await.unwrap() {
                assert_eq!(error.code, SshErrorCode::ChallengeCancelled);
                break;
            }
        }
    }

    struct AgentSuccessBackend;

    impl AuthenticationBackend for AgentSuccessBackend {
        fn authenticate_agent<'a>(
            &'a mut self,
            _user: &'a str,
        ) -> BoxFuture<'a, Result<AuthOutcome, SshError>> {
            Box::pin(async { Ok(AuthOutcome::Success) })
        }

        fn private_key_requires_passphrase(&self, _identity_file: &Path) -> Result<bool, SshError> {
            Ok(false)
        }

        fn authenticate_private_key<'a>(
            &'a mut self,
            _user: &'a str,
            _identity_file: &'a Path,
            _passphrase: Option<&'a crate::modules::ssh::AuthAnswer>,
        ) -> BoxFuture<'a, Result<AuthOutcome, SshError>> {
            Box::pin(async { Ok(AuthOutcome::Rejected) })
        }

        fn authenticate_password<'a>(
            &'a mut self,
            _user: &'a str,
            _response: &'a AuthResponse,
        ) -> BoxFuture<'a, Result<AuthOutcome, SshError>> {
            Box::pin(async { Ok(AuthOutcome::Rejected) })
        }

        fn start_keyboard_interactive<'a>(
            &'a mut self,
            _user: &'a str,
            _broker: &'a mut crate::modules::ssh::AuthBroker,
        ) -> BoxFuture<'a, Result<KeyboardInteractiveStep, SshError>> {
            Box::pin(async { Ok(KeyboardInteractiveStep::Rejected) })
        }

        fn continue_keyboard_interactive<'a>(
            &'a mut self,
            _broker: &'a mut crate::modules::ssh::AuthBroker,
            _response: AuthResponse,
        ) -> BoxFuture<'a, Result<KeyboardInteractiveStep, SshError>> {
            Box::pin(async { Ok(KeyboardInteractiveStep::Rejected) })
        }
    }

    #[tokio::test]
    async fn manager_authentication_loop_reaches_injected_agent_adapter() {
        let server = TestSshServer::password("user", "unused").await;
        let mut config = server.connect_request("space-agent").config;
        config.auth_order = vec![crate::modules::ssh::AuthMethod::Agent];
        let mut driver = AuthenticationDriver::new(1, config);
        let mut backend = AgentSuccessBackend;
        let (_commands, command_rx) = mpsc::channel(1);
        let (events, _event_rx) = mpsc::channel(1);
        drive_authentication(
            &mut driver,
            &mut backend,
            1,
            command_rx,
            &events,
            "space-agent",
            CancellationToken::new(),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn one_space_owns_one_transport_and_multiple_channels() {
        let server = TestSshServer::password("user", "secret").await;
        let state = SshState::default();
        let connection = state
            .connect(server.connect_request("space-a"))
            .await
            .unwrap();
        let one = state
            .open_pty(connection, PtySize::new(80, 24))
            .await
            .unwrap();
        let two = state
            .open_pty(connection, PtySize::new(100, 40))
            .await
            .unwrap();
        assert_ne!(one, two);
        assert_eq!(state.connection_count().await, 1);
        assert_eq!(state.channel_count(connection).await, 2);
    }

    #[tokio::test]
    async fn thirty_third_channel_is_rejected() {
        let server = TestSshServer::password("user", "secret").await;
        let state = SshState::default();
        let connection = state
            .connect(server.connect_request("space-a"))
            .await
            .unwrap();
        for _ in 0..32 {
            state
                .open_pty(connection, PtySize::new(80, 24))
                .await
                .unwrap();
        }
        assert_eq!(
            state
                .open_pty(connection, PtySize::new(80, 24))
                .await
                .unwrap_err()
                .code,
            SshErrorCode::ChannelLimitReached
        );
    }

    #[tokio::test]
    async fn unconsumed_pty_output_closes_at_the_bounded_queue() {
        let server = TestSshServer::password("user", "secret").await;
        server.burst_output();
        let state = SshState::default();
        let connection = state
            .connect(server.connect_request("space-burst"))
            .await
            .unwrap();
        let channel = state
            .open_pty(connection, PtySize::new(80, 24))
            .await
            .unwrap();
        for _ in 0..20 {
            if state.channel_phase(connection, channel).await == Some(ChannelPhase::Lost) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert_eq!(
            state.channel_phase(connection, channel).await,
            Some(ChannelPhase::Lost)
        );
        loop {
            if let Some(PtyEvent::Error(error)) = state.read_pty(connection, channel).await.unwrap()
            {
                assert_eq!(error.code, SshErrorCode::ProtocolLimitExceeded);
                break;
            }
        }
    }

    #[tokio::test]
    async fn transport_loss_marks_live_channels_lost_without_reopening() {
        let server = TestSshServer::password("user", "secret").await;
        let state = SshState::default();
        let connection = state
            .connect(server.connect_request("space-a"))
            .await
            .unwrap();
        let channel = state
            .open_pty(connection, PtySize::new(80, 24))
            .await
            .unwrap();
        for _ in 0..20 {
            if server.shell_open_count() == 1 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        server.drop_transport().await;
        for _ in 0..20 {
            if state.channel_phase(connection, channel).await == Some(ChannelPhase::Lost) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert_eq!(
            state.channel_phase(connection, channel).await,
            Some(ChannelPhase::Lost)
        );
        assert_eq!(state.channel_count(connection).await, 1);
        assert_eq!(server.shell_open_count(), 1);
    }

    #[tokio::test]
    async fn unknown_host_requires_explicit_trust() {
        let server = TestSshServer::password("user", "secret").await;
        let state = SshState::default();
        let mut request = server.connect_request("space-a");
        request.trust_unknown_host = false;
        let (events, mut event_rx) = mpsc::channel(16);
        let connection = state.start_connect(request, events).await.unwrap();
        loop {
            if let SshConnectionEvent::Challenge {
                challenge: SshChallenge::UnknownHost { challenge_id, .. },
            } = event_rx.recv().await.unwrap()
            {
                state
                    .respond_trust(connection, challenge_id, false)
                    .await
                    .unwrap();
                break;
            }
        }
        loop {
            if let SshConnectionEvent::Error { error, .. } = event_rx.recv().await.unwrap() {
                assert_eq!(error.code, SshErrorCode::HostUnknown);
                break;
            }
        }
    }

    #[tokio::test]
    async fn explicit_host_trust_allows_authentication_to_continue() {
        let server = TestSshServer::password("user", "secret").await;
        let state = SshState::default();
        let mut request = server.connect_request("space-trust");
        request.trust_unknown_host = false;
        request.password = None;
        let (events, mut event_rx) = mpsc::channel(16);
        let connection = state.start_connect(request, events).await.unwrap();
        loop {
            match event_rx.recv().await.unwrap() {
                SshConnectionEvent::Challenge {
                    challenge: SshChallenge::UnknownHost { challenge_id, .. },
                } => state
                    .respond_trust(connection, challenge_id, true)
                    .await
                    .unwrap(),
                SshConnectionEvent::Challenge {
                    challenge: SshChallenge::Password { challenge_id, .. },
                } => state
                    .respond_challenge(
                        connection,
                        challenge_id,
                        AuthResponse {
                            challenge_id,
                            answers: vec![crate::modules::ssh::AuthAnswer::new("secret")],
                            remember: false,
                        },
                    )
                    .await
                    .unwrap(),
                SshConnectionEvent::Ready { .. } => break,
                _ => {}
            }
        }
        assert_eq!(state.phase(connection).await, Some(ConnectionPhase::Ready));
    }

    #[tokio::test]
    async fn pty_round_trips_bytes_resize_and_exit() {
        let server = TestSshServer::password("user", "secret").await;
        let state = SshState::default();
        let connection = state
            .connect(server.connect_request("space-a"))
            .await
            .unwrap();
        let channel = state
            .open_pty(connection, PtySize::new(80, 24))
            .await
            .unwrap();
        state
            .write_pty(connection, channel, b"printf terax-test\r")
            .await
            .unwrap();
        let event = state.read_pty(connection, channel).await.unwrap().unwrap();
        assert!(
            matches!(event, PtyEvent::Output(data) if data.windows(10).any(|part| part == b"terax-test"))
        );
        state
            .resize_pty(connection, channel, PtySize::new(132, 50))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert_eq!(server.last_window_size(), (132, 50));
        state.close_pty(connection, channel).await.unwrap();
    }

    #[tokio::test]
    async fn remote_home_failure_keeps_connection_ready() {
        let server = TestSshServer::password("user", "secret").await;
        server.fail_home_command();
        let state = SshState::default();
        let connection = state
            .connect(server.connect_request("space-a"))
            .await
            .unwrap();
        assert_eq!(state.discover_remote_home(connection).await.unwrap(), None);
        assert_eq!(state.phase(connection).await, Some(ConnectionPhase::Ready));
    }
}
