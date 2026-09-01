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

pub const fn retry_delays(code: SshErrorCode) -> [u64; 3] {
    let _ = code;
    [1, 2, 5]
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
        assert_eq!(retry_delays(SshErrorCode::NetworkTimeout), [1, 2, 5]);
    }
}
