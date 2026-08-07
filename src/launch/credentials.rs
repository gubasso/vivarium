//! Fixed-size host-opened parked credential pool.

use crate::launch::{CredentialSpec, LaunchError};
use crate::protocol::hybrid;
use crate::protocol::{CREDENTIAL_PARKED_ACK, CREDENTIAL_POOL_SIZE, CREDENTIAL_PORT, CredentialId};
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

const RETRY_INTERVAL: Duration = Duration::from_millis(25);

pub async fn start(
    control_socket: PathBuf,
    credentials: &[CredentialSpec],
    cancellation: CancellationToken,
    timeout: Duration,
) -> Result<Vec<JoinHandle<Result<(), LaunchError>>>, LaunchError> {
    let count = credentials.len() * CREDENTIAL_POOL_SIZE;
    let (ready_tx, mut ready_rx) = mpsc::channel(count.max(1));
    let mut handles = Vec::with_capacity(count);
    for credential in credentials {
        for _ in 0..CREDENTIAL_POOL_SIZE {
            handles.push(tokio::spawn(worker(
                control_socket.clone(),
                credential.id,
                credential.host_socket.clone(),
                cancellation.clone(),
                ready_tx.clone(),
            )));
        }
    }
    drop(ready_tx);
    let initial = async {
        for _ in 0..count {
            ready_rx
                .recv()
                .await
                .ok_or(LaunchError::Readiness("credential pool"))?;
        }
        Ok(())
    };
    tokio::time::timeout(timeout, initial)
        .await
        .map_err(|_| LaunchError::Readiness("credential pool"))??;
    Ok(handles)
}

async fn worker(
    control_socket: PathBuf,
    id: CredentialId,
    host_socket: PathBuf,
    cancellation: CancellationToken,
    ready: mpsc::Sender<()>,
) -> Result<(), LaunchError> {
    let mut announced = false;
    loop {
        if cancellation.is_cancelled() {
            return Ok(());
        }
        let relay = establish(&control_socket, id, &host_socket).await;
        match relay {
            Ok((mut guest, mut host)) => {
                if !announced {
                    ready.send(()).await.map_err(|_| LaunchError::Cancelled)?;
                    announced = true;
                }
                tokio::select! {
                    () = cancellation.cancelled() => return Ok(()),
                    _ = tokio::io::copy_bidirectional(&mut guest, &mut host) => {}
                }
            }
            Err(()) => {
                tokio::select! {
                    () = cancellation.cancelled() => return Ok(()),
                    () = tokio::time::sleep(RETRY_INTERVAL) => {}
                }
            }
        }
    }
}

async fn establish(
    control_socket: &std::path::Path,
    id: CredentialId,
    host_socket: &std::path::Path,
) -> Result<(UnixStream, UnixStream), ()> {
    let mut guest = hybrid::connect(control_socket, CREDENTIAL_PORT, Duration::from_secs(1))
        .await
        .map_err(|_| ())?;
    guest.write_u8(id.setup_byte()).await.map_err(|_| ())?;
    if guest.read_u8().await.map_err(|_| ())? != CREDENTIAL_PARKED_ACK {
        return Err(());
    }
    let host = UnixStream::connect(host_socket).await.map_err(|_| ())?;
    Ok((guest, host))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixListener;

    fn socket_path(name: &str) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "vivarium-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[tokio::test]
    async fn four_host_opened_slots_relay_opaque_bytes() {
        let control_path = socket_path("credential-control");
        let agent_path = socket_path("credential-agent");
        let control = UnixListener::bind(&control_path).unwrap();
        let agent = UnixListener::bind(&agent_path).unwrap();
        let (guest_tx, mut guest_rx) = mpsc::channel(CREDENTIAL_POOL_SIZE);
        let vmm = tokio::spawn(async move {
            for port in 0..CREDENTIAL_POOL_SIZE {
                let (mut stream, _) = control.accept().await.unwrap();
                let mut line = [0; 14];
                stream.read_exact(&mut line).await.unwrap();
                assert_eq!(&line, b"CONNECT 52001\n");
                stream
                    .write_all(format!("OK {}\n", 40_000 + port).as_bytes())
                    .await
                    .unwrap();
                assert_eq!(
                    stream.read_u8().await.unwrap(),
                    CredentialId::Ssh.setup_byte()
                );
                stream.write_u8(CREDENTIAL_PARKED_ACK).await.unwrap();
                guest_tx.send(stream).await.unwrap();
            }
        });
        let echo = tokio::spawn(async move {
            for _ in 0..CREDENTIAL_POOL_SIZE {
                let (mut stream, _) = agent.accept().await.unwrap();
                tokio::spawn(async move {
                    let mut bytes = [0; 64];
                    let count = stream.read(&mut bytes).await.unwrap();
                    stream.write_all(&bytes[..count]).await.unwrap();
                });
            }
        });
        let cancellation = CancellationToken::new();
        let handles = start(
            control_path.clone(),
            &[CredentialSpec {
                id: CredentialId::Ssh,
                host_socket: agent_path.clone(),
            }],
            cancellation.clone(),
            Duration::from_secs(1),
        )
        .await
        .unwrap();
        let mut guest = guest_rx.recv().await.unwrap();
        let sentinel = b"opaque-sentinel";
        guest.write_all(sentinel).await.unwrap();
        let mut echoed = vec![0; sentinel.len()];
        guest.read_exact(&mut echoed).await.unwrap();
        assert_eq!(echoed, sentinel);
        cancellation.cancel();
        for handle in handles {
            handle.await.unwrap().unwrap();
        }
        vmm.await.unwrap();
        echo.await.unwrap();
        let _ = std::fs::remove_file(control_path);
        let _ = std::fs::remove_file(agent_path);
    }
}
