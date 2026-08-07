//! Cloud Hypervisor hybrid-vsock establishment from the host Unix socket.

use std::path::Path;
use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

const ACK_MAX: usize = 64;

#[derive(Debug, Error)]
pub enum HybridError {
    #[error("hybrid transport timed out")]
    Timeout,
    #[error("hybrid transport I/O failed")]
    Io(#[source] std::io::Error),
    #[error("invalid hybrid transport acknowledgement")]
    InvalidAcknowledgement,
}

/// Establish one host-opened hybrid-vsock stream and consume the VMM acknowledgement.
///
/// # Errors
/// Returns a timeout, I/O, or acknowledgement-shape error.
pub async fn connect(
    socket: &Path,
    guest_port: u32,
    timeout: Duration,
) -> Result<UnixStream, HybridError> {
    tokio::time::timeout(timeout, async {
        let mut stream = UnixStream::connect(socket).await.map_err(HybridError::Io)?;
        stream
            .write_all(format!("CONNECT {guest_port}\n").as_bytes())
            .await
            .map_err(HybridError::Io)?;
        let mut line = Vec::with_capacity(ACK_MAX);
        loop {
            if line.len() == ACK_MAX {
                return Err(HybridError::InvalidAcknowledgement);
            }
            let byte = stream.read_u8().await.map_err(HybridError::Io)?;
            line.push(byte);
            if byte == b'\n' {
                break;
            }
        }
        let text = std::str::from_utf8(&line).map_err(|_| HybridError::InvalidAcknowledgement)?;
        text.strip_prefix("OK ")
            .and_then(|value| value.strip_suffix('\n'))
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|value| *value > 0)
            .ok_or(HybridError::InvalidAcknowledgement)?;
        Ok(stream)
    })
    .await
    .map_err(|_| HybridError::Timeout)?
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tokio::net::UnixListener;

    fn socket_path() -> std::path::PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "vivarium-hybrid-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[tokio::test]
    async fn consumes_split_ack_before_protocol_bytes() {
        let path = socket_path();
        let listener = UnixListener::bind(&path).unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0; 14];
            stream.read_exact(&mut request).await.unwrap();
            assert_eq!(&request, b"CONNECT 52000\n");
            for part in [b"OK ".as_slice(), b"4".as_slice(), b"2\nZ".as_slice()] {
                stream.write_all(part).await.unwrap();
            }
        });
        let mut stream = connect(&path, 52_000, Duration::from_secs(1))
            .await
            .unwrap();
        assert_eq!(stream.read_u8().await.unwrap(), b'Z');
        server.await.unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn rejects_bad_acknowledgements() {
        for ack in [
            b"NO 1\n".as_slice(),
            b"OK nope\n".as_slice(),
            b"OK 0\n".as_slice(),
        ] {
            let path = socket_path();
            let listener = UnixListener::bind(&path).unwrap();
            let bytes = ack.to_vec();
            tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = [0; 14];
                stream.read_exact(&mut request).await.unwrap();
                stream.write_all(&bytes).await.unwrap();
            });
            assert!(matches!(
                connect(&path, 52_000, Duration::from_secs(1)).await,
                Err(HybridError::InvalidAcknowledgement)
            ));
            let _ = std::fs::remove_file(path);
        }
    }
}
