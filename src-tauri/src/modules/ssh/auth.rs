use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;
use std::sync::Arc;

use futures_util::future::LocalBoxFuture;
use russh::client::{AuthResult, Handle, KeyboardInteractiveAuthResponse};
use russh::keys::agent::{client::AgentClient, AgentIdentity};
use russh::keys::{decode_secret_key, Error as KeyError, PrivateKeyWithHashAlg};
use zeroize::Zeroizing;

use super::errors::{SshError, SshErrorCode};
use super::types::{
    AuthAnswer, AuthChallenge, AuthChallengeKind, AuthMethod, AuthResponse, ChallengeId,
    ConnectionId, SshPrompt,
};

const MAX_PROMPTS: usize = 16;
const MAX_SERVER_TEXT_BYTES: usize = 8 * 1024;
const MAX_RESPONSE_BYTES: usize = 64 * 1024;

pub const SSH_PASSWORD_SERVICE: &str = "terax-ai";

pub fn password_account(profile_id: &str) -> String {
    format!("ssh:{profile_id}:password")
}

pub fn auth_order(explicit: Option<&[AuthMethod]>) -> Vec<AuthMethod> {
    explicit.map_or_else(
        || {
            vec![
                AuthMethod::Agent,
                AuthMethod::PrivateKey,
                AuthMethod::Password,
                AuthMethod::KeyboardInteractive,
            ]
        },
        <[AuthMethod]>::to_vec,
    )
}

pub fn configured_auth_order(config: &super::types::ResolvedSshConfig) -> Vec<AuthMethod> {
    auth_order(Some(&config.auth_order))
}

pub async fn remembered_password(
    app: &tauri::AppHandle,
    state: &crate::modules::secrets::SecretsState,
    profile_id: &str,
) -> Result<Option<AuthAnswer>, String> {
    crate::modules::secrets::get_value(
        app,
        state,
        SSH_PASSWORD_SERVICE,
        &password_account(profile_id),
    )
    .await
    .map(|value| value.map(AuthAnswer::new))
}

pub async fn remember_password(
    app: &tauri::AppHandle,
    state: &crate::modules::secrets::SecretsState,
    profile_id: &str,
    response: &AuthResponse,
) -> Result<(), String> {
    if !response.remember {
        return Ok(());
    }
    let password = single_answer(response).map_err(|error| error.to_string())?;
    crate::modules::secrets::set_value(
        app,
        state,
        SSH_PASSWORD_SERVICE,
        &password_account(profile_id),
        password,
    )
    .await
}

pub async fn forget_password(
    app: &tauri::AppHandle,
    state: &crate::modules::secrets::SecretsState,
    profile_id: &str,
) -> Result<(), String> {
    crate::modules::secrets::delete_value(
        app,
        state,
        SSH_PASSWORD_SERVICE,
        &password_account(profile_id),
    )
    .await
}

#[derive(Debug)]
pub struct AuthBroker {
    connection_id: ConnectionId,
    next_id: ChallengeId,
    pending: Option<PendingChallenge>,
}

#[derive(Debug)]
struct PendingChallenge {
    challenge: AuthChallenge,
    answer_count: usize,
}

impl AuthBroker {
    pub fn new(connection_id: ConnectionId) -> Self {
        Self {
            connection_id,
            next_id: 1,
            pending: None,
        }
    }

    pub fn pending(&self) -> Option<&AuthChallenge> {
        self.pending.as_ref().map(|pending| &pending.challenge)
    }

    pub fn begin_password(&mut self) -> Result<AuthChallenge, SshError> {
        self.begin(
            AuthChallengeKind::Password {
                prompt: "Password:".to_owned(),
            },
            1,
        )
    }

    pub fn begin_private_key_passphrase(
        &mut self,
        identity_file: impl Into<String>,
    ) -> Result<AuthChallenge, SshError> {
        self.begin(
            AuthChallengeKind::PrivateKeyPassphrase {
                identity_file: identity_file.into(),
                prompt: "Private key passphrase:".to_owned(),
            },
            1,
        )
    }

    pub fn begin_keyboard_interactive(
        &mut self,
        prompts: Vec<SshPrompt>,
    ) -> Result<AuthChallenge, SshError> {
        self.begin_keyboard_interactive_round(String::new(), String::new(), prompts)
    }

    pub fn begin_keyboard_interactive_round(
        &mut self,
        name: String,
        instruction: String,
        prompts: Vec<SshPrompt>,
    ) -> Result<AuthChallenge, SshError> {
        if prompts.len() > MAX_PROMPTS
            || prompts
                .iter()
                .any(|prompt| prompt.text.len() > MAX_SERVER_TEXT_BYTES)
            || name.len() > MAX_SERVER_TEXT_BYTES
            || instruction.len() > MAX_SERVER_TEXT_BYTES
        {
            self.pending = None;
            return Err(self.error(
                SshErrorCode::ProtocolLimitExceeded,
                "keyboard-interactive request exceeded protocol limits",
            ));
        }
        let answer_count = prompts.len();
        self.begin(
            AuthChallengeKind::KeyboardInteractive {
                name,
                instruction,
                prompts,
            },
            answer_count,
        )
    }

    pub fn respond(
        &mut self,
        challenge_id: ChallengeId,
        answers: Vec<AuthAnswer>,
    ) -> Result<AuthResponse, SshError> {
        self.respond_with_remember(challenge_id, answers, false)
    }

    pub fn respond_with_remember(
        &mut self,
        challenge_id: ChallengeId,
        answers: Vec<AuthAnswer>,
        remember: bool,
    ) -> Result<AuthResponse, SshError> {
        let Some(pending) = self.pending.take() else {
            return Err(self.error(
                SshErrorCode::ChallengeCancelled,
                "authentication challenge is no longer active",
            ));
        };
        if pending.challenge.id != challenge_id {
            return Err(self.error(
                SshErrorCode::ChallengeCancelled,
                "authentication challenge is no longer active",
            ));
        }
        if answers.len() != pending.answer_count
            || answers
                .iter()
                .any(|answer| answer.expose().len() > MAX_RESPONSE_BYTES)
        {
            return Err(self.error(
                SshErrorCode::ProtocolLimitExceeded,
                "authentication response exceeded protocol limits",
            ));
        }
        Ok(AuthResponse {
            challenge_id,
            answers,
            remember,
        })
    }

    pub fn cancel(&mut self, challenge_id: ChallengeId) -> Result<(), SshError> {
        let matches = self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.challenge.id == challenge_id);
        self.pending = None;
        if matches {
            Ok(())
        } else {
            Err(self.error(
                SshErrorCode::ChallengeCancelled,
                "authentication challenge is no longer active",
            ))
        }
    }

    fn begin(
        &mut self,
        challenge: AuthChallengeKind,
        answer_count: usize,
    ) -> Result<AuthChallenge, SshError> {
        if self.pending.is_some() {
            self.pending = None;
            return Err(self.error(
                SshErrorCode::ChallengeCancelled,
                "another authentication challenge was already active",
            ));
        }
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).ok_or_else(|| {
            self.error(
                SshErrorCode::ProtocolLimitExceeded,
                "authentication challenge identifier limit reached",
            )
        })?;
        let challenge = AuthChallenge {
            connection_id: self.connection_id,
            id,
            challenge,
        };
        self.pending = Some(PendingChallenge {
            challenge: challenge.clone(),
            answer_count,
        });
        Ok(challenge)
    }

    fn error(&self, code: SshErrorCode, message: &str) -> SshError {
        SshError::new(
            code,
            "authentication",
            self.connection_id.to_string(),
            message,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentEndpoint {
    #[cfg(unix)]
    UnixSocket(PathBuf),
    #[cfg(windows)]
    NamedPipe(String),
}

pub trait AgentEndpointDiscovery {
    fn endpoint(&self) -> Result<AgentEndpoint, SshError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PlatformAgentEndpoint;

impl AgentEndpointDiscovery for PlatformAgentEndpoint {
    fn endpoint(&self) -> Result<AgentEndpoint, SshError> {
        #[cfg(unix)]
        {
            let path = std::env::var_os("SSH_AUTH_SOCK")
                .filter(|path| !path.is_empty())
                .map(PathBuf::from)
                .ok_or_else(agent_unavailable)?;
            Ok(AgentEndpoint::UnixSocket(path))
        }
        #[cfg(windows)]
        {
            Ok(AgentEndpoint::NamedPipe(
                r"\\.\pipe\openssh-ssh-agent".to_owned(),
            ))
        }
        #[cfg(not(any(unix, windows)))]
        {
            Err(agent_unavailable())
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthOutcome {
    Success,
    Rejected,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthenticationStep {
    Success,
    Rejected,
    Challenge(AuthChallenge),
}

#[derive(Debug)]
enum DriverState {
    NotStarted,
    Advancing,
    Password,
    PrivateKey(String),
    KeyboardInteractive,
    Complete,
}

pub trait AuthenticationBackend {
    fn authenticate_agent<'a>(
        &'a mut self,
        user: &'a str,
    ) -> LocalBoxFuture<'a, Result<AuthOutcome, SshError>>;

    fn private_key_requires_passphrase(&self, identity_file: &Path) -> Result<bool, SshError>;

    fn authenticate_private_key<'a>(
        &'a mut self,
        user: &'a str,
        identity_file: &'a Path,
        passphrase: Option<&'a AuthAnswer>,
    ) -> LocalBoxFuture<'a, Result<AuthOutcome, SshError>>;

    fn authenticate_password<'a>(
        &'a mut self,
        user: &'a str,
        response: &'a AuthResponse,
    ) -> LocalBoxFuture<'a, Result<AuthOutcome, SshError>>;

    fn start_keyboard_interactive<'a>(
        &'a mut self,
        user: &'a str,
        broker: &'a mut AuthBroker,
    ) -> LocalBoxFuture<'a, Result<KeyboardInteractiveStep, SshError>>;

    fn continue_keyboard_interactive<'a>(
        &'a mut self,
        broker: &'a mut AuthBroker,
        response: AuthResponse,
    ) -> LocalBoxFuture<'a, Result<KeyboardInteractiveStep, SshError>>;
}

pub struct AuthenticationDriver {
    config: super::types::ResolvedSshConfig,
    broker: AuthBroker,
    method_index: usize,
    identity_index: usize,
    state: DriverState,
}

impl AuthenticationDriver {
    pub fn new(connection_id: ConnectionId, config: super::types::ResolvedSshConfig) -> Self {
        Self {
            config,
            broker: AuthBroker::new(connection_id),
            method_index: 0,
            identity_index: 0,
            state: DriverState::NotStarted,
        }
    }

    pub fn pending(&self) -> Option<&AuthChallenge> {
        self.broker.pending()
    }

    pub async fn start<B: AuthenticationBackend>(
        &mut self,
        backend: &mut B,
    ) -> Result<AuthenticationStep, SshError> {
        if !matches!(self.state, DriverState::NotStarted) {
            return Err(self.broker.error(
                SshErrorCode::ChallengeCancelled,
                "authentication driver has already started",
            ));
        }
        self.state = DriverState::Advancing;
        let result = self.advance(backend).await;
        if result.is_err() {
            self.terminate();
        }
        result
    }

    pub async fn respond<B: AuthenticationBackend>(
        &mut self,
        backend: &mut B,
        challenge_id: ChallengeId,
        answers: Vec<AuthAnswer>,
        remember: bool,
    ) -> Result<AuthenticationStep, SshError> {
        let state = std::mem::replace(&mut self.state, DriverState::Complete);
        let response = match self
            .broker
            .respond_with_remember(challenge_id, answers, remember)
        {
            Ok(response) => response,
            Err(error) => {
                self.terminate();
                return Err(error);
            }
        };
        self.state = DriverState::Advancing;
        let result = match state {
            DriverState::Password => backend
                .authenticate_password(&self.config.user, &response)
                .await
                .map(AuthenticationResult::Outcome),
            DriverState::PrivateKey(identity_file) => backend
                .authenticate_private_key(
                    &self.config.user,
                    Path::new(&identity_file),
                    response.answers.first(),
                )
                .await
                .map(AuthenticationResult::Outcome),
            DriverState::KeyboardInteractive => backend
                .continue_keyboard_interactive(&mut self.broker, response)
                .await
                .map(AuthenticationResult::KeyboardInteractive),
            DriverState::NotStarted | DriverState::Advancing | DriverState::Complete => {
                Err(self.broker.error(
                    SshErrorCode::ChallengeCancelled,
                    "authentication challenge is no longer active",
                ))
            }
        };
        let result = match result {
            Ok(result) => self.finish_result(backend, result).await,
            Err(error) => Err(error),
        };
        if result.is_err() {
            self.terminate();
        }
        result
    }

    async fn advance<B: AuthenticationBackend>(
        &mut self,
        backend: &mut B,
    ) -> Result<AuthenticationStep, SshError> {
        while let Some(method) = self.config.auth_order.get(self.method_index).copied() {
            match method {
                AuthMethod::Agent => {
                    self.method_index += 1;
                    match backend.authenticate_agent(&self.config.user).await {
                        Ok(AuthOutcome::Success) => return Ok(self.complete(true)),
                        Ok(AuthOutcome::Rejected)
                        | Err(SshError {
                            code: SshErrorCode::AgentUnavailable,
                            ..
                        }) => continue,
                        Err(error) => return Err(error),
                    }
                }
                AuthMethod::PrivateKey => {
                    if let Some(identity) = self.config.identity_files.get(self.identity_index) {
                        self.identity_index += 1;
                        let identity_file = Path::new(identity);
                        match backend.private_key_requires_passphrase(identity_file) {
                            Ok(true) => {
                                let challenge =
                                    self.broker.begin_private_key_passphrase(identity.clone())?;
                                self.state = DriverState::PrivateKey(identity.clone());
                                return Ok(AuthenticationStep::Challenge(challenge));
                            }
                            Ok(false) => match backend
                                .authenticate_private_key(&self.config.user, identity_file, None)
                                .await
                            {
                                Ok(AuthOutcome::Success) => return Ok(self.complete(true)),
                                Ok(AuthOutcome::Rejected) => continue,
                                Err(error) => return Err(error),
                            },
                            Err(error) => return Err(error),
                        }
                    }
                    self.identity_index = 0;
                    self.method_index += 1;
                }
                AuthMethod::Password => {
                    self.method_index += 1;
                    let challenge = self.broker.begin_password()?;
                    self.state = DriverState::Password;
                    return Ok(AuthenticationStep::Challenge(challenge));
                }
                AuthMethod::KeyboardInteractive => {
                    self.method_index += 1;
                    let result = backend
                        .start_keyboard_interactive(&self.config.user, &mut self.broker)
                        .await?;
                    return self
                        .finish_result(backend, AuthenticationResult::KeyboardInteractive(result))
                        .await;
                }
            }
        }
        Ok(self.complete(false))
    }

    async fn finish_result<B: AuthenticationBackend>(
        &mut self,
        backend: &mut B,
        result: AuthenticationResult,
    ) -> Result<AuthenticationStep, SshError> {
        match result {
            AuthenticationResult::Outcome(AuthOutcome::Success)
            | AuthenticationResult::KeyboardInteractive(KeyboardInteractiveStep::Success) => {
                Ok(self.complete(true))
            }
            AuthenticationResult::Outcome(AuthOutcome::Rejected)
            | AuthenticationResult::KeyboardInteractive(KeyboardInteractiveStep::Rejected) => {
                Box::pin(self.advance(backend)).await
            }
            AuthenticationResult::KeyboardInteractive(KeyboardInteractiveStep::Challenge(
                challenge,
            )) => {
                self.state = DriverState::KeyboardInteractive;
                Ok(AuthenticationStep::Challenge(challenge))
            }
        }
    }

    fn complete(&mut self, success: bool) -> AuthenticationStep {
        self.state = DriverState::Complete;
        if success {
            AuthenticationStep::Success
        } else {
            AuthenticationStep::Rejected
        }
    }

    fn terminate(&mut self) {
        self.state = DriverState::Complete;
        self.broker.pending = None;
    }
}

enum AuthenticationResult {
    Outcome(AuthOutcome),
    KeyboardInteractive(KeyboardInteractiveStep),
}

pub trait AgentConnector {
    fn authenticate<'a>(
        &'a mut self,
        endpoint: AgentEndpoint,
        user: &'a str,
    ) -> LocalBoxFuture<'a, Result<AuthOutcome, SshError>>;
}

struct RusshAgentConnector<'a, H: russh::client::Handler> {
    handle: &'a mut Handle<H>,
}

impl<H: russh::client::Handler> AgentConnector for RusshAgentConnector<'_, H> {
    fn authenticate<'a>(
        &'a mut self,
        endpoint: AgentEndpoint,
        user: &'a str,
    ) -> LocalBoxFuture<'a, Result<AuthOutcome, SshError>> {
        Box::pin(async move {
            #[cfg(unix)]
            let mut agent = match endpoint {
                AgentEndpoint::UnixSocket(path) => AgentClient::connect_uds(path)
                    .await
                    .map_err(|_| agent_unavailable())?,
            };
            #[cfg(windows)]
            let mut agent = match endpoint {
                AgentEndpoint::NamedPipe(path) => AgentClient::connect_named_pipe(path)
                    .await
                    .map_err(|_| agent_unavailable())?,
            };
            authenticate_agent_identities(self.handle, user, &mut agent).await
        })
    }
}

pub async fn authenticate_agent<H: russh::client::Handler, D: AgentEndpointDiscovery>(
    handle: &mut Handle<H>,
    user: &str,
    discovery: &D,
) -> Result<AuthOutcome, SshError> {
    let mut connector = RusshAgentConnector { handle };
    authenticate_agent_with_connector(user, discovery, &mut connector).await
}

pub async fn authenticate_agent_with_connector<D: AgentEndpointDiscovery, C: AgentConnector>(
    user: &str,
    discovery: &D,
    connector: &mut C,
) -> Result<AuthOutcome, SshError> {
    connector.authenticate(discovery.endpoint()?, user).await
}

pub struct RusshAuthenticationBackend<'a, H: russh::client::Handler, D> {
    handle: &'a mut Handle<H>,
    agent_discovery: D,
}

impl<'a, H: russh::client::Handler, D> RusshAuthenticationBackend<'a, H, D> {
    pub fn new(handle: &'a mut Handle<H>, agent_discovery: D) -> Self {
        Self {
            handle,
            agent_discovery,
        }
    }
}

impl<H: russh::client::Handler, D: AgentEndpointDiscovery> AuthenticationBackend
    for RusshAuthenticationBackend<'_, H, D>
{
    fn authenticate_agent<'a>(
        &'a mut self,
        user: &'a str,
    ) -> LocalBoxFuture<'a, Result<AuthOutcome, SshError>> {
        Box::pin(authenticate_agent(self.handle, user, &self.agent_discovery))
    }

    fn private_key_requires_passphrase(&self, identity_file: &Path) -> Result<bool, SshError> {
        private_key_requires_passphrase(identity_file)
    }

    fn authenticate_private_key<'a>(
        &'a mut self,
        user: &'a str,
        identity_file: &'a Path,
        passphrase: Option<&'a AuthAnswer>,
    ) -> LocalBoxFuture<'a, Result<AuthOutcome, SshError>> {
        Box::pin(authenticate_private_key(
            self.handle,
            user,
            identity_file,
            passphrase,
        ))
    }

    fn authenticate_password<'a>(
        &'a mut self,
        user: &'a str,
        response: &'a AuthResponse,
    ) -> LocalBoxFuture<'a, Result<AuthOutcome, SshError>> {
        Box::pin(authenticate_password(self.handle, user, response))
    }

    fn start_keyboard_interactive<'a>(
        &'a mut self,
        user: &'a str,
        broker: &'a mut AuthBroker,
    ) -> LocalBoxFuture<'a, Result<KeyboardInteractiveStep, SshError>> {
        Box::pin(start_keyboard_interactive(self.handle, user, broker))
    }

    fn continue_keyboard_interactive<'a>(
        &'a mut self,
        broker: &'a mut AuthBroker,
        response: AuthResponse,
    ) -> LocalBoxFuture<'a, Result<KeyboardInteractiveStep, SshError>> {
        Box::pin(continue_keyboard_interactive(self.handle, broker, response))
    }
}

pub trait AgentClientBehavior {
    type Identity;

    fn request_identities(&mut self) -> LocalBoxFuture<'_, Result<Vec<Self::Identity>, SshError>>;

    fn authenticate_identity<'a>(
        &'a mut self,
        user: &'a str,
        identity: Self::Identity,
    ) -> LocalBoxFuture<'a, Result<AuthOutcome, SshError>>;
}

async fn authenticate_agent_client<C: AgentClientBehavior>(
    client: &mut C,
    user: &str,
) -> Result<AuthOutcome, SshError> {
    for identity in client.request_identities().await? {
        if client.authenticate_identity(user, identity).await? == AuthOutcome::Success {
            return Ok(AuthOutcome::Success);
        }
    }
    Ok(AuthOutcome::Rejected)
}

struct RusshAgentClient<'a, H: russh::client::Handler, S: russh::keys::agent::client::AgentStream> {
    handle: &'a mut Handle<H>,
    agent: &'a mut AgentClient<S>,
}

impl<H, S> AgentClientBehavior for RusshAgentClient<'_, H, S>
where
    H: russh::client::Handler,
    S: russh::keys::agent::client::AgentStream + Send + Unpin,
{
    type Identity = AgentIdentity;

    fn request_identities(&mut self) -> LocalBoxFuture<'_, Result<Vec<Self::Identity>, SshError>> {
        Box::pin(async {
            self.agent
                .request_identities()
                .await
                .map_err(|_| agent_unavailable())
        })
    }

    fn authenticate_identity<'a>(
        &'a mut self,
        user: &'a str,
        identity: Self::Identity,
    ) -> LocalBoxFuture<'a, Result<AuthOutcome, SshError>> {
        Box::pin(async move {
            let hash_alg = self
                .handle
                .best_supported_rsa_hash()
                .await
                .map_err(authentication_failed)?
                .flatten();
            let result = match identity {
                AgentIdentity::PublicKey { key, .. } => {
                    self.handle
                        .authenticate_publickey_with(user, key, hash_alg, self.agent)
                        .await
                }
                AgentIdentity::Certificate { certificate, .. } => {
                    self.handle
                        .authenticate_certificate_with(user, certificate, hash_alg, self.agent)
                        .await
                }
            }
            .map_err(|_| agent_unavailable())?;
            Ok(auth_outcome(result))
        })
    }
}

async fn authenticate_agent_identities<
    H: russh::client::Handler,
    S: russh::keys::agent::client::AgentStream + Send + Unpin,
>(
    handle: &mut Handle<H>,
    user: &str,
    agent: &mut AgentClient<S>,
) -> Result<AuthOutcome, SshError> {
    authenticate_agent_client(&mut RusshAgentClient { handle, agent }, user).await
}

pub fn private_key_requires_passphrase(identity_file: &Path) -> Result<bool, SshError> {
    match read_private_key(identity_file, None) {
        Ok(_) => Ok(false),
        Err(KeyError::KeyIsEncrypted) => Ok(true),
        Err(error) => Err(private_key_error(identity_file, error)),
    }
}

pub async fn authenticate_private_key<H: russh::client::Handler>(
    handle: &mut Handle<H>,
    user: &str,
    identity_file: &Path,
    passphrase: Option<&AuthAnswer>,
) -> Result<AuthOutcome, SshError> {
    let key = read_private_key(identity_file, passphrase.map(AuthAnswer::expose))
        .map_err(|error| private_key_error(identity_file, error))?;
    let hash_alg = handle
        .best_supported_rsa_hash()
        .await
        .map_err(authentication_failed)?
        .flatten();
    let result = handle
        .authenticate_publickey(user, PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg))
        .await
        .map_err(authentication_failed)?;
    Ok(auth_outcome(result))
}

async fn authenticate_password<H: russh::client::Handler>(
    handle: &mut Handle<H>,
    user: &str,
    response: &AuthResponse,
) -> Result<AuthOutcome, SshError> {
    let password = single_answer(response)?;
    let result = handle
        .authenticate_password(user, password)
        .await
        .map_err(authentication_failed)?;
    Ok(auth_outcome(result))
}

async fn start_keyboard_interactive<H: russh::client::Handler>(
    handle: &mut Handle<H>,
    user: &str,
    broker: &mut AuthBroker,
) -> Result<KeyboardInteractiveStep, SshError> {
    let response = handle
        .authenticate_keyboard_interactive_start(user, None::<String>)
        .await
        .map_err(authentication_failed)?;
    keyboard_interactive_step(response, broker)
}

async fn continue_keyboard_interactive<H: russh::client::Handler>(
    handle: &mut Handle<H>,
    broker: &mut AuthBroker,
    response: AuthResponse,
) -> Result<KeyboardInteractiveStep, SshError> {
    let answers = response
        .answers
        .iter()
        .map(|answer| answer.expose().to_owned())
        .collect();
    let response = handle
        .authenticate_keyboard_interactive_respond(answers)
        .await
        .map_err(authentication_failed)?;
    keyboard_interactive_step(response, broker)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyboardInteractiveStep {
    Success,
    Rejected,
    Challenge(AuthChallenge),
}

fn keyboard_interactive_step(
    response: KeyboardInteractiveAuthResponse,
    broker: &mut AuthBroker,
) -> Result<KeyboardInteractiveStep, SshError> {
    match response {
        KeyboardInteractiveAuthResponse::Success => Ok(KeyboardInteractiveStep::Success),
        KeyboardInteractiveAuthResponse::Failure { .. } => Ok(KeyboardInteractiveStep::Rejected),
        KeyboardInteractiveAuthResponse::InfoRequest {
            name,
            instructions,
            prompts,
        } => broker
            .begin_keyboard_interactive_round(
                name,
                instructions,
                prompts
                    .into_iter()
                    .map(|prompt| SshPrompt {
                        text: prompt.prompt,
                        echo: prompt.echo,
                    })
                    .collect(),
            )
            .map(KeyboardInteractiveStep::Challenge),
    }
}

fn read_private_key(
    identity_file: &Path,
    passphrase: Option<&str>,
) -> Result<russh::keys::PrivateKey, KeyError> {
    let encoded = Zeroizing::new(std::fs::read_to_string(identity_file).map_err(KeyError::IO)?);
    decode_secret_key(encoded.as_str(), passphrase)
}

fn private_key_error(identity_file: &Path, error: KeyError) -> SshError {
    let code = if matches!(error, KeyError::IO(_)) {
        SshErrorCode::KeyReadFailed
    } else {
        SshErrorCode::KeyDecryptFailed
    };
    SshError::new(
        code,
        "authentication",
        identity_file.display().to_string(),
        "private key could not be loaded",
    )
}

fn single_answer(response: &AuthResponse) -> Result<&str, SshError> {
    if response.answers.len() != 1 {
        return Err(SshError::new(
            SshErrorCode::ProtocolLimitExceeded,
            "authentication",
            "password",
            "password response must contain exactly one answer",
        ));
    }
    Ok(response.answers[0].expose())
}

fn auth_outcome(result: AuthResult) -> AuthOutcome {
    if matches!(result, AuthResult::Success) {
        AuthOutcome::Success
    } else {
        AuthOutcome::Rejected
    }
}

fn agent_unavailable() -> SshError {
    SshError::new(
        SshErrorCode::AgentUnavailable,
        "authentication",
        "ssh-agent",
        "no supported SSH Agent endpoint is available",
    )
}

fn authentication_failed(_: russh::Error) -> SshError {
    SshError::new(
        SshErrorCode::AuthenticationRejected,
        "authentication",
        "ssh",
        "SSH authentication failed",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prompt(text: &str, echo: bool) -> SshPrompt {
        SshPrompt {
            text: text.to_owned(),
            echo,
        }
    }

    fn secret(value: &str) -> AuthAnswer {
        AuthAnswer::new(value)
    }

    #[test]
    fn default_order_is_agent_key_password_keyboard_interactive() {
        assert_eq!(
            auth_order(None),
            [
                AuthMethod::Agent,
                AuthMethod::PrivateKey,
                AuthMethod::Password,
                AuthMethod::KeyboardInteractive,
            ]
        );
    }

    #[test]
    fn explicit_order_is_preserved() {
        let explicit = [AuthMethod::Password, AuthMethod::Agent];
        assert_eq!(auth_order(Some(&explicit)), explicit);
    }

    #[test]
    fn stale_challenge_and_oversized_round_are_rejected() {
        let mut broker = AuthBroker::new(ConnectionId::from(7_u32));
        let challenge = broker
            .begin_keyboard_interactive(vec![prompt("Password:", false)])
            .unwrap();
        assert_eq!(
            broker
                .respond(challenge.id + 1, vec![secret("x")])
                .unwrap_err()
                .code,
            SshErrorCode::ChallengeCancelled
        );
        assert_eq!(
            broker
                .begin_keyboard_interactive(vec![prompt("x", false); 17])
                .unwrap_err()
                .code,
            SshErrorCode::ProtocolLimitExceeded
        );
    }

    #[test]
    fn cancellation_zeroizes_pending_responses() {
        let mut broker = AuthBroker::new(ConnectionId::from(9_u32));
        let id = broker.begin_password().unwrap().id;
        broker.cancel(id).unwrap();
        assert!(broker.pending().is_none());
    }

    #[test]
    fn responses_must_match_prompt_count_and_size_limit() {
        let mut broker = AuthBroker::new(11);
        let challenge = broker
            .begin_keyboard_interactive(vec![prompt("one", true), prompt("two", false)])
            .unwrap();
        assert_eq!(
            broker
                .respond(challenge.id, vec![secret("one")])
                .unwrap_err()
                .code,
            SshErrorCode::ProtocolLimitExceeded
        );

        let challenge = broker.begin_password().unwrap();
        assert_eq!(
            broker
                .respond(
                    challenge.id,
                    vec![secret(&"x".repeat(MAX_RESPONSE_BYTES + 1))]
                )
                .unwrap_err()
                .code,
            SshErrorCode::ProtocolLimitExceeded
        );
    }

    #[test]
    fn only_explicit_remember_response_is_marked_for_storage() {
        let mut broker = AuthBroker::new(13);
        let challenge = broker.begin_password().unwrap();
        let response = broker
            .respond_with_remember(challenge.id, vec![secret("password")], true)
            .unwrap();
        assert!(response.remember);
        assert_eq!(password_account("work"), "ssh:work:password");
    }

    #[test]
    fn secret_debug_output_is_redacted() {
        let answer = secret("do-not-print");
        assert_eq!(format!("{answer:?}"), "AuthAnswer([REDACTED])");
    }

    struct FakeDiscovery;

    impl AgentEndpointDiscovery for FakeDiscovery {
        fn endpoint(&self) -> Result<AgentEndpoint, SshError> {
            #[cfg(unix)]
            return Ok(AgentEndpoint::UnixSocket(PathBuf::from("fake-agent")));
            #[cfg(windows)]
            return Ok(AgentEndpoint::NamedPipe("fake-agent".to_owned()));
        }
    }

    #[test]
    fn fake_agent_endpoint_discovery_is_injected() {
        #[cfg(unix)]
        assert_eq!(
            FakeDiscovery.endpoint().unwrap(),
            AgentEndpoint::UnixSocket(PathBuf::from("fake-agent"))
        );
        #[cfg(windows)]
        assert_eq!(
            FakeDiscovery.endpoint().unwrap(),
            AgentEndpoint::NamedPipe("fake-agent".to_owned())
        );
    }

    #[tokio::test]
    async fn fake_agent_connector_exercises_endpoint_and_rejection() {
        struct FakeConnector {
            calls: Vec<(AgentEndpoint, String)>,
        }

        impl AgentConnector for FakeConnector {
            fn authenticate<'a>(
                &'a mut self,
                endpoint: AgentEndpoint,
                user: &'a str,
            ) -> LocalBoxFuture<'a, Result<AuthOutcome, SshError>> {
                self.calls.push((endpoint, user.to_owned()));
                Box::pin(async { Ok(AuthOutcome::Rejected) })
            }
        }

        let mut connector = FakeConnector { calls: Vec::new() };
        assert_eq!(
            authenticate_agent_with_connector("alice", &FakeDiscovery, &mut connector)
                .await
                .unwrap(),
            AuthOutcome::Rejected
        );
        assert_eq!(connector.calls.len(), 1);
        assert_eq!(connector.calls[0].1, "alice");
    }

    #[tokio::test]
    async fn agent_client_enumerates_identities_until_one_succeeds() {
        struct FakeClient {
            attempts: Vec<u8>,
        }

        impl AgentClientBehavior for FakeClient {
            type Identity = u8;

            fn request_identities(
                &mut self,
            ) -> LocalBoxFuture<'_, Result<Vec<Self::Identity>, SshError>> {
                Box::pin(async { Ok(vec![1, 2, 3]) })
            }

            fn authenticate_identity<'a>(
                &'a mut self,
                _user: &'a str,
                identity: Self::Identity,
            ) -> LocalBoxFuture<'a, Result<AuthOutcome, SshError>> {
                self.attempts.push(identity);
                Box::pin(async move {
                    Ok(if identity == 2 {
                        AuthOutcome::Success
                    } else {
                        AuthOutcome::Rejected
                    })
                })
            }
        }

        let mut client = FakeClient {
            attempts: Vec::new(),
        };
        assert_eq!(
            authenticate_agent_client(&mut client, "alice")
                .await
                .unwrap(),
            AuthOutcome::Success
        );
        assert_eq!(client.attempts, [1, 2]);
    }

    #[tokio::test]
    async fn agent_client_failure_is_reported_without_more_attempts() {
        struct FailingClient;

        impl AgentClientBehavior for FailingClient {
            type Identity = u8;

            fn request_identities(
                &mut self,
            ) -> LocalBoxFuture<'_, Result<Vec<Self::Identity>, SshError>> {
                Box::pin(async { Err(agent_unavailable()) })
            }

            fn authenticate_identity<'a>(
                &'a mut self,
                _user: &'a str,
                _identity: Self::Identity,
            ) -> LocalBoxFuture<'a, Result<AuthOutcome, SshError>> {
                unreachable!()
            }
        }

        assert_eq!(
            authenticate_agent_client(&mut FailingClient, "alice")
                .await
                .unwrap_err()
                .code,
            SshErrorCode::AgentUnavailable
        );
    }

    struct FakeBackend {
        calls: Vec<String>,
        fail_encrypted_key: bool,
    }

    impl AuthenticationBackend for FakeBackend {
        fn authenticate_agent<'a>(
            &'a mut self,
            _user: &'a str,
        ) -> LocalBoxFuture<'a, Result<AuthOutcome, SshError>> {
            self.calls.push("agent".to_owned());
            Box::pin(async { Err(agent_unavailable()) })
        }

        fn private_key_requires_passphrase(&self, identity_file: &Path) -> Result<bool, SshError> {
            Ok(identity_file.ends_with("second"))
        }

        fn authenticate_private_key<'a>(
            &'a mut self,
            _user: &'a str,
            identity_file: &'a Path,
            passphrase: Option<&'a AuthAnswer>,
        ) -> LocalBoxFuture<'a, Result<AuthOutcome, SshError>> {
            self.calls.push(format!(
                "key:{}:{}",
                identity_file.display(),
                passphrase.is_some()
            ));
            let result = if self.fail_encrypted_key && passphrase.is_some() {
                Err(SshError::new(
                    SshErrorCode::AuthenticationRejected,
                    "authentication",
                    "fake-backend",
                    "authentication backend failed",
                ))
            } else {
                Ok(AuthOutcome::Rejected)
            };
            Box::pin(async move { result })
        }

        fn authenticate_password<'a>(
            &'a mut self,
            _user: &'a str,
            _response: &'a AuthResponse,
        ) -> LocalBoxFuture<'a, Result<AuthOutcome, SshError>> {
            self.calls.push("password".to_owned());
            Box::pin(async { Ok(AuthOutcome::Success) })
        }

        fn start_keyboard_interactive<'a>(
            &'a mut self,
            _user: &'a str,
            _broker: &'a mut AuthBroker,
        ) -> LocalBoxFuture<'a, Result<KeyboardInteractiveStep, SshError>> {
            self.calls.push("keyboard-interactive".to_owned());
            Box::pin(async { Ok(KeyboardInteractiveStep::Rejected) })
        }

        fn continue_keyboard_interactive<'a>(
            &'a mut self,
            _broker: &'a mut AuthBroker,
            _response: AuthResponse,
        ) -> LocalBoxFuture<'a, Result<KeyboardInteractiveStep, SshError>> {
            unreachable!()
        }
    }

    fn driver_config() -> super::super::types::ResolvedSshConfig {
        super::super::types::ResolvedSshConfig {
            profile_id: "profile".to_owned(),
            host: "example.com".to_owned(),
            port: 22,
            user: "alice".to_owned(),
            identity_files: vec!["first".to_owned(), "second".to_owned()],
            auth_order: auth_order(None),
            proxy_command: None,
            proxy_jump: None,
            add_keys_to_agent: super::super::types::AddKeysToAgent::No,
            known_hosts_files: Vec::new(),
            warnings: Vec::new(),
            proxy_consent_hash: None,
        }
    }

    #[tokio::test]
    async fn driver_orders_methods_iterates_keys_and_correlates_responses() {
        let mut driver = AuthenticationDriver::new(41, driver_config());
        let mut backend = FakeBackend {
            calls: Vec::new(),
            fail_encrypted_key: false,
        };

        let AuthenticationStep::Challenge(key_challenge) =
            driver.start(&mut backend).await.unwrap()
        else {
            panic!("encrypted second key should request a passphrase");
        };
        assert_eq!(backend.calls, ["agent", "key:first:false"]);
        assert_eq!(
            driver
                .respond(
                    &mut backend,
                    key_challenge.id + 1,
                    vec![secret("wrong")],
                    false,
                )
                .await
                .unwrap_err()
                .code,
            SshErrorCode::ChallengeCancelled
        );
        assert_eq!(backend.calls, ["agent", "key:first:false"]);
        assert_eq!(
            driver.start(&mut backend).await.unwrap_err().code,
            SshErrorCode::ChallengeCancelled
        );
        assert_eq!(backend.calls, ["agent", "key:first:false"]);
    }

    #[tokio::test]
    async fn driver_backend_failure_is_terminal() {
        let mut driver = AuthenticationDriver::new(42, driver_config());
        let mut backend = FakeBackend {
            calls: Vec::new(),
            fail_encrypted_key: true,
        };
        let AuthenticationStep::Challenge(key_challenge) =
            driver.start(&mut backend).await.unwrap()
        else {
            panic!("encrypted second key should request a passphrase");
        };

        assert_eq!(
            driver
                .respond(
                    &mut backend,
                    key_challenge.id,
                    vec![secret("passphrase")],
                    false,
                )
                .await
                .unwrap_err()
                .code,
            SshErrorCode::AuthenticationRejected
        );
        assert!(driver.pending().is_none());
        assert_eq!(
            driver.start(&mut backend).await.unwrap_err().code,
            SshErrorCode::ChallengeCancelled
        );
        assert_eq!(
            backend.calls,
            ["agent", "key:first:false", "key:second:true"]
        );
    }

    #[tokio::test]
    async fn driver_continues_from_key_rejection_to_password_success() {
        let mut driver = AuthenticationDriver::new(43, driver_config());
        let mut backend = FakeBackend {
            calls: Vec::new(),
            fail_encrypted_key: false,
        };
        let AuthenticationStep::Challenge(key_challenge) =
            driver.start(&mut backend).await.unwrap()
        else {
            panic!("encrypted second key should request a passphrase");
        };
        let AuthenticationStep::Challenge(password_challenge) = driver
            .respond(
                &mut backend,
                key_challenge.id,
                vec![secret("passphrase")],
                false,
            )
            .await
            .unwrap()
        else {
            panic!("password should follow rejected private keys");
        };
        assert_eq!(
            backend.calls,
            ["agent", "key:first:false", "key:second:true"]
        );
        assert_eq!(
            driver
                .respond(
                    &mut backend,
                    password_challenge.id,
                    vec![secret("password")],
                    false,
                )
                .await
                .unwrap(),
            AuthenticationStep::Success
        );
        assert_eq!(backend.calls.last().unwrap(), "password");
    }
}
