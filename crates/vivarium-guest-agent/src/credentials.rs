use std::collections::{HashMap, HashSet};
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tokio_vsock::{VMADDR_CID_HOST, VsockListener};
use vivarium::protocol::{CREDENTIAL_PARKED_ACK, CREDENTIAL_POOL_SIZE, CredentialId};

pub async fn run(
    listener: VsockListener,
    declared: HashSet<CredentialId>,
    cancellation: CancellationToken,
) -> Result<(), std::io::Error> {
    prepare_directory(Path::new("/run/vivarium"))?;
    let mut senders = HashMap::new();
    for id in &declared {
        let path = PathBuf::from(id.guest_socket());
        let local = create_socket(&path)?;
        let (sender, receiver) = mpsc::channel(CREDENTIAL_POOL_SIZE);
        senders.insert(*id, sender);
        tokio::spawn(serve_local(local, receiver, cancellation.clone()));
    }
    loop {
        tokio::select! {
            () = cancellation.cancelled() => return Ok(()),
            accepted = listener.accept() => {
                let (mut stream, peer) = accepted?;
                // Only host-opened connections may be parked (ADR-0071). The listener binds
                // `VMADDR_CID_ANY`, so a guest-local loopback peer would otherwise occupy a
                // pool slot and receive the relayed bytes of a guest client.
                if peer.cid() != VMADDR_CID_HOST {
                    continue;
                }
                let senders = senders.clone();
                tokio::spawn(async move {
                    let id = stream
                        .read_u8()
                        .await
                        .ok()
                        .and_then(|byte| CredentialId::from_setup_byte(byte).ok());
                    let Some(sender) = id.and_then(|id| senders.get(&id).cloned()) else { return; };
                    let Ok(permit) = sender.reserve_owned().await else { return; };
                    if stream.write_u8(CREDENTIAL_PARKED_ACK).await.is_ok() { permit.send(stream); }
                });
            }
        }
    }
}

fn prepare_directory(path: &Path) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    verify_owner(path)
}

fn create_socket(path: &Path) -> Result<UnixListener, std::io::Error> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_socket() => std::fs::remove_file(path)?,
        Ok(_) => {
            return Err(std::io::Error::other(
                "credential path is not a stale socket",
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    verify_owner(path)?;
    Ok(listener)
}

fn verify_owner(path: &Path) -> Result<(), std::io::Error> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.uid() != rustix::process::getuid().as_raw()
        || metadata.gid() != rustix::process::getgid().as_raw()
    {
        return Err(std::io::Error::other("credential path ownership mismatch"));
    }
    Ok(())
}

/// Hand one parked host connection to one guest-local client, and no more.
///
/// The parked stream is a `VsockStream` in the running agent; the bound is written as one
/// because nothing here depends on the transport, and a deterministic test cannot open an
/// `AF_VSOCK` connection without the loopback module the guest does not load.
async fn serve_local<S: AsyncRead + AsyncWrite + Send + Unpin + 'static>(
    listener: UnixListener,
    mut parked: mpsc::Receiver<S>,
    cancellation: CancellationToken,
) {
    loop {
        let Some(mut stream) = parked.recv().await else {
            return;
        };
        tokio::select! {
            () = cancellation.cancelled() => return,
            accepted = listener.accept() => if let Ok((mut local, _)) = accepted {
                tokio::spawn(async move {
                    let _ = tokio::io::copy_bidirectional(&mut stream, &mut local).await;
                });
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tokio::net::UnixStream;

    fn scratch(name: &str) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "vivarium-credentials-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    /// The credential directory is created private and owned by the agent's own user.
    ///
    /// `ADR-0071` rests on the guest user being the only one that can reach the socket, so
    /// the mode is asserted rather than assumed from `create_dir_all`'s umask behaviour.
    #[test]
    fn the_directory_is_private_and_owned_by_the_agent() {
        let path = scratch("directory");
        prepare_directory(&path).unwrap();
        let metadata = std::fs::symlink_metadata(&path).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o777, 0o700);
        assert_eq!(metadata.uid(), rustix::process::getuid().as_raw());
        // Re-preparing an existing directory is how a restarted agent finds its own
        // runtime directory, so it must not fail.
        prepare_directory(&path).unwrap();
        std::fs::remove_dir_all(&path).unwrap();
    }

    /// The credential socket is created private and owned, and replaces a stale one.
    #[tokio::test]
    async fn the_socket_is_private_owned_and_replaces_a_stale_one() {
        let path = scratch("socket");
        let listener = create_socket(&path).unwrap();
        let metadata = std::fs::symlink_metadata(&path).unwrap();
        assert!(metadata.file_type().is_socket());
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        assert_eq!(metadata.uid(), rustix::process::getuid().as_raw());
        // A restart finds the previous boot's socket still on disk; binding over it is
        // only safe because the path was verified to be a socket first.
        drop(listener);
        let replacement = create_socket(&path).unwrap();
        assert!(
            std::fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_socket()
        );
        drop(replacement);
        std::fs::remove_file(&path).unwrap();
    }

    /// A path that is not a stale socket is refused rather than unlinked.
    ///
    /// Unlinking whatever happens to sit at the path would let a wrong configuration
    /// destroy a real file; failing closed is the only safe reading.
    #[tokio::test]
    async fn a_non_socket_path_is_refused() {
        let path = scratch("regular-file");
        std::fs::write(&path, b"not a socket").unwrap();
        assert!(create_socket(&path).is_err());
        // The refusal must leave the file intact.
        assert_eq!(std::fs::read(&path).unwrap(), b"not a socket");
        std::fs::remove_file(&path).unwrap();
    }

    /// One guest-local client consumes exactly one parked connection.
    ///
    /// The pool is bounded, so a local client that could drain more than its own slot
    /// would starve the rest; the second client here is served only once a second host
    /// connection has been parked.
    #[tokio::test]
    async fn a_local_client_consumes_exactly_one_parked_connection() {
        let path = scratch("relay");
        let listener = create_socket(&path).unwrap();
        let (sender, receiver) = mpsc::channel(CREDENTIAL_POOL_SIZE);
        let cancellation = CancellationToken::new();
        let relay = tokio::spawn(serve_local(listener, receiver, cancellation.clone()));

        // Park one host connection and let a guest-local client use it.
        let (host, parked) = UnixStream::pair().unwrap();
        sender.send(parked).await.unwrap();
        let mut local = UnixStream::connect(&path).await.unwrap();
        let mut host = host;
        local.write_all(b"opaque-request").await.unwrap();
        let mut seen = vec![0; b"opaque-request".len()];
        host.read_exact(&mut seen).await.unwrap();
        assert_eq!(seen, b"opaque-request");
        host.write_all(b"opaque-reply").await.unwrap();
        let mut back = vec![0; b"opaque-reply".len()];
        local.read_exact(&mut back).await.unwrap();
        assert_eq!(back, b"opaque-reply");

        // A second client waits: the first consumed the only parked connection.
        let mut second = UnixStream::connect(&path).await.unwrap();
        second.write_all(b"second").await.unwrap();
        let mut idle = [0; 6];
        assert!(
            tokio::time::timeout(
                std::time::Duration::from_millis(250),
                second.read_exact(&mut idle)
            )
            .await
            .is_err(),
            "a second local client was served without a second parked connection"
        );

        // Park another and the waiting client is served from it.
        let (second_host, second_parked) = UnixStream::pair().unwrap();
        sender.send(second_parked).await.unwrap();
        let mut second_host = second_host;
        let mut seen = vec![0; b"second".len()];
        second_host.read_exact(&mut seen).await.unwrap();
        assert_eq!(seen, b"second");

        cancellation.cancel();
        drop(sender);
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), relay).await;
        std::fs::remove_file(&path).unwrap();
    }
}
