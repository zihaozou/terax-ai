use std::path::{Path, PathBuf};
use std::sync::Arc;

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

#[cfg(unix)]
pub async fn authenticate_agent<H: russh::client::Handler, D: AgentEndpointDiscovery>(
    handle: &mut Handle<H>,
    user: &str,
    discovery: &D,
) -> Result<AuthOutcome, SshError> {
    let AgentEndpoint::UnixSocket(path) = discovery.endpoint()?;
    let mut agent = AgentClient::connect_uds(path)
        .await
        .map_err(|_| agent_unavailable())?;
    authenticate_agent_identities(handle, user, &mut agent).await
}

#[cfg(windows)]
pub async fn authenticate_agent<H: russh::client::Handler, D: AgentEndpointDiscovery>(
    handle: &mut Handle<H>,
    user: &str,
    discovery: &D,
) -> Result<AuthOutcome, SshError> {
    let AgentEndpoint::NamedPipe(path) = discovery.endpoint()?;
    let mut agent = AgentClient::connect_named_pipe(path)
        .await
        .map_err(|_| agent_unavailable())?;
    authenticate_agent_identities(handle, user, &mut agent).await
}

async fn authenticate_agent_identities<
    H: russh::client::Handler,
    S: russh::keys::agent::client::AgentStream + Send + Unpin,
>(
    handle: &mut Handle<H>,
    user: &str,
    agent: &mut AgentClient<S>,
) -> Result<AuthOutcome, SshError> {
    let identities = agent
        .request_identities()
        .await
        .map_err(|_| agent_unavailable())?;
    for identity in identities {
        let hash_alg = handle
            .best_supported_rsa_hash()
            .await
            .map_err(authentication_failed)?
            .flatten();
        let result = match identity {
            AgentIdentity::PublicKey { key, .. } => {
                handle
                    .authenticate_publickey_with(user, key, hash_alg, agent)
                    .await
            }
            AgentIdentity::Certificate { certificate, .. } => {
                handle
                    .authenticate_certificate_with(user, certificate, hash_alg, agent)
                    .await
            }
        }
        .map_err(|_| agent_unavailable())?;
        if matches!(result, AuthResult::Success) {
            return Ok(AuthOutcome::Success);
        }
    }
    Ok(AuthOutcome::Rejected)
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

pub async fn authenticate_password<H: russh::client::Handler>(
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

pub async fn start_keyboard_interactive<H: russh::client::Handler>(
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

pub async fn continue_keyboard_interactive<H: russh::client::Handler>(
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

    #[test]
    fn fake_agent_endpoint_discovery_is_injected() {
        struct FakeDiscovery;
        impl AgentEndpointDiscovery for FakeDiscovery {
            fn endpoint(&self) -> Result<AgentEndpoint, SshError> {
                #[cfg(unix)]
                return Ok(AgentEndpoint::UnixSocket(PathBuf::from("fake-agent")));
                #[cfg(windows)]
                return Ok(AgentEndpoint::NamedPipe("fake-agent".to_owned()));
            }
        }

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
}
