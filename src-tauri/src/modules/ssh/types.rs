use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use zeroize::Zeroize;

pub type ConnectionId = u64;
pub type ChannelId = u32;
pub type ChallengeId = u64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresentedHostKey {
    pub algorithm: String,
    pub blob: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostKeyDecision {
    Match,
    Unknown,
    Mismatch {
        old_fingerprint: String,
        new_fingerprint: String,
        file: PathBuf,
        line: usize,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SshProfileInput {
    pub id: String,
    pub name: String,
    pub source: SshProfileSource,
    pub overrides: Option<SshProfileOverrides>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SshProfileSource {
    #[serde(rename = "openssh")]
    OpenSsh {
        alias: String,
    },
    Manual {
        host: String,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AuthMethod {
    Agent,
    PrivateKey,
    Password,
    KeyboardInteractive,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SshProfileOverrides {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub user: Option<String>,
    pub identity_files: Option<Vec<String>>,
    pub auth_order: Option<Vec<AuthMethod>>,
    pub known_hosts_file: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AddKeysToAgent {
    Yes,
    Confirm,
    Ask,
    #[default]
    No,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedSshConfig {
    pub profile_id: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub identity_files: Vec<String>,
    pub auth_order: Vec<AuthMethod>,
    pub proxy_command: Option<String>,
    pub proxy_jump: Option<String>,
    pub add_keys_to_agent: AddKeysToAgent,
    pub known_hosts_files: Vec<String>,
    pub warnings: Vec<SshConfigWarning>,
    pub proxy_consent_hash: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SshConfigWarning {
    pub code: String,
    pub message: String,
    pub blocking: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionPhase {
    Dormant,
    ResolvingProfile,
    Connecting,
    VerifyingHostKey,
    AwaitingTrust,
    Authenticating,
    AwaitingAuthResponse,
    Ready,
    Disconnecting,
    Lost,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SshPrompt {
    pub text: String,
    pub echo: bool,
}

#[derive(Deserialize, Serialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct AuthAnswer(String);

impl AuthAnswer {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for AuthAnswer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AuthAnswer([REDACTED])")
    }
}

impl Drop for AuthAnswer {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AuthChallengeKind {
    Password {
        prompt: String,
    },
    PrivateKeyPassphrase {
        identity_file: String,
        prompt: String,
    },
    KeyboardInteractive {
        name: String,
        instruction: String,
        prompts: Vec<SshPrompt>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuthChallenge {
    pub connection_id: ConnectionId,
    pub id: ChallengeId,
    pub challenge: AuthChallengeKind,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuthResponse {
    pub challenge_id: ChallengeId,
    pub answers: Vec<AuthAnswer>,
    pub remember: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SshChallenge {
    UnknownHost {
        connection_id: ConnectionId,
        challenge_id: ChallengeId,
        host: String,
        port: u16,
        algorithm: String,
        fingerprint: String,
        known_hosts_file: Option<String>,
    },
    Password {
        connection_id: ConnectionId,
        challenge_id: ChallengeId,
        prompt: String,
    },
    PrivateKeyPassphrase {
        connection_id: ConnectionId,
        challenge_id: ChallengeId,
        identity_file: String,
        prompt: String,
    },
    KeyboardInteractive {
        connection_id: ConnectionId,
        challenge_id: ChallengeId,
        name: String,
        instruction: String,
        prompts: Vec<SshPrompt>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SshConnectionEvent {
    PhaseChanged {
        connection_id: ConnectionId,
        space_id: String,
        phase: ConnectionPhase,
    },
    Challenge {
        challenge: SshChallenge,
    },
    Ready {
        connection_id: ConnectionId,
        space_id: String,
        remote_home: Option<String>,
    },
    Error {
        connection_id: Option<ConnectionId>,
        space_id: String,
        error: super::errors::SshError,
    },
    Disconnected {
        connection_id: ConnectionId,
        space_id: String,
        reason: super::errors::SshErrorCode,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_source_serializes_with_explicit_openssh_tag() {
        let source = SshProfileSource::OpenSsh {
            alias: "work".to_owned(),
        };

        assert_eq!(
            serde_json::to_string(&source).expect("source must serialize"),
            r#"{"kind":"openssh","alias":"work"}"#
        );
        assert_eq!(
            serde_json::from_str::<SshProfileSource>(r#"{"kind":"openssh","alias":"work"}"#)
                .expect("source must deserialize"),
            source
        );
    }

    #[test]
    fn profile_overrides_reject_host_key_bypass() {
        let overrides = r#"{"strictHostKeyChecking":false}"#;

        assert!(serde_json::from_str::<SshProfileOverrides>(overrides).is_err());
    }

    #[test]
    fn challenge_and_event_preserve_explicit_tags() {
        let challenge = SshChallenge::UnknownHost {
            connection_id: 7,
            challenge_id: 9,
            host: "example.test".to_owned(),
            port: 22,
            algorithm: "ssh-ed25519".to_owned(),
            fingerprint: "SHA256:example".to_owned(),
            known_hosts_file: None,
        };
        let event = SshConnectionEvent::Challenge {
            challenge: challenge.clone(),
        };

        let challenge_json = serde_json::to_value(&challenge).expect("challenge must serialize");
        let event_json = serde_json::to_value(&event).expect("event must serialize");
        assert_eq!(challenge_json["kind"], "unknownHost");
        assert_eq!(event_json["type"], "challenge");
        assert_eq!(
            serde_json::from_value::<SshChallenge>(challenge_json)
                .expect("challenge must deserialize"),
            challenge
        );
        assert_eq!(
            serde_json::from_value::<SshConnectionEvent>(event_json)
                .expect("event must deserialize"),
            event
        );
    }
}
