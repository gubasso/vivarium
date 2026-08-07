use crate::session;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::sync::CancellationToken;
use tokio_vsock::{VMADDR_CID_HOST, VsockListener};
use vivarium::protocol::{
    AgentFrame, ClientFrame, CredentialId, Hello, ProtocolErrorMessage, SCHEMA_VERSION,
    read_client_frame, write_agent_frame,
};

pub async fn accept_loop(
    listener: VsockListener,
    boot_identity: String,
    credentials: Vec<CredentialId>,
    cancellation: CancellationToken,
) -> Result<(), std::io::Error> {
    loop {
        tokio::select! {
            () = cancellation.cancelled() => return Ok(()),
            accepted = listener.accept() => {
                let (stream, peer) = accepted?;
                // ADR-0065's authorization rests on the guest being unable to originate a
                // control connection. The listener binds `VMADDR_CID_ANY`, which also admits
                // a guest-local loopback peer, so host origin is checked rather than assumed.
                if peer.cid() != VMADDR_CID_HOST {
                    continue;
                }
                let identity = boot_identity.clone();
                let credentials = credentials.clone();
                tokio::spawn(async move {
                    let _ = Box::pin(handle(stream, &identity, &credentials)).await;
                });
            }
        }
    }
}

pub async fn handle<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    expected_identity: &str,
    credentials: &[CredentialId],
) -> Result<(), ()> {
    let hello = match read_client_frame(&mut stream).await {
        Ok(ClientFrame::Hello(hello))
            if hello.schema_version == SCHEMA_VERSION
                && hello.boot_identity == expected_identity =>
        {
            hello
        }
        Ok(_) => {
            safe_error(&mut stream, "authorization").await;
            return Err(());
        }
        Err(_) => {
            safe_error(&mut stream, "framing").await;
            return Err(());
        }
    };
    if write_agent_frame(
        &mut stream,
        &AgentFrame::Hello(Hello {
            schema_version: SCHEMA_VERSION,
            boot_identity: hello.boot_identity,
        }),
    )
    .await
    .is_err()
    {
        return Err(());
    }
    let mut initial_size = None;
    loop {
        match read_client_frame(&mut stream).await {
            Ok(ClientFrame::Ping) => {
                write_agent_frame(&mut stream, &AgentFrame::Pong)
                    .await
                    .map_err(|_| ())?;
                return Ok(());
            }
            Ok(ClientFrame::Resize(size)) if initial_size.is_none() => initial_size = Some(size),
            Ok(ClientFrame::Start(request)) => {
                return Box::pin(session::run(
                    &mut stream,
                    request,
                    initial_size,
                    credentials,
                ))
                .await
                .map_err(|_| ());
            }
            Ok(_) => {
                safe_error(&mut stream, "state").await;
                return Err(());
            }
            Err(_) => {
                safe_error(&mut stream, "framing").await;
                return Err(());
            }
        }
    }
}

async fn safe_error<S: AsyncWrite + Unpin>(stream: &mut S, code: &str) {
    let _ = write_agent_frame(
        stream,
        &AgentFrame::Error(ProtocolErrorMessage {
            code: code.to_owned(),
        }),
    )
    .await;
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use tokio::io::duplex;
    use vivarium::protocol::{SCHEMA_VERSION, read_agent_frame, write_client_frame};

    const ID: &str = "01234567-89ab-cdef-0123-456789abcdef";

    #[tokio::test]
    async fn authenticated_ping_round_trips() {
        let (mut client, server) = duplex(4096);
        let task = tokio::spawn(handle(server, ID, &[]));
        write_client_frame(
            &mut client,
            &ClientFrame::Hello(Hello {
                schema_version: SCHEMA_VERSION,
                boot_identity: ID.to_owned(),
            }),
        )
        .await
        .unwrap();
        assert!(matches!(
            read_agent_frame(&mut client).await.unwrap(),
            AgentFrame::Hello(_)
        ));
        write_client_frame(&mut client, &ClientFrame::Ping)
            .await
            .unwrap();
        assert_eq!(
            read_agent_frame(&mut client).await.unwrap(),
            AgentFrame::Pong
        );
        assert!(task.await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn pre_spawn_failure_reports_error() {
        use vivarium::protocol::{SessionMode, StartRequest, UnixBytes};

        let (mut client, server) = duplex(4096);
        let task = tokio::spawn(handle(server, ID, &[]));
        write_client_frame(
            &mut client,
            &ClientFrame::Hello(Hello {
                schema_version: SCHEMA_VERSION,
                boot_identity: ID.to_owned(),
            }),
        )
        .await
        .unwrap();
        assert!(matches!(
            read_agent_frame(&mut client).await.unwrap(),
            AgentFrame::Hello(_)
        ));
        write_client_frame(
            &mut client,
            &ClientFrame::Start(StartRequest {
                mode: SessionMode::Exec,
                argv: vec![UnixBytes::new(b"/vivarium/definitely-missing".to_vec())],
                environment: Vec::new(),
                cwd: UnixBytes::new(b"/".to_vec()),
                pty: false,
            }),
        )
        .await
        .unwrap();
        assert!(matches!(
            read_agent_frame(&mut client).await.unwrap(),
            AgentFrame::Error(_)
        ));
        assert!(task.await.unwrap().is_err());
    }

    #[tokio::test]
    async fn invalid_identity_fails_closed_repeatedly() {
        for _ in 0..20 {
            let (mut client, server) = duplex(4096);
            let task = tokio::spawn(handle(server, ID, &[]));
            write_client_frame(
                &mut client,
                &ClientFrame::Hello(Hello {
                    schema_version: SCHEMA_VERSION,
                    boot_identity: "ffffffff-ffff-ffff-ffff-ffffffffffff".to_owned(),
                }),
            )
            .await
            .unwrap();
            assert!(matches!(
                read_agent_frame(&mut client).await.unwrap(),
                AgentFrame::Error(_)
            ));
            assert!(task.await.unwrap().is_err());
        }
    }
}
