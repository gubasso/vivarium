//! Bounded frame codec shared by host and guest.

use super::message::{
    AgentFrame, CONTROL_PAYLOAD_MAX, ClientFrame, ExitStatus, Hello, ProtocolErrorMessage,
    STREAM_PAYLOAD_MAX, SignalRequest, StartRequest, TAG_AGENT_HELLO, TAG_CLIENT_HELLO, TAG_ERROR,
    TAG_EXIT, TAG_PING, TAG_PONG, TAG_RESIZE, TAG_SIGNAL, TAG_START, TAG_STDERR, TAG_STDIN,
    TAG_STDIN_END, TAG_STDOUT, TerminalSize,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[derive(Debug, Error)]
pub enum FrameError {
    #[error("end of stream")]
    Eof,
    #[error("truncated frame")]
    Truncated,
    #[error("invalid frame length")]
    InvalidLength,
    #[error("frame exceeds its size bound")]
    Oversized,
    #[error("unknown frame tag")]
    UnknownTag,
    #[error("frame tag is invalid in this direction")]
    WrongDirection,
    #[error("malformed control payload")]
    MalformedControl,
    #[error("protocol I/O failed")]
    Io(#[source] std::io::Error),
}

impl FrameError {
    fn read(error: std::io::Error) -> Self {
        if error.kind() == std::io::ErrorKind::UnexpectedEof {
            Self::Truncated
        } else {
            Self::Io(error)
        }
    }
}

async fn read_raw<R: AsyncRead + Unpin>(reader: &mut R) -> Result<(u8, Vec<u8>), FrameError> {
    let mut prefix = [0_u8; 4];
    let count = reader
        .read(&mut prefix[..1])
        .await
        .map_err(FrameError::Io)?;
    if count == 0 {
        return Err(FrameError::Eof);
    }
    reader
        .read_exact(&mut prefix[1..])
        .await
        .map_err(FrameError::read)?;
    let length = usize::try_from(u32::from_be_bytes(prefix)).map_err(|_| FrameError::Oversized)?;
    if length == 0 {
        return Err(FrameError::InvalidLength);
    }
    if length > CONTROL_PAYLOAD_MAX + 1 {
        return Err(FrameError::Oversized);
    }
    let mut tag = [0_u8; 1];
    reader
        .read_exact(&mut tag)
        .await
        .map_err(FrameError::read)?;
    let payload_len = length - 1;
    if is_stream_tag(tag[0]) && payload_len > STREAM_PAYLOAD_MAX {
        return Err(FrameError::Oversized);
    }
    let mut payload = vec![0; payload_len];
    reader
        .read_exact(&mut payload)
        .await
        .map_err(FrameError::read)?;
    Ok((tag[0], payload))
}

async fn write_raw<W: AsyncWrite + Unpin>(
    writer: &mut W,
    tag: u8,
    payload: &[u8],
) -> Result<(), FrameError> {
    let max = if is_stream_tag(tag) {
        STREAM_PAYLOAD_MAX
    } else {
        CONTROL_PAYLOAD_MAX
    };
    if payload.len() > max {
        return Err(FrameError::Oversized);
    }
    let length = u32::try_from(payload.len() + 1).map_err(|_| FrameError::Oversized)?;
    writer
        .write_all(&length.to_be_bytes())
        .await
        .map_err(FrameError::Io)?;
    writer.write_all(&[tag]).await.map_err(FrameError::Io)?;
    writer.write_all(payload).await.map_err(FrameError::Io)?;
    writer.flush().await.map_err(FrameError::Io)
}

const fn is_stream_tag(tag: u8) -> bool {
    matches!(tag, TAG_STDIN | TAG_STDOUT | TAG_STDERR)
}

fn json<T: Serialize>(value: &T) -> Result<Vec<u8>, FrameError> {
    serde_json::to_vec(value).map_err(|_| FrameError::MalformedControl)
}

fn from_json<T: DeserializeOwned>(payload: &[u8]) -> Result<T, FrameError> {
    serde_json::from_slice(payload).map_err(|_| FrameError::MalformedControl)
}

/// Write one bounded client-direction frame.
///
/// # Errors
/// Returns a classified framing or I/O error without formatting payload bytes.
pub async fn write_client_frame<W: AsyncWrite + Unpin>(
    writer: &mut W,
    frame: &ClientFrame,
) -> Result<(), FrameError> {
    let (tag, payload) = match frame {
        ClientFrame::Hello(value) => (TAG_CLIENT_HELLO, json(value)?),
        ClientFrame::Ping => (TAG_PING, Vec::new()),
        ClientFrame::Start(value) => (TAG_START, json(value)?),
        ClientFrame::Stdin(value) => (TAG_STDIN, value.clone()),
        ClientFrame::StdinEnd => (TAG_STDIN_END, Vec::new()),
        ClientFrame::Resize(value) => (TAG_RESIZE, json(value)?),
        ClientFrame::Signal(value) => (TAG_SIGNAL, json(value)?),
    };
    write_raw(writer, tag, &payload).await
}

/// Write one bounded agent-direction frame.
///
/// # Errors
/// Returns a classified framing or I/O error without formatting payload bytes.
pub async fn write_agent_frame<W: AsyncWrite + Unpin>(
    writer: &mut W,
    frame: &AgentFrame,
) -> Result<(), FrameError> {
    let (tag, payload) = match frame {
        AgentFrame::Hello(value) => (TAG_AGENT_HELLO, json(value)?),
        AgentFrame::Pong => (TAG_PONG, Vec::new()),
        AgentFrame::Stdout(value) => (TAG_STDOUT, value.clone()),
        AgentFrame::Stderr(value) => (TAG_STDERR, value.clone()),
        AgentFrame::Exit(value) => (TAG_EXIT, json(value)?),
        AgentFrame::Error(value) => (TAG_ERROR, json(value)?),
    };
    write_raw(writer, tag, &payload).await
}

/// Read and validate one client-direction frame.
///
/// # Errors
/// Returns a classified error for EOF, truncation, bounds, direction, JSON, or I/O.
pub async fn read_client_frame<R: AsyncRead + Unpin>(
    reader: &mut R,
) -> Result<ClientFrame, FrameError> {
    let (tag, payload) = read_raw(reader).await?;
    match tag {
        TAG_CLIENT_HELLO => Ok(ClientFrame::Hello(from_json::<Hello>(&payload)?)),
        TAG_PING if payload.is_empty() => Ok(ClientFrame::Ping),
        TAG_START => Ok(ClientFrame::Start(from_json::<StartRequest>(&payload)?)),
        TAG_STDIN => Ok(ClientFrame::Stdin(payload)),
        TAG_STDIN_END if payload.is_empty() => Ok(ClientFrame::StdinEnd),
        TAG_RESIZE => Ok(ClientFrame::Resize(from_json::<TerminalSize>(&payload)?)),
        TAG_SIGNAL => Ok(ClientFrame::Signal(from_json::<SignalRequest>(&payload)?)),
        TAG_AGENT_HELLO | TAG_PONG | TAG_STDOUT | TAG_STDERR | TAG_EXIT | TAG_ERROR => {
            Err(FrameError::WrongDirection)
        }
        TAG_PING | TAG_STDIN_END => Err(FrameError::MalformedControl),
        _ => Err(FrameError::UnknownTag),
    }
}

/// Read and validate one agent-direction frame.
///
/// # Errors
/// Returns a classified error for EOF, truncation, bounds, direction, JSON, or I/O.
pub async fn read_agent_frame<R: AsyncRead + Unpin>(
    reader: &mut R,
) -> Result<AgentFrame, FrameError> {
    let (tag, payload) = read_raw(reader).await?;
    match tag {
        TAG_AGENT_HELLO => Ok(AgentFrame::Hello(from_json::<Hello>(&payload)?)),
        TAG_PONG if payload.is_empty() => Ok(AgentFrame::Pong),
        TAG_STDOUT => Ok(AgentFrame::Stdout(payload)),
        TAG_STDERR => Ok(AgentFrame::Stderr(payload)),
        TAG_EXIT => Ok(AgentFrame::Exit(from_json::<ExitStatus>(&payload)?)),
        TAG_ERROR => Ok(AgentFrame::Error(from_json::<ProtocolErrorMessage>(
            &payload,
        )?)),
        TAG_CLIENT_HELLO | TAG_PING | TAG_START | TAG_STDIN | TAG_STDIN_END | TAG_RESIZE
        | TAG_SIGNAL => Err(FrameError::WrongDirection),
        TAG_PONG => Err(FrameError::MalformedControl),
        _ => Err(FrameError::UnknownTag),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::protocol::message::{EnvironmentVariable, SessionMode, UnixBytes};

    async fn encoded_client(frame: ClientFrame) -> Vec<u8> {
        let mut bytes = Vec::new();
        write_client_frame(&mut bytes, &frame).await.unwrap();
        bytes
    }

    async fn encoded_agent(frame: AgentFrame) -> Vec<u8> {
        let mut bytes = Vec::new();
        write_agent_frame(&mut bytes, &frame).await.unwrap();
        bytes
    }

    /// Assemble the wire form by hand: a four-byte big-endian length covering the tag plus
    /// the payload, then the tag, then the payload.
    fn framed(tag: u8, payload: &[u8]) -> Vec<u8> {
        let mut bytes = u32::try_from(payload.len() + 1)
            .unwrap()
            .to_be_bytes()
            .to_vec();
        bytes.push(tag);
        bytes.extend_from_slice(payload);
        bytes
    }

    /// Pin the exact encoding of every tag in `spec/12`'s table.
    ///
    /// This is the line the specification's tag table encodes, so all thirteen appear here
    /// with their literal payloads: a tag that only round-trips through this crate's own
    /// codec would still be free to drift, and the argv and environment wire shape (a JSON
    /// array of byte values, from `UnixBytes` being `serde(transparent)`) is a contract a
    /// second implementation has to match.
    #[tokio::test]
    async fn golden_tags_and_shapes() {
        const IDENTITY: &str = "01234567-89ab-cdef-0123-456789abcdef";
        let hello = Hello {
            schema_version: 1,
            boot_identity: IDENTITY.to_owned(),
        };
        let hello_json =
            br#"{"schema_version":1,"boot_identity":"01234567-89ab-cdef-0123-456789abcdef"}"#;

        // Client direction, tags 0x01 and 0x03 through 0x09.
        assert_eq!(
            encoded_client(ClientFrame::Hello(hello.clone())).await,
            framed(0x01, hello_json)
        );
        assert_eq!(encoded_client(ClientFrame::Ping).await, framed(0x03, b""));
        assert_eq!(
            encoded_client(ClientFrame::Start(StartRequest {
                mode: SessionMode::Exec,
                argv: vec![
                    UnixBytes::new(b"/bin/sh".to_vec()),
                    UnixBytes::new(vec![0xff])
                ],
                environment: vec![EnvironmentVariable {
                    name: UnixBytes::new(b"K".to_vec()),
                    value: UnixBytes::new(vec![0xfe]),
                }],
                cwd: UnixBytes::new(b"/".to_vec()),
                pty: true,
            }))
            .await,
            framed(
                0x05,
                // Split only to stay inside the line budget; the two halves are adjacent
                // bytes of one payload and the seam carries no whitespace.
                &[
                    br#"{"mode":"exec","argv":[[47,98,105,110,47,115,104],[255]],"#.as_slice(),
                    br#""environment":[{"name":[75],"value":[254]}],"cwd":[47],"pty":true}"#
                        .as_slice(),
                ]
                .concat()
            )
        );
        assert_eq!(
            encoded_client(ClientFrame::Stdin(vec![0xff])).await,
            framed(0x06, &[0xff])
        );
        assert_eq!(
            encoded_client(ClientFrame::StdinEnd).await,
            framed(0x07, b"")
        );
        assert_eq!(
            encoded_client(ClientFrame::Resize(TerminalSize {
                rows: 24,
                columns: 80
            }))
            .await,
            framed(0x08, br#"{"rows":24,"columns":80}"#)
        );
        assert_eq!(
            encoded_client(ClientFrame::Signal(SignalRequest { signal: 15 })).await,
            framed(0x09, br#"{"signal":15}"#)
        );

        // Agent direction, tags 0x02, 0x04, and 0x0a through 0x0d.
        assert_eq!(
            encoded_agent(AgentFrame::Hello(hello)).await,
            framed(0x02, hello_json)
        );
        assert_eq!(encoded_agent(AgentFrame::Pong).await, framed(0x04, b""));
        assert_eq!(
            encoded_agent(AgentFrame::Stdout(vec![0xfe])).await,
            framed(0x0a, &[0xfe])
        );
        assert_eq!(
            encoded_agent(AgentFrame::Stderr(vec![0xfd])).await,
            framed(0x0b, &[0xfd])
        );
        assert_eq!(
            encoded_agent(AgentFrame::Exit(ExitStatus { status: 42 })).await,
            framed(0x0c, br#"{"status":42}"#)
        );
        assert_eq!(
            encoded_agent(AgentFrame::Error(ProtocolErrorMessage {
                code: "framing".to_owned()
            }))
            .await,
            framed(0x0d, br#"{"code":"framing"}"#)
        );
    }

    #[tokio::test]
    async fn byte_values_round_trip_without_utf8() {
        let request = StartRequest {
            mode: SessionMode::Exec,
            argv: vec![UnixBytes::new(vec![b'a', 0xff])],
            environment: vec![EnvironmentVariable {
                name: UnixBytes::new(b"K".to_vec()),
                value: UnixBytes::new(vec![0xfe]),
            }],
            cwd: UnixBytes::new(b"/".to_vec()),
            pty: false,
        };
        let bytes = encoded_client(ClientFrame::Start(request.clone())).await;
        let decoded = read_client_frame(&mut bytes.as_slice()).await.unwrap();
        assert_eq!(decoded, ClientFrame::Start(request));
    }

    #[tokio::test]
    async fn malformed_input_fails_closed_repeatedly() {
        for _ in 0..20 {
            assert!(matches!(
                read_client_frame(&mut [0, 0, 0, 0].as_slice()).await,
                Err(FrameError::InvalidLength)
            ));
            assert!(matches!(
                read_client_frame(&mut [0, 0, 0, 1, 0xff].as_slice()).await,
                Err(FrameError::UnknownTag)
            ));
            assert!(matches!(
                read_client_frame(&mut [0, 0, 0, 1, TAG_PONG].as_slice()).await,
                Err(FrameError::WrongDirection)
            ));
            assert!(matches!(
                read_client_frame(&mut [0, 0, 0, 2, TAG_START, b'{'].as_slice()).await,
                Err(FrameError::MalformedControl)
            ));
            assert!(matches!(
                read_client_frame(&mut [0, 0, 0, 2, TAG_STDIN].as_slice()).await,
                Err(FrameError::Truncated)
            ));
            assert!(matches!(
                read_client_frame(&mut [0, 0].as_slice()).await,
                Err(FrameError::Truncated)
            ));
        }
    }

    #[tokio::test]
    async fn oversized_lengths_are_rejected_before_payload_read() {
        let control = u32::try_from(CONTROL_PAYLOAD_MAX + 2)
            .unwrap()
            .to_be_bytes();
        assert!(matches!(
            read_client_frame(&mut control.as_slice()).await,
            Err(FrameError::Oversized)
        ));
        let mut stream = Vec::from(u32::try_from(STREAM_PAYLOAD_MAX + 2).unwrap().to_be_bytes());
        stream.push(TAG_STDIN);
        assert!(matches!(
            read_client_frame(&mut stream.as_slice()).await,
            Err(FrameError::Oversized)
        ));
    }
}
