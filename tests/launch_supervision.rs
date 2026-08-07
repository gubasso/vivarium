// Integration tests unwrap freely; a panic here is the failure report.
#![allow(clippy::unwrap_used)]

use serde_json::json;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::AsyncWriteExt;
use tokio::net::UnixListener;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use vivarium::doctor::descriptors::host_fd_limit_sufficient;
use vivarium::launch::{
    BackendPrograms, ConfinementProfile, ConsoleReader, ConsoleSink, DescriptorBudget,
    IdentityTranslation, LaunchSpec, ResourceSpec, RuntimePaths, ShareSpec, SocketLegs, Supervisor,
    TransientUnitSpec,
};

fn fixture(name: &str) -> LaunchSpec {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("vivarium-supervision-{name}-{nonce}"));
    let child = |name: &str| root.join(name);
    LaunchSpec {
        schema_version: 2,
        project_id: "project".into(),
        target: "target".into(),
        runtime_paths: RuntimePaths {
            root: root.clone(),
            launch_spec: child("launch.json"),
            ready_socket: child("ready.sock"),
            api_socket: child("api.sock"),
            console_socket: child("console.sock"),
            console_log: child("console.log"),
            control_socket: child("control.sock"),
            vm_pid: child("vm.pid"),
            boot_json: child("boot.json"),
            vm_create_json: child("vm-create.json"),
        },
        backend_programs: BackendPrograms {
            cloud_hypervisor: "/nix/store/fake/bin/cloud-hypervisor".into(),
            ch_remote: "/nix/store/fake/bin/ch-remote".into(),
            virtiofsd: "/nix/store/fake/bin/virtiofsd".into(),
            setpriv: "/nix/store/fake/bin/setpriv".into(),
            truncate: "/nix/store/fake/bin/truncate".into(),
            mkfs_ext4: "/nix/store/fake/bin/mkfs.ext4".into(),
            systemd_run: "/nix/store/fake/bin/systemd-run".into(),
            supervisor: "/nix/store/fake/bin/vivarium-supervisor".into(),
        },
        socket_legs: SocketLegs {
            api: child("api.sock"),
            console: child("console.sock"),
            credentials: Vec::new(),
        },
        resources: ResourceSpec {
            vcpus: 2,
            memory_mib: 1024,
            cpu_weight: 100,
        },
        descriptor_budget: DescriptorBudget::default(),
        identity_translation: IdentityTranslation {
            guest_uid: 1000,
            guest_gid: 1000,
            host_uid: 1000,
            host_gid: 1000,
            overflow_uid: 65534,
            overflow_gid: 65534,
            id_max: 4_294_967_294,
        },
        shares: vec![ShareSpec {
            tag: "workspace".into(),
            source: std::env::current_dir().unwrap(),
            socket: child("workspace.sock"),
            cache: "auto".into(),
            read_only: false,
            extra_args: vec![],
        }],
        volumes: vec![],
        vm_create: json!({"payload":{"kernel":"/nix/store/fake/vmlinux"}}),
        landlock_available: false,
        console_log_enabled: true,
    }
}

#[tokio::test]
async fn reader_acknowledges_connection_before_boot_bytes_and_reaches_eof() {
    for iteration in 0..20 {
        let spec = fixture(&format!("console-{iteration}"));
        tokio::fs::create_dir_all(&spec.runtime_paths.root)
            .await
            .unwrap();
        let listener = UnixListener::bind(&spec.runtime_paths.console_socket).unwrap();
        let reader = ConsoleReader::new();
        let cancellation = CancellationToken::new();
        let (connected_tx, connected_rx) = oneshot::channel();
        let handle = reader.spawn(
            spec.runtime_paths.console_socket.clone(),
            ConsoleSink::Drain,
            cancellation,
            connected_tx,
        );
        let (mut peer, _) = listener.accept().await.unwrap();
        connected_rx.await.unwrap();
        peer.write_all(b"boot-after-connect").await.unwrap();
        drop(peer);
        handle.await.unwrap().unwrap();
        tokio::fs::remove_dir_all(&spec.runtime_paths.root)
            .await
            .unwrap();
    }
}

// Transport and lifetime are different claims. The test above proves the reader is
// attached before the first guest byte and drains to EOF; it says nothing about how
// long the reader lives, because it holds its own token. This one binds the reader to
// the token the supervisor actually hands it and asserts the lifetime half: an idle
// guest does not end the reader, a byte produced after that idle is still captured,
// and only the supervisor's cancellation ends it. `run` cancels that token no earlier
// than the post-`VmExit` teardown, so a reader whose lifetime is exactly its token's
// is a reader that survives until VM exit.
#[tokio::test]
async fn console_reader_lifetime_is_the_supervisors_cancellation() {
    for iteration in 0..20 {
        let spec = fixture(&format!("lifetime-{iteration}"));
        tokio::fs::create_dir_all(&spec.runtime_paths.root)
            .await
            .unwrap();
        let listener = UnixListener::bind(&spec.runtime_paths.console_socket).unwrap();
        let cancellation = Supervisor::new(spec.clone()).cancellation_token();
        let reader = ConsoleReader::new();
        let mut tap = reader.subscribe();
        let (connected_tx, connected_rx) = oneshot::channel();
        let handle = reader.spawn(
            spec.runtime_paths.console_socket.clone(),
            ConsoleSink::open(&spec.runtime_paths.console_log, true)
                .await
                .unwrap(),
            cancellation.clone(),
            connected_tx,
        );
        let (mut peer, _) = listener.accept().await.unwrap();
        connected_rx.await.unwrap();
        peer.write_all(b"first").await.unwrap();
        assert_eq!(tap.recv().await.unwrap(), b"first");

        // Longer than every poll and retry interval the seam is built from, so a
        // reader that ends on a quiet guest rather than on its token fails here.
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!handle.is_finished());

        peer.write_all(b"last").await.unwrap();
        assert_eq!(tap.recv().await.unwrap(), b"last");
        assert!(!handle.is_finished());

        cancellation.cancel();
        handle.await.unwrap().unwrap();
        assert_eq!(
            tokio::fs::read(&spec.runtime_paths.console_log)
                .await
                .unwrap(),
            b"firstlast"
        );
        drop(peer);
        tokio::fs::remove_dir_all(&spec.runtime_paths.root)
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn cleanup_is_allowlisted_and_idempotent() {
    let spec = fixture("cleanup");
    tokio::fs::create_dir_all(&spec.runtime_paths.root)
        .await
        .unwrap();
    tokio::fs::write(&spec.runtime_paths.launch_spec, b"owned")
        .await
        .unwrap();
    Supervisor::new(spec.clone()).cleanup().await.unwrap();
    assert!(!spec.runtime_paths.root.exists());
    Supervisor::new(spec).cleanup().await.unwrap();
}

#[tokio::test]
async fn unknown_artifact_prevents_directory_removal() {
    let spec = fixture("unknown");
    tokio::fs::create_dir_all(&spec.runtime_paths.root)
        .await
        .unwrap();
    let unknown = spec.runtime_paths.root.join("not-owned");
    tokio::fs::write(&unknown, b"preserve").await.unwrap();
    assert!(Supervisor::new(spec.clone()).cleanup().await.is_err());
    assert!(unknown.exists());
    tokio::fs::remove_dir_all(&spec.runtime_paths.root)
        .await
        .unwrap();
}

#[tokio::test]
async fn detached() {
    for _ in 0..20 {
        let spec = fixture("detached");
        let rendered = TransientUnitSpec::new(&spec).command().rendered().join(" ");
        assert!(rendered.contains("--no-block"));
        assert!(rendered.contains("--service-type=exec"));
        assert!(rendered.contains("KillMode=control-group"));
        assert!(!rendered.contains("--scope"));
    }
}

#[test]
fn one_descriptor_field_drives_daemon_unit_and_doctor() {
    let mut spec = fixture("descriptors");
    spec.descriptor_budget = DescriptorBudget {
        limit: 1000,
        worker_pool_size: 4,
    };
    let profile = ConfinementProfile::new(&spec).unwrap();
    assert_eq!(profile.shares().len(), 1);
    assert!(
        profile.shares()[0]
            .args()
            .contains(&"--rlimit-nofile=1000".into())
    );
    assert!(
        TransientUnitSpec::new(&spec)
            .command()
            .args()
            .contains(&"--property=LimitNOFILE=1000".into())
    );
    assert_eq!(
        host_fd_limit_sufficient(spec.descriptor_budget)
            .unwrap()
            .guest_allowance,
        387
    );
}
