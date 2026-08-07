use std::collections::{HashMap, HashSet};
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tokio_vsock::{VMADDR_CID_HOST, VsockListener, VsockStream};
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

async fn serve_local(
    listener: UnixListener,
    mut parked: mpsc::Receiver<VsockStream>,
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
