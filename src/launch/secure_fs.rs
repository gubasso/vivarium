//! The private-filesystem primitives every runtime path is built with.
//!
//! The runtime directory and the files inside it are the channel over which the launcher hands a
//! launch specification to a supervisor it then trusts. That trust rests on ownership and mode, so
//! the modes are named here and applied through these functions rather than re-spelled at each
//! site that creates or checks a path. The supervisor's own trust check reads the same two
//! constants, which is what keeps producer and validator from drifting apart.

use crate::launch::LaunchError;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;

/// The only mode a runtime directory may carry: reachable by its owner and no one else.
pub const PRIVATE_DIR_MODE: u32 = 0o700;

/// The only mode a runtime file may carry.
pub const PRIVATE_FILE_MODE: u32 = 0o600;

/// What a path holds, from the point of view of code that expects a socket there.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SocketState {
    /// Nothing is at the path.
    Absent,
    /// A socket is at the path, most likely left by an earlier run.
    Socket,
    /// Something that is not a socket is at the path.
    Other,
}

/// Creates a directory only its owner can reach, refusing anything that is not a real directory.
///
/// The symlink rejection is the load-bearing part: a runtime root that is a symlink would let its
/// contents be redirected between the moment they are written and the moment they are trusted.
///
/// # Errors
///
/// Returns [`LaunchError::InvalidRuntimePath`] when the path exists but is not a directory, and
/// [`LaunchError::Io`] when creating, inspecting, or securing it fails.
pub async fn private_dir(path: &Path) -> Result<(), LaunchError> {
    fs::create_dir_all(path)
        .await
        .map_err(|error| LaunchError::io("create runtime directory", error))?;
    let metadata = fs::symlink_metadata(path)
        .await
        .map_err(|error| LaunchError::io("inspect runtime directory", error))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(LaunchError::InvalidRuntimePath(
            "runtime root is not a directory",
        ));
    }
    fs::set_permissions(path, std::fs::Permissions::from_mode(PRIVATE_DIR_MODE))
        .await
        .map_err(|error| LaunchError::io("set runtime permissions", error))
}

/// Writes a file readable only by its owner, creating it with that mode rather than fixing it up.
///
/// Opening with the mode closes the window in which a freshly created file is world-readable.
///
/// # Errors
///
/// Returns [`LaunchError::Io`] when the file cannot be created or written.
pub async fn private_write(path: &Path, bytes: &[u8]) -> Result<(), LaunchError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(PRIVATE_FILE_MODE)
        .open(path)
        .await
        .map_err(|error| LaunchError::io("create runtime metadata", error))?;
    file.write_all(bytes)
        .await
        .map_err(|error| LaunchError::io("write runtime metadata", error))
}

/// Reports what occupies a socket path without following a symlink to it.
///
/// # Errors
///
/// Returns [`LaunchError::Io`] when the path exists but cannot be inspected.
pub async fn socket_state(path: &Path) -> Result<SocketState, LaunchError> {
    use std::os::unix::fs::FileTypeExt;
    match fs::symlink_metadata(path).await {
        Ok(metadata) if metadata.file_type().is_socket() => Ok(SocketState::Socket),
        Ok(_) => Ok(SocketState::Other),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(SocketState::Absent),
        Err(error) => Err(LaunchError::io("inspect socket path", error)),
    }
}
