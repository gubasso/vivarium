//! Current-boot guest-agent readiness handshake.

use crate::launch::{BootMetadata, LaunchError};
use crate::protocol::hybrid;
use crate::protocol::{
    AgentFrame, CONTROL_PORT, ClientFrame, Hello, SCHEMA_VERSION, read_agent_frame,
    write_client_frame,
};
use std::path::Path;
use std::time::Duration;

const RETRY_INTERVAL: Duration = Duration::from_millis(25);

pub async fn wait_for_agent(
    control_socket: &Path,
    metadata: &BootMetadata,
    timeout: Duration,
) -> Result<(), LaunchError> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err(LaunchError::Readiness("guest agent"));
        }
        match handshake(control_socket, metadata, remaining).await {
            Ok(()) => return Ok(()),
            Err(HandshakeError::Identity) => {
                return Err(LaunchError::Readiness("current guest identity"));
            }
            Err(HandshakeError::Retry) => {
                tokio::time::sleep(RETRY_INTERVAL.min(remaining)).await;
            }
        }
    }
}

enum HandshakeError {
    Identity,
    Retry,
}

async fn handshake(
    socket: &Path,
    metadata: &BootMetadata,
    timeout: Duration,
) -> Result<(), HandshakeError> {
    let mut stream = hybrid::connect(socket, CONTROL_PORT, timeout)
        .await
        .map_err(|_| HandshakeError::Retry)?;
    write_client_frame(
        &mut stream,
        &ClientFrame::Hello(Hello {
            schema_version: SCHEMA_VERSION,
            boot_identity: metadata.boot_identity.clone(),
        }),
    )
    .await
    .map_err(|_| HandshakeError::Retry)?;
    match read_agent_frame(&mut stream)
        .await
        .map_err(|_| HandshakeError::Retry)?
    {
        AgentFrame::Hello(hello)
            if hello.schema_version == SCHEMA_VERSION
                && hello.boot_identity == metadata.boot_identity => {}
        AgentFrame::Hello(_) => return Err(HandshakeError::Identity),
        _ => return Err(HandshakeError::Retry),
    }
    write_client_frame(&mut stream, &ClientFrame::Ping)
        .await
        .map_err(|_| HandshakeError::Retry)?;
    match read_agent_frame(&mut stream).await {
        Ok(AgentFrame::Pong) => Ok(()),
        _ => Err(HandshakeError::Retry),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::protocol::{read_client_frame, write_agent_frame};
    use std::sync::atomic::{AtomicU64, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixListener;

    /// A bindable socket path. See the note on the matching helper in `credentials.rs`: a
    /// `TMPDIR` on another drive overruns the 108-byte socket limit, so the per-user runtime
    /// tmpfs is preferred and `TMPDIR` is the fallback.
    fn socket_path() -> std::path::PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let base =
            std::path::PathBuf::from(format!("/run/user/{}", crate::config::effective_uid()));
        let base = if base.is_dir() {
            base
        } else {
            std::env::temp_dir()
        };
        base.join(format!(
            "viv-control-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn metadata(identity: &str) -> BootMetadata {
        BootMetadata {
            schema_version: SCHEMA_VERSION,
            boot_identity: identity.to_owned(),
            project_id: "p".to_owned(),
            target: "t".to_owned(),
            backend: "cloud-hypervisor".to_owned(),
            workspace_host_path: "/workspace".into(),
        }
    }

    #[tokio::test]
    async fn readiness_requires_current_identity_and_pong() {
        let path = socket_path();
        let listener = UnixListener::bind(&path).unwrap();
        let identity = "01234567-89ab-cdef-0123-456789abcdef";
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut line = [0; 14];
            stream.read_exact(&mut line).await.unwrap();
            assert_eq!(&line, b"CONNECT 52000\n");
            stream.write_all(b"OK 40000\n").await.unwrap();
            let ClientFrame::Hello(hello) = read_client_frame(&mut stream).await.unwrap() else {
                unreachable!()
            };
            write_agent_frame(
                &mut stream,
                &AgentFrame::Hello(Hello {
                    schema_version: SCHEMA_VERSION,
                    boot_identity: hello.boot_identity,
                }),
            )
            .await
            .unwrap();
            assert_eq!(
                read_client_frame(&mut stream).await.unwrap(),
                ClientFrame::Ping
            );
            write_agent_frame(&mut stream, &AgentFrame::Pong)
                .await
                .unwrap();
        });
        wait_for_agent(&path, &metadata(identity), Duration::from_secs(1))
            .await
            .unwrap();
        server.await.unwrap();
        let _ = std::fs::remove_file(path);
    }
}
