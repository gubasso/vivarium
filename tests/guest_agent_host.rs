//! Target-host proof for the real guest `AF_VSOCK` and credential relay.
#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::unwrap_used
)]

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::process::Command;
use vivarium::launch::BootMetadata;
use vivarium::protocol::hybrid;
use vivarium::protocol::{
    AgentFrame, CONTROL_PORT, ClientFrame, EnvironmentVariable, Hello, SCHEMA_VERSION, SessionMode,
    StartRequest, TerminalSize, UnixBytes, read_agent_frame, write_client_frame,
};

fn bytes(value: &[u8]) -> UnixBytes {
    UnixBytes::new(value.to_vec())
}

async fn connect_control(runtime: &Path, metadata: &BootMetadata) -> UnixStream {
    let mut stream = hybrid::connect(
        &runtime.join("control.sock"),
        CONTROL_PORT,
        Duration::from_secs(5),
    )
    .await
    .unwrap();
    write_client_frame(
        &mut stream,
        &ClientFrame::Hello(Hello {
            schema_version: SCHEMA_VERSION,
            boot_identity: metadata.boot_identity.clone(),
        }),
    )
    .await
    .unwrap();
    assert!(matches!(
        read_agent_frame(&mut stream).await.unwrap(),
        AgentFrame::Hello(_)
    ));
    stream
}

async fn execute(
    runtime: &Path,
    metadata: &BootMetadata,
    argv: Vec<UnixBytes>,
    stdin: &[u8],
    pty: bool,
) -> (Vec<u8>, Vec<u8>, u8) {
    let mut stream = connect_control(runtime, metadata).await;
    if pty {
        write_client_frame(
            &mut stream,
            &ClientFrame::Resize(TerminalSize {
                rows: 31,
                columns: 97,
            }),
        )
        .await
        .unwrap();
    }
    write_client_frame(
        &mut stream,
        &ClientFrame::Start(StartRequest {
            mode: SessionMode::Exec,
            argv,
            environment: vec![EnvironmentVariable {
                name: bytes(b"SSH_AUTH_SOCK"),
                value: bytes(b"/host/path/must-not-win"),
            }],
            cwd: bytes(b"/"),
            pty,
        }),
    )
    .await
    .unwrap();
    if !stdin.is_empty() {
        write_client_frame(&mut stream, &ClientFrame::Stdin(stdin.to_vec()))
            .await
            .unwrap();
    }
    write_client_frame(&mut stream, &ClientFrame::StdinEnd)
        .await
        .unwrap();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    loop {
        match read_agent_frame(&mut stream).await.unwrap() {
            AgentFrame::Stdout(bytes) => stdout.extend(bytes),
            AgentFrame::Stderr(bytes) => stderr.extend(bytes),
            AgentFrame::Exit(exit) => return (stdout, stderr, exit.status),
            AgentFrame::Error(error) => panic!("guest agent error: {}", error.code),
            _ => panic!("unexpected frame"),
        }
    }
}

#[tokio::test]
#[ignore = "requires Linux, Nix, /dev/kvm, a systemd user manager, and XDG_RUNTIME_DIR"]
async fn guest_agent_and_credential_relay_on_capable_host() {
    let runner =
        PathBuf::from(std::env::var_os("VIVARIUM_AGENT_RUNNER").expect("VIVARIUM_AGENT_RUNNER"));
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("vivarium-agent-host-{nonce}"));
    let runtime = root.join("runtime");
    tokio::fs::create_dir_all(&runtime).await.unwrap();
    let agent_path = root.join("ssh-agent.sock");
    let agent = UnixListener::bind(&agent_path).unwrap();
    let echo = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = agent.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut bytes = [0_u8; 4096];
                loop {
                    let Ok(count) = stream.read(&mut bytes).await else {
                        return;
                    };
                    if count == 0 {
                        return;
                    }
                    if stream.write_all(&bytes[..count]).await.is_err() {
                        return;
                    }
                }
            });
        }
    });
    let status = Command::new(&runner)
        .args([
            "--workspace",
            std::env::current_dir().unwrap().to_str().unwrap(),
            "--runtime-dir",
            runtime.to_str().unwrap(),
            "--volume",
            root.join("home.img").to_str().unwrap(),
            "--store-volume",
            root.join("store.img").to_str().unwrap(),
            "--uid",
            "1000",
            "--gid",
            "1000",
            "--memory-mib",
            "2048",
            "--vcpu",
            "2",
            "--project-id",
            "agent-check",
            "--target",
            "default",
            "--ssh-agent-socket",
            agent_path.to_str().unwrap(),
        ])
        .status()
        .await
        .unwrap();
    assert!(
        status.success(),
        "runner failed; diagnostics retained at {}",
        root.display()
    );
    let metadata: BootMetadata =
        serde_json::from_slice(&tokio::fs::read(runtime.join("boot.json")).await.unwrap()).unwrap();
    let mut ping = connect_control(&runtime, &metadata).await;
    write_client_frame(&mut ping, &ClientFrame::Ping)
        .await
        .unwrap();
    assert_eq!(read_agent_frame(&mut ping).await.unwrap(), AgentFrame::Pong);

    let (stdout, stderr, status) = execute(
        &runtime,
        &metadata,
        vec![
            bytes(b"/bin/sh"),
            bytes(b"-c"),
            bytes(b"printf out; printf err >&2; exit 42"),
        ],
        b"",
        false,
    )
    .await;
    assert_eq!(
        (stdout, stderr, status),
        (b"out".to_vec(), b"err".to_vec(), 42)
    );
    let (stdout, _, status) = execute(
        &runtime,
        &metadata,
        vec![bytes(b"/bin/sh"), bytes(b"-c"), bytes(b"stty size")],
        b"",
        true,
    )
    .await;
    assert_eq!(status, 0);
    assert!(String::from_utf8_lossy(&stdout).contains("31 97"));
    for index in 0..5 {
        let sentinel = format!("opaque-{index}");
        let (stdout, _, status) = execute(
            &runtime,
            &metadata,
            vec![
                bytes(b"/bin/sh"),
                bytes(b"-c"),
                bytes(b"socat - UNIX-CONNECT:$SSH_AUTH_SOCK"),
            ],
            sentinel.as_bytes(),
            false,
        )
        .await;
        assert_eq!(status, 0);
        assert_eq!(stdout, sentinel.as_bytes());
    }
    let _ = Command::new("systemctl")
        .args(["--user", "stop", "vivarium-agent-check-default.service"])
        .status()
        .await;
    echo.abort();
}
