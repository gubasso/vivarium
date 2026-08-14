// Integration tests unwrap freely; a panic here is the failure report.
#![allow(clippy::unwrap_used)]

use serde_json::json;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use vivarium::doctor::descriptors::host_fd_limit_sufficient;
use vivarium::launch::{
    BackendPrograms, ConfinementProfile, ConsoleReader, ConsoleSink, DescriptorBudget, EgressSpec,
    GuestSession, IdentityTranslation, LAUNCH_SCHEMA_VERSION, LaunchEgressMode, LaunchReady,
    LaunchSpec, NetworkSpec, ReadinessReport, ResourceSpec, RuntimePaths, ShareSpec, SocketLegs,
    Supervisor, TransientUnitSpec,
};

fn fixture(name: &str) -> LaunchSpec {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("vivarium-supervision-{name}-{nonce}"));
    let child = |name: &str| root.join(name);
    LaunchSpec {
        schema_version: LAUNCH_SCHEMA_VERSION,
        project_id: "project".into(),
        target: "target".into(),
        runtime_paths: RuntimePaths {
            root: root.clone(),
            launch_spec: child("launch.json"),
            lock: child("lock"),
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
            unshare: "/nix/store/fake/bin/unshare".into(),
            nsenter: "/nix/store/fake/bin/nsenter".into(),
            ip: "/nix/store/fake/bin/ip".into(),
            nft: "/nix/store/fake/bin/nft".into(),
            pasta: "/nix/store/fake/bin/pasta".into(),
            sleep: "/nix/store/fake/bin/sleep".into(),
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
        guest_session: GuestSession {
            user: "vivarium".into(),
            home: "/home/vivarium".into(),
            shell: "/nix/store/fake/bin/bash".into(),
            path: "/run/wrappers/bin:/run/current-system/sw/bin".into(),
        },
        egress: EgressSpec {
            mode: LaunchEgressMode::Open,
            allow: Vec::new(),
        },
        network: NetworkSpec {
            tap_name: "viv-tap0".into(),
            gateway_address: "10.177.0.1".parse().unwrap(),
            prefix_length: 24,
            guest_address: "10.177.0.2".parse().unwrap(),
            dns_forward_address: "10.177.53.53".parse().unwrap(),
            guest_mac: "02:56:49:56:41:00".into(),
            resolver_port: 53,
        },
        shares: vec![ShareSpec {
            tag: "workspace".into(),
            source: std::env::current_dir().unwrap(),
            mount_point: "/run/vivarium-workspace".into(),
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
    let spec = fixture("console");
    tokio::fs::create_dir_all(&spec.runtime_paths.root)
        .await
        .unwrap();
    let listener = UnixListener::bind(&spec.runtime_paths.console_socket).unwrap();
    let reader = ConsoleReader::new();
    let mut tap = reader.subscribe();
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
    // The byte written only after the acknowledgement must still be seen: that
    // is the "attached before the first boot byte" half of the name, asserted
    // through the tap rather than assumed from the channel's ordering.
    peer.write_all(b"boot-after-connect").await.unwrap();
    assert_eq!(tap.recv().await.unwrap(), b"boot-after-connect");
    drop(peer);
    // And peer EOF ends the reader cleanly — the other half of the name.
    handle.await.unwrap().unwrap();
    tokio::fs::remove_dir_all(&spec.runtime_paths.root)
        .await
        .unwrap();
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
    // Every artefact a real boot leaves behind, not just the one vivarium writes
    // first. This test wrote only `launch_spec` while the allowlist was missing
    // cloud-hypervisor's `api.sock.lock`, so it passed on a set that never
    // included the entry that aborted every real cleanup. The list below is what
    // `tests/host/first-microvm-check` observed retained on a capable host; the
    // daemon-created names are the ones worth the duplication, because nothing
    // else in this crate names them.
    let paths = &spec.runtime_paths;
    let mut owned = vec![
        paths.launch_spec.clone(),
        paths.ready_socket.clone(),
        paths.api_socket.clone(),
        PathBuf::from(format!("{}.lock", paths.api_socket.display())),
        paths.console_socket.clone(),
        paths.console_log.clone(),
        PathBuf::from(format!("{}.1", paths.console_log.display())),
        PathBuf::from(format!("{}.2", paths.console_log.display())),
        paths.control_socket.clone(),
        paths.vm_pid.clone(),
        paths.boot_json.clone(),
        paths.vm_create_json.clone(),
    ];
    for share in &spec.shares {
        owned.push(share.socket.clone());
        owned.push(PathBuf::from(format!("{}.pid", share.socket.display())));
    }
    for path in &owned {
        tokio::fs::write(path, b"owned").await.unwrap();
    }
    Supervisor::new(spec.clone()).cleanup().await.unwrap();
    assert!(!spec.runtime_paths.root.exists());
    Supervisor::new(spec).cleanup().await.unwrap();
}

/// The sweep tolerates the startup lock and removes it under no circumstances.
///
/// `viv start --rebuild` and a session's stale-record repair both stop the old VM while holding the
/// per-target `flock` (spec/12 step 1), and this sweep is what that stop runs. `flock` is held
/// against an inode rather than a name, so unlinking the file here would leave the holder locked to
/// an inode nobody can reach while the next `viv` created a fresh `lock` and took it uncontended —
/// two processes inside a decision that exists to admit one. The lock outliving the sweep is what
/// makes the exclusion a property of the name.
#[tokio::test]
async fn cleanup_retains_the_startup_lock_and_its_directory() {
    let spec = fixture("held-lock");
    tokio::fs::create_dir_all(&spec.runtime_paths.root)
        .await
        .unwrap();
    tokio::fs::write(&spec.runtime_paths.launch_spec, b"owned")
        .await
        .unwrap();
    tokio::fs::write(&spec.runtime_paths.lock, b"")
        .await
        .unwrap();

    Supervisor::new(spec.clone()).cleanup().await.unwrap();

    assert!(
        spec.runtime_paths.lock.exists(),
        "the sweep unlinked a startup lock a caller may still hold"
    );
    assert!(
        !spec.runtime_paths.launch_spec.exists(),
        "retaining the lock must not retain the rest of the runtime artefacts"
    );
    assert!(
        spec.runtime_paths.root.exists(),
        "the directory holding the retained lock cannot be removed"
    );
    // Idempotent over the retained lock rather than only over an absent directory.
    Supervisor::new(spec.clone()).cleanup().await.unwrap();
    assert!(spec.runtime_paths.lock.exists());

    tokio::fs::remove_dir_all(&spec.runtime_paths.root)
        .await
        .unwrap();
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

/// A launch that fails reports `Failed` through the ready channel rather than
/// going silent — without the report, the launcher's only account of a failed
/// start is its own handoff timeout, thirty seconds later and naming no cause.
/// The send precedes the shutdown ladder and cleanup by code position (the
/// readiness socket cleanup unlinks lives in the runtime directory), which the
/// supervisor's own comment records; what this trial pins is the half a
/// refactor most plausibly loses, that the report is sent at all.
#[tokio::test]
async fn a_failed_launch_reports_failed_rather_than_going_silent() {
    let spec = fixture("failed-report");
    tokio::fs::create_dir_all(&spec.runtime_paths.root)
        .await
        .unwrap();
    let (ready_tx, mut ready_rx) = tokio::sync::mpsc::channel(1);
    // Every backend program is a fake store path, so the first spawn fails and
    // the run takes the failure path end to end: report, ladder, cleanup.
    let outcome = Supervisor::new(spec).run(ready_tx).await;
    assert!(outcome.is_err(), "fake store paths cannot launch");
    assert_eq!(ready_rx.try_recv(), Ok(LaunchReady::Failed));
}

/// The trial above stops at `Supervisor::run`'s channel; the regression this
/// path repairs lived one boundary further out, so this one crosses it: the
/// built supervisor binary, handed a spec whose first spawn fails, must land a
/// decodable `failed` report on the launcher's readiness listener — the report
/// that spares the launcher its full handoff wait — before it exits.
#[tokio::test]
async fn a_failed_launch_reports_failed_across_the_readiness_socket() {
    use std::os::unix::fs::PermissionsExt;
    let spec = fixture("failed-socket-report");
    tokio::fs::create_dir_all(&spec.runtime_paths.root)
        .await
        .unwrap();
    // The binary trusts only a private spec: 0o700 directory, 0o600 file, one
    // owner (`validate_metadata` in `src/bin/vivarium-supervisor.rs`).
    tokio::fs::set_permissions(
        &spec.runtime_paths.root,
        std::fs::Permissions::from_mode(0o700),
    )
    .await
    .unwrap();
    tokio::fs::write(
        &spec.runtime_paths.launch_spec,
        serde_json::to_vec(&spec).unwrap(),
    )
    .await
    .unwrap();
    tokio::fs::set_permissions(
        &spec.runtime_paths.launch_spec,
        std::fs::Permissions::from_mode(0o600),
    )
    .await
    .unwrap();
    let listener = UnixListener::bind(&spec.runtime_paths.ready_socket).unwrap();
    let status = tokio::process::Command::new(env!("CARGO_BIN_EXE_vivarium-supervisor"))
        .arg("--spec")
        .arg(&spec.runtime_paths.launch_spec)
        .arg("--ready-socket")
        .arg(&spec.runtime_paths.ready_socket)
        .status()
        .await
        .unwrap();
    assert!(!status.success(), "fake store paths cannot launch");
    // The report was sent before the exit just observed, so the connection is
    // already queued; the timeout only keeps a regression from hanging the run.
    let (mut peer, _) = tokio::time::timeout(Duration::from_secs(10), listener.accept())
        .await
        .unwrap()
        .unwrap();
    let mut bytes = Vec::new();
    peer.read_to_end(&mut bytes).await.unwrap();
    assert_eq!(
        ReadinessReport::decode(&bytes).ok(),
        Some(ReadinessReport::failed())
    );
    // The supervisor's own cleanup removes the runtime directory on the
    // failure path; this is only a backstop for an exit that left it behind.
    let _ = tokio::fs::remove_dir_all(&spec.runtime_paths.root).await;
}

#[tokio::test]
async fn detached() {
    let spec = fixture("detached");
    let rendered = TransientUnitSpec::new(&spec).command().rendered().join(" ");
    assert!(rendered.contains("--no-block"));
    assert!(rendered.contains("--service-type=exec"));
    // `mixed` and not `control-group`: the latter signals cloud-hypervisor directly, so
    // `systemctl stop` destroyed the VM before the supervisor could power the guest down and
    // uncommitted guest writes were lost. Asserted by name so the value cannot drift back.
    assert!(rendered.contains("KillMode=mixed"));
    assert!(!rendered.contains("KillMode=control-group"));
    assert!(!rendered.contains("--scope"));
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
