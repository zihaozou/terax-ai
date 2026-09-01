use std::sync::Arc;

use russh::client;
use russh::{ChannelMsg, ChannelWriteHalf};
use tokio::sync::{mpsc, watch, Mutex};
use tokio::task::JoinHandle;

use super::errors::{SshError, SshErrorCode};
use super::transport::AuthenticatedTransport;
use super::types::{ChannelPhase, PtyEvent, PtySize};

const PTY_EVENT_CAPACITY: usize = 64;
const REMOTE_HOME_LIMIT: usize = 64 * 1024;

pub struct RemotePty {
    phase: watch::Sender<ChannelPhase>,
    writer: Arc<Mutex<ChannelWriteHalf<client::Msg>>>,
    events: Mutex<mpsc::Receiver<PtyEvent>>,
    reader: JoinHandle<()>,
}

impl RemotePty {
    pub async fn open(transport: &AuthenticatedTransport, size: PtySize) -> Result<Self, SshError> {
        if !size.is_valid() {
            return Err(pty_error("PTY dimensions must be non-zero"));
        }
        let channel = transport
            .open_session()
            .await
            .map_err(|error| pty_error(error.message))?;
        channel
            .request_pty(true, "xterm-256color", size.columns, size.rows, 0, 0, &[])
            .await
            .map_err(|error| pty_error(error.to_string()))?;
        channel
            .request_shell(true)
            .await
            .map_err(|error| pty_error(error.to_string()))?;

        let (mut read_half, write_half) = channel.split();
        let writer = Arc::new(Mutex::new(write_half));
        let reader_writer = writer.clone();
        let (phase, _) = watch::channel(ChannelPhase::Live);
        let reader_phase = phase.clone();
        let (events_tx, events) = mpsc::channel(PTY_EVENT_CAPACITY);
        let reader = tokio::spawn(async move {
            while let Some(message) = read_half.wait().await {
                let event = match message {
                    ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                        Some(PtyEvent::Output(data.to_vec()))
                    }
                    ChannelMsg::ExitStatus { exit_status } => {
                        reader_phase.send_replace(ChannelPhase::Exited);
                        Some(PtyEvent::ExitStatus(exit_status))
                    }
                    ChannelMsg::Close => {
                        reader_phase.send_replace(ChannelPhase::Exited);
                        Some(PtyEvent::Closed)
                    }
                    _ => None,
                };
                if let Some(event) = event {
                    if events_tx.try_send(event).is_err() {
                        let error = SshError::new(
                            SshErrorCode::ProtocolLimitExceeded,
                            "streaming PTY output",
                            "SSH channel",
                            "PTY event consumer exceeded the bounded queue",
                        );
                        let _ = reader_writer.lock().await.close().await;
                        reader_phase.send_replace(ChannelPhase::Lost);
                        let _ = events_tx.send(PtyEvent::Error(error)).await;
                        return;
                    }
                }
            }
            if *reader_phase.borrow() == ChannelPhase::Live {
                reader_phase.send_replace(ChannelPhase::Lost);
                let _ = events_tx.send(PtyEvent::Lost).await;
            }
        });
        Ok(Self {
            phase,
            writer,
            events: Mutex::new(events),
            reader,
        })
    }

    pub fn phase(&self) -> ChannelPhase {
        *self.phase.borrow()
    }

    pub async fn write(&self, data: &[u8]) -> Result<(), SshError> {
        ensure_live(self.phase())?;
        self.writer
            .lock()
            .await
            .data_bytes(data.to_vec())
            .await
            .map_err(channel_transport_error)
    }

    pub async fn resize(&self, size: PtySize) -> Result<(), SshError> {
        ensure_live(self.phase())?;
        if !size.is_valid() {
            return Err(pty_error("PTY dimensions must be non-zero"));
        }
        self.writer
            .lock()
            .await
            .window_change(size.columns, size.rows, 0, 0)
            .await
            .map_err(channel_transport_error)
    }

    pub async fn next_event(&self) -> Option<PtyEvent> {
        self.events.lock().await.recv().await
    }

    pub async fn close(&self) -> Result<(), SshError> {
        self.writer
            .lock()
            .await
            .close()
            .await
            .map_err(channel_transport_error)
    }
}

impl Drop for RemotePty {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

pub async fn discover_remote_home(
    transport: &AuthenticatedTransport,
) -> Result<Option<String>, SshError> {
    let mut channel = transport.open_session().await?;
    if channel.exec(true, "printf '%s' \"$HOME\"").await.is_err() {
        return Ok(None);
    }
    let mut output = Vec::new();
    let mut succeeded = false;
    while let Some(message) = channel.wait().await {
        match message {
            ChannelMsg::Data { data } => {
                if output.len().saturating_add(data.len()) > REMOTE_HOME_LIMIT {
                    let _ = channel.close().await;
                    return Ok(None);
                }
                output.extend_from_slice(&data);
            }
            ChannelMsg::ExitStatus { exit_status } => succeeded = exit_status == 0,
            ChannelMsg::Close => break,
            _ => {}
        }
    }
    if !succeeded {
        return Ok(None);
    }
    let Ok(path) = String::from_utf8(output) else {
        return Ok(None);
    };
    Ok(is_canonical_absolute_posix_path(&path).then_some(path))
}

fn is_canonical_absolute_posix_path(path: &str) -> bool {
    if path == "/" {
        return true;
    }
    if !path.starts_with('/') || path.contains('\0') || path.ends_with('/') {
        return false;
    }
    !path
        .split('/')
        .skip(1)
        .any(|part| part.is_empty() || part == "." || part == "..")
}

fn ensure_live(phase: ChannelPhase) -> Result<(), SshError> {
    if phase == ChannelPhase::Live {
        Ok(())
    } else {
        Err(SshError::new(
            SshErrorCode::TransportLost,
            "using PTY channel",
            "SSH channel",
            "PTY channel is no longer live",
        ))
    }
}

fn pty_error(message: impl Into<String>) -> SshError {
    SshError::new(
        SshErrorCode::PtyRejected,
        "opening PTY channel",
        "SSH server",
        message,
    )
}

fn channel_transport_error(error: russh::Error) -> SshError {
    SshError::new(
        SshErrorCode::TransportLost,
        "using PTY channel",
        "SSH transport",
        error.to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::is_canonical_absolute_posix_path;

    #[test]
    fn remote_home_requires_a_canonical_absolute_posix_path() {
        assert!(is_canonical_absolute_posix_path("/"));
        assert!(is_canonical_absolute_posix_path("/Users/alice"));
        assert!(!is_canonical_absolute_posix_path("Users/alice"));
        assert!(!is_canonical_absolute_posix_path("/Users/../root"));
        assert!(!is_canonical_absolute_posix_path("/Users//alice"));
        assert!(!is_canonical_absolute_posix_path("/Users/alice/"));
    }
}
