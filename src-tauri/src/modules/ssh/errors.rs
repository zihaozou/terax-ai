use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SshErrorCode {
    ProfileInvalid,
    ConfigUnsupported,
    DnsFailure,
    NetworkTimeout,
    ConnectionRefused,
    ProxyCommandDenied,
    ProxyCommandFailed,
    HostUnknown,
    HostKeyMismatch,
    KnownHostsReadFailed,
    AgentUnavailable,
    KeyReadFailed,
    KeyDecryptFailed,
    AuthenticationRejected,
    ChallengeCancelled,
    PtyRejected,
    TransportLost,
    ChannelLimitReached,
    ProtocolLimitExceeded,
}

impl SshErrorCode {
    pub const fn is_retryable(self) -> bool {
        matches!(
            self,
            Self::DnsFailure
                | Self::NetworkTimeout
                | Self::ConnectionRefused
                | Self::ProxyCommandFailed
                | Self::TransportLost
        )
    }
}

const NETWORK_RETRY_DELAYS: &[u64] = &[1, 2, 5];
const NO_RETRY_DELAYS: &[u64] = &[];

pub const fn retry_delays(code: SshErrorCode) -> &'static [u64] {
    if code.is_retryable() {
        NETWORK_RETRY_DELAYS
    } else {
        NO_RETRY_DELAYS
    }
}

#[derive(Clone, Debug, Deserialize, Error, Serialize, PartialEq, Eq)]
#[error("{code:?} during {stage} for {target}: {message}")]
#[serde(rename_all = "camelCase")]
pub struct SshError {
    pub code: SshErrorCode,
    pub stage: String,
    pub target: String,
    pub message: String,
    pub retryable: bool,
}

impl SshError {
    pub fn new(
        code: SshErrorCode,
        stage: impl Into<String>,
        target: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            stage: stage.into(),
            target: target.into(),
            message: message.into(),
            retryable: code.is_retryable(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_and_host_key_errors_do_not_retry() {
        assert!(!SshErrorCode::AuthenticationRejected.is_retryable());
        assert!(!SshErrorCode::HostKeyMismatch.is_retryable());
        assert!(!SshErrorCode::KnownHostsReadFailed.is_retryable());
        assert!(retry_delays(SshErrorCode::AuthenticationRejected).is_empty());
        assert!(retry_delays(SshErrorCode::HostKeyMismatch).is_empty());
        assert!(retry_delays(SshErrorCode::KnownHostsReadFailed).is_empty());
        assert_eq!(retry_delays(SshErrorCode::NetworkTimeout), [1, 2, 5]);
    }

    #[test]
    fn authentication_error_codes_have_stable_wire_names() {
        let cases = [
            (SshErrorCode::AgentUnavailable, "\"agentUnavailable\""),
            (SshErrorCode::KeyReadFailed, "\"keyReadFailed\""),
            (SshErrorCode::KeyDecryptFailed, "\"keyDecryptFailed\""),
            (
                SshErrorCode::AuthenticationRejected,
                "\"authenticationRejected\"",
            ),
            (SshErrorCode::ChallengeCancelled, "\"challengeCancelled\""),
            (
                SshErrorCode::ProtocolLimitExceeded,
                "\"protocolLimitExceeded\"",
            ),
        ];

        for (code, expected) in cases {
            assert_eq!(serde_json::to_string(&code).unwrap(), expected);
        }
    }
}
