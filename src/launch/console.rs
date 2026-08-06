//! The single lossless console reader and non-blocking attach fan-out.

use crate::launch::LaunchError;
use crate::launch::secure_fs::PRIVATE_FILE_MODE;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::fs::{self, File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::sync::{broadcast, oneshot};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

const CONSOLE_ROTATE_BYTES: u64 = 4 * 1024 * 1024;
const CONSOLE_ROTATE_COUNT: usize = 2;

/// How long to wait between attempts to reach the console socket, and how many to make.
///
/// Deliberately its own pair rather than the supervisor's child-polling cadence, which happens to
/// use the same interval: these bound how long the VMM may take to publish its socket, and they
/// should be free to move without disturbing how often a child is checked for exit.
const CONNECT_RETRY_INTERVAL: Duration = Duration::from_millis(25);
const CONNECT_RETRIES: usize = 200;

pub enum ConsoleSink {
    File {
        path: PathBuf,
        file: File,
        written: u64,
    },
    Drain,
}

impl ConsoleSink {
    /// Open the private lossless file sink, or a draining sink when disabled.
    ///
    /// # Errors
    ///
    /// Returns an error when the console file cannot be opened or inspected.
    pub async fn open(path: &Path, enabled: bool) -> Result<Self, LaunchError> {
        if !enabled {
            return Ok(Self::Drain);
        }
        let file = secure_open(path).await?;
        let written = file
            .metadata()
            .await
            .map_err(|error| LaunchError::io("inspect console log", error))?
            .len();
        Ok(Self::File {
            path: path.to_path_buf(),
            file,
            written,
        })
    }

    async fn write_all(&mut self, bytes: &[u8]) -> Result<(), LaunchError> {
        match self {
            Self::Drain => Ok(()),
            Self::File {
                path,
                file,
                written,
            } => {
                if written.saturating_add(bytes.len() as u64) > CONSOLE_ROTATE_BYTES {
                    file.flush()
                        .await
                        .map_err(|error| LaunchError::io("flush console log", error))?;
                    rotate(path).await?;
                    *file = secure_open(path).await?;
                    *written = 0;
                }
                file.write_all(bytes)
                    .await
                    .map_err(|error| LaunchError::io("write console log", error))?;
                *written = written.saturating_add(bytes.len() as u64);
                Ok(())
            }
        }
    }
}

pub struct ConsoleReader {
    subscribers: broadcast::Sender<Vec<u8>>,
}

impl ConsoleReader {
    #[must_use]
    pub fn new() -> Self {
        let (subscribers, _) = broadcast::channel(64);
        Self { subscribers }
    }

    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<Vec<u8>> {
        self.subscribers.subscribe()
    }

    pub fn spawn(
        &self,
        socket: PathBuf,
        mut sink: ConsoleSink,
        cancellation: CancellationToken,
        connected: oneshot::Sender<()>,
    ) -> JoinHandle<Result<(), LaunchError>> {
        let subscribers = self.subscribers.clone();
        tokio::spawn(async move {
            let mut stream = connect_bounded(&socket, &cancellation).await?;
            connected
                .send(())
                .map_err(|()| LaunchError::Readiness("console acknowledgment receiver"))?;
            let mut buffer = vec![0_u8; 64 * 1024];
            loop {
                tokio::select! {
                    () = cancellation.cancelled() => break,
                    result = stream.read(&mut buffer) => {
                        let count = result.map_err(|error| LaunchError::io("read console", error))?;
                        if count == 0 { break; }
                        sink.write_all(&buffer[..count]).await?;
                        let _ = subscribers.send(buffer[..count].to_vec());
                    }
                }
            }
            Ok(())
        })
    }
}

impl Default for ConsoleReader {
    fn default() -> Self {
        Self::new()
    }
}

async fn connect_bounded(
    path: &Path,
    cancellation: &CancellationToken,
) -> Result<UnixStream, LaunchError> {
    for _ in 0..CONNECT_RETRIES {
        if let Ok(stream) = UnixStream::connect(path).await {
            return Ok(stream);
        }
        tokio::select! {
            () = cancellation.cancelled() => return Err(LaunchError::Cancelled),
            () = tokio::time::sleep(CONNECT_RETRY_INTERVAL) => {}
        }
    }
    Err(LaunchError::Readiness("console socket connection"))
}

async fn secure_open(path: &Path) -> Result<File, LaunchError> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .mode(PRIVATE_FILE_MODE)
        .open(path)
        .await
        .map_err(|error| LaunchError::io("open console log", error))
}

async fn rotate(path: &Path) -> Result<(), LaunchError> {
    for index in (1..=CONSOLE_ROTATE_COUNT).rev() {
        let from = if index == 1 {
            path.to_path_buf()
        } else {
            numbered(path, index - 1)
        };
        let to = numbered(path, index);
        match fs::rename(&from, &to).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(LaunchError::io("rotate console log", error)),
        }
    }
    Ok(())
}

fn numbered(path: &Path, index: usize) -> PathBuf {
    PathBuf::from(format!("{}.{}", path.display(), index))
}
