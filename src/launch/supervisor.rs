//! One cancellation tree owns every backend child and reader task.

use crate::launch::secure_fs::{self, SocketState};
use crate::launch::{
    BootMetadata, CommandSpec, ConfinementProfile, ConsoleReader, ConsoleSink, LaunchError,
    LaunchSpec,
};
use crate::protocol::validate_boot_identity;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;
use std::time::Duration;
use tokio::fs;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

const STARTUP_POLLS: usize = 400;
const POLL_INTERVAL: Duration = Duration::from_millis(25);
const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChildKind {
    Virtiofsd(String),
    Vmm,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChildExit {
    pub kind: ChildKind,
    pub status: Option<i32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShutdownReason {
    VmExit,
    ChildFailure(ChildExit),
    ExplicitStop,
    ProcessSignal,
    UnitStop,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LaunchReady {
    ProcessReady,
}

pub trait GuestReadiness: Send + Sync {
    fn wait<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<(), LaunchError>> + Send + 'a>>;
}

struct ManagedChild {
    kind: ChildKind,
    child: Child,
}

pub struct Supervisor {
    spec: LaunchSpec,
    cancellation: CancellationToken,
    tasks: JoinSet<Result<(), LaunchError>>,
    children: Vec<ManagedChild>,
}

impl Supervisor {
    #[must_use]
    pub fn new(spec: LaunchSpec) -> Self {
        Self {
            spec,
            cancellation: CancellationToken::new(),
            tasks: JoinSet::new(),
            children: Vec::new(),
        }
    }

    #[must_use]
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    /// Run create-before-boot launch and retain ownership until shutdown.
    ///
    /// # Errors
    ///
    /// Returns the first construction, readiness, child, task, or cleanup failure.
    pub async fn run(
        mut self,
        ready: mpsc::Sender<LaunchReady>,
    ) -> Result<ShutdownReason, LaunchError> {
        self.prepare_runtime().await?;
        if let Err(error) = self.run_inner(&ready).await {
            self.cancellation.cancel();
            let _ = self.shutdown_children().await;
            let cleanup = self.cleanup().await;
            return cleanup.and(Err(error));
        }
        let reason = match self.monitor().await {
            Ok(reason) => reason,
            Err(error) => {
                self.cancellation.cancel();
                let _ = self.shutdown_children().await;
                let cleanup = self.cleanup().await;
                return cleanup.and(Err(error));
            }
        };
        if !matches!(reason, ShutdownReason::VmExit) {
            self.cancellation.cancel();
        }
        let shutdown = self.shutdown_children().await;
        let cleanup = self.cleanup().await;
        shutdown?;
        cleanup?;
        Ok(reason)
    }

    async fn run_inner(&mut self, ready: &mpsc::Sender<LaunchReady>) -> Result<(), LaunchError> {
        self.provision_volumes().await?;
        let profile = ConfinementProfile::new(&self.spec)?;
        let vmm = profile.vmm();
        let shares = profile.shares();
        ConfinementProfile::validate_rendered(&vmm, &shares, profile.drops_bounding_set())?;
        for (share, command) in self.spec.shares.clone().into_iter().zip(shares) {
            self.spawn_child(ChildKind::Virtiofsd(share.tag), &command)?;
        }
        let share_sockets = self
            .spec
            .shares
            .iter()
            .map(|share| (share.socket.clone(), share.tag.clone()))
            .collect::<Vec<_>>();
        for (socket, tag) in share_sockets {
            self.wait_for_socket(&socket, Some(&tag)).await?;
        }
        self.spawn_child(ChildKind::Vmm, &vmm)?;
        self.write_vmm_pid().await?;
        let api_socket = self.spec.runtime_paths.api_socket.clone();
        self.wait_for_socket(&api_socket, None).await?;
        let metadata = self.prepare_boot_metadata().await?;
        self.write_vm_create_json().await?;
        self.write_boot_json(&metadata).await?;
        self.remote("create", Some(&self.spec.runtime_paths.vm_create_json))
            .await?;
        let console_socket = self.spec.runtime_paths.console_socket.clone();
        self.wait_for_socket(&console_socket, None).await?;
        let sink = ConsoleSink::open(
            &self.spec.runtime_paths.console_log,
            self.spec.console_log_enabled,
        )
        .await?;
        let reader = ConsoleReader::new();
        let (connected_tx, connected_rx) = oneshot::channel();
        let handle = reader.spawn(
            self.spec.runtime_paths.console_socket.clone(),
            sink,
            self.cancellation.clone(),
            connected_tx,
        );
        self.tasks
            .spawn(async move { handle.await.map_err(|_| LaunchError::Task)? });
        connected_rx
            .await
            .map_err(|_| LaunchError::Readiness("console connection"))?;
        self.remote("boot", None).await?;
        crate::launch::control::wait_for_agent(
            &self.spec.runtime_paths.control_socket,
            &metadata,
            STARTUP_TIMEOUT,
        )
        .await?;
        let credential_tasks = crate::launch::credentials::start(
            self.spec.runtime_paths.control_socket.clone(),
            &self.spec.socket_legs.credentials,
            self.cancellation.clone(),
            STARTUP_TIMEOUT,
        )
        .await?;
        for task in credential_tasks {
            self.tasks
                .spawn(async move { task.await.map_err(|_| LaunchError::Task)? });
        }
        ready
            .send(LaunchReady::ProcessReady)
            .await
            .map_err(|_| LaunchError::Readiness("initiator readiness channel"))?;
        Ok(())
    }

    async fn prepare_runtime(&self) -> Result<(), LaunchError> {
        self.spec.validate()?;
        secure_fs::private_dir(&self.spec.runtime_paths.root).await
    }

    async fn provision_volumes(&self) -> Result<(), LaunchError> {
        for volume in &self.spec.volumes {
            if fs::try_exists(&volume.path)
                .await
                .map_err(|error| LaunchError::io("inspect volume", error))?
            {
                continue;
            }
            run_checked(CommandSpec::new(
                self.spec.backend_programs.truncate.clone(),
                vec![
                    "-s".into(),
                    format!("{}M", volume.size_mib),
                    volume.path.display().to_string(),
                ],
            ))
            .await?;
            let mut args = vec!["-q".into(), "-L".into(), volume.label.clone()];
            if let Some(ratio) = volume.inode_ratio {
                args.extend(["-i".into(), ratio.to_string()]);
            }
            args.push(volume.path.display().to_string());
            run_checked(CommandSpec::new(
                self.spec.backend_programs.mkfs_ext4.clone(),
                args,
            ))
            .await?;
        }
        Ok(())
    }

    fn spawn_child(&mut self, kind: ChildKind, spec: &CommandSpec) -> Result<(), LaunchError> {
        let mut command = Command::new(spec.program());
        command
            .args(spec.args())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command
            .spawn()
            .map_err(|error| LaunchError::io("spawn supervised child", error))?;
        if let Some(stdout) = child.stdout.take() {
            self.tasks.spawn(drain(stdout));
        }
        if let Some(stderr) = child.stderr.take() {
            self.tasks.spawn(drain(stderr));
        }
        self.children.push(ManagedChild { kind, child });
        Ok(())
    }

    async fn wait_for_socket(
        &mut self,
        path: &Path,
        share_tag: Option<&str>,
    ) -> Result<(), LaunchError> {
        for _ in 0..STARTUP_POLLS {
            if socket_exists(path).await? {
                return Ok(());
            }
            for managed in &mut self.children {
                if let Some(status) = managed
                    .child
                    .try_wait()
                    .map_err(|error| LaunchError::io("poll child", error))?
                {
                    let relevant = match (&managed.kind, share_tag) {
                        (ChildKind::Virtiofsd(tag), Some(expected)) => tag == expected,
                        (ChildKind::Vmm, None) => true,
                        _ => false,
                    };
                    if relevant {
                        return Err(LaunchError::ChildExit {
                            kind: "startup child",
                            status: status.code(),
                        });
                    }
                }
            }
            tokio::select! {
                () = self.cancellation.cancelled() => return Err(LaunchError::Cancelled),
                () = tokio::time::sleep(POLL_INTERVAL) => {}
            }
        }
        Err(LaunchError::Readiness("backend socket"))
    }

    async fn remote(
        &self,
        operation: &'static str,
        payload: Option<&Path>,
    ) -> Result<(), LaunchError> {
        let mut args = vec![
            "--api-socket".into(),
            self.spec.runtime_paths.api_socket.display().to_string(),
            operation.into(),
        ];
        if let Some(path) = payload {
            args.push(path.display().to_string());
        }
        run_checked(CommandSpec::new(
            self.spec.backend_programs.ch_remote.clone(),
            args,
        ))
        .await
    }

    async fn prepare_boot_metadata(&mut self) -> Result<BootMetadata, LaunchError> {
        let boot_identity = fs::read_to_string("/proc/sys/kernel/random/uuid")
            .await
            .map_err(|error| LaunchError::io("read boot identity", error))?;
        let boot_identity = boot_identity.trim();
        validate_boot_identity(boot_identity)
            .map_err(|_| LaunchError::InvalidSpec("kernel UUID is malformed"))?;
        let cmdline = self
            .spec
            .vm_create
            .get_mut("payload")
            .and_then(|payload| payload.get_mut("cmdline"))
            .and_then(|value| value.as_str())
            .ok_or(LaunchError::InvalidSpec("VM create cmdline is missing"))?;
        let cmdline = format!("{cmdline} vivarium.boot_identity={boot_identity}");
        self.spec.vm_create["payload"]["cmdline"] = serde_json::Value::String(cmdline);
        let workspace_host_path = self
            .spec
            .shares
            .iter()
            .find(|share| share.tag == "workspace")
            .map(|share| share.source.clone())
            .ok_or(LaunchError::InvalidSpec("workspace share is missing"))?;
        Ok(BootMetadata {
            schema_version: crate::protocol::SCHEMA_VERSION,
            boot_identity: boot_identity.to_owned(),
            project_id: self.spec.project_id.clone(),
            target: self.spec.target.clone(),
            backend: "cloud-hypervisor".to_owned(),
            workspace_host_path,
        })
    }

    async fn write_boot_json(&self, metadata: &BootMetadata) -> Result<(), LaunchError> {
        secure_fs::private_write(
            &self.spec.runtime_paths.boot_json,
            &serde_json::to_vec(metadata)
                .map_err(|_| LaunchError::InvalidSpec("boot metadata cannot be serialized"))?,
        )
        .await
    }

    async fn write_vm_create_json(&self) -> Result<(), LaunchError> {
        secure_fs::private_write(
            &self.spec.runtime_paths.vm_create_json,
            &serde_json::to_vec(&self.spec.vm_create)
                .map_err(|_| LaunchError::InvalidSpec("VM create JSON cannot be serialized"))?,
        )
        .await
    }

    async fn write_vmm_pid(&self) -> Result<(), LaunchError> {
        let pid = self
            .children
            .iter()
            .find(|child| matches!(child.kind, ChildKind::Vmm))
            .and_then(|child| child.child.id())
            .ok_or(LaunchError::Task)?;
        secure_fs::private_write(&self.spec.runtime_paths.vm_pid, pid.to_string().as_bytes()).await
    }

    async fn monitor(&mut self) -> Result<ShutdownReason, LaunchError> {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .map_err(|error| LaunchError::io("install signal handler", error))?;
        loop {
            for managed in &mut self.children {
                if let Some(status) = managed
                    .child
                    .try_wait()
                    .map_err(|error| LaunchError::io("poll child", error))?
                {
                    let exit = ChildExit {
                        kind: managed.kind.clone(),
                        status: status.code(),
                    };
                    return Ok(if matches!(managed.kind, ChildKind::Vmm) {
                        ShutdownReason::VmExit
                    } else {
                        ShutdownReason::ChildFailure(exit)
                    });
                }
            }
            tokio::select! {
                () = self.cancellation.cancelled() => return Ok(ShutdownReason::ExplicitStop),
                result = tokio::signal::ctrl_c() => {
                    result.map_err(|error| {
                        LaunchError::io("wait for process signal", error)
                    })?;
                    return Ok(ShutdownReason::ProcessSignal);
                },
                _ = terminate.recv() => return Ok(ShutdownReason::UnitStop),
                result = self.tasks.join_next(), if !self.tasks.is_empty() => {
                    match result {
                        Some(Ok(Ok(()))) => {
                            return Ok(ShutdownReason::ChildFailure(ChildExit {
                                kind: ChildKind::Vmm,
                                status: None,
                            }));
                        }
                        Some(_) => return Err(LaunchError::Task),
                        None => {}
                    }
                }
                () = tokio::time::sleep(POLL_INTERVAL) => {}
            }
        }
    }

    async fn shutdown_children(&mut self) -> Result<(), LaunchError> {
        let _ = self.remote("shutdown", None).await;
        let deadline = tokio::time::Instant::now() + SHUTDOWN_TIMEOUT;
        loop {
            let mut alive = false;
            for managed in &mut self.children {
                alive |= managed
                    .child
                    .try_wait()
                    .map_err(|error| LaunchError::io("poll shutdown", error))?
                    .is_none();
            }
            if !alive || tokio::time::Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
        for managed in &mut self.children {
            if managed
                .child
                .try_wait()
                .map_err(|error| LaunchError::io("poll shutdown", error))?
                .is_none()
            {
                managed
                    .child
                    .start_kill()
                    .map_err(|error| LaunchError::io("kill child", error))?;
            }
            let _ = managed
                .child
                .wait()
                .await
                .map_err(|error| LaunchError::io("reap child", error))?;
        }
        self.cancellation.cancel();
        while let Some(result) = self.tasks.join_next().await {
            let _ = result;
        }
        Ok(())
    }

    /// Remove only artifacts declared by this launch.
    ///
    /// # Errors
    ///
    /// Returns an error rather than removing the directory when an unknown entry exists.
    pub async fn cleanup(&self) -> Result<(), LaunchError> {
        cleanup_runtime(&self.spec).await
    }
}

async fn run_checked(spec: CommandSpec) -> Result<(), LaunchError> {
    let status = Command::new(spec.program())
        .args(spec.args())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map_err(|error| LaunchError::io("run launch helper", error))?;
    if status.success() {
        Ok(())
    } else {
        Err(LaunchError::ChildExit {
            kind: "launch helper",
            status: status.code(),
        })
    }
}

async fn drain<R: AsyncRead + Unpin>(mut reader: R) -> Result<(), LaunchError> {
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .await
            .map_err(|error| LaunchError::io("drain child stream", error))?;
        if read == 0 {
            return Ok(());
        }
    }
}

async fn socket_exists(path: &Path) -> Result<bool, LaunchError> {
    Ok(secure_fs::socket_state(path).await? == SocketState::Socket)
}

async fn cleanup_runtime(spec: &LaunchSpec) -> Result<(), LaunchError> {
    let mut allowed = vec![
        spec.runtime_paths.launch_spec.clone(),
        spec.runtime_paths.ready_socket.clone(),
        spec.runtime_paths.api_socket.clone(),
        spec.runtime_paths.console_socket.clone(),
        spec.runtime_paths.control_socket.clone(),
        spec.runtime_paths.console_log.clone(),
        spec.runtime_paths.vm_pid.clone(),
        spec.runtime_paths.boot_json.clone(),
        spec.runtime_paths.vm_create_json.clone(),
        PathBuf::from(format!("{}.1", spec.runtime_paths.console_log.display())),
        PathBuf::from(format!("{}.2", spec.runtime_paths.console_log.display())),
    ];
    for share in &spec.shares {
        allowed.push(share.socket.clone());
        allowed.push(PathBuf::from(format!("{}.pid", share.socket.display())));
    }
    let mut entries = match fs::read_dir(&spec.runtime_paths.root).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(LaunchError::io("read runtime directory", error)),
    };
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|error| LaunchError::io("read runtime entry", error))?
    {
        let path = entry.path();
        if !allowed.contains(&path) {
            return Err(LaunchError::UnknownRuntimeArtifact(path));
        }
    }
    for path in allowed {
        match fs::remove_file(path).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(LaunchError::io("remove runtime artifact", error)),
        }
    }
    fs::remove_dir(&spec.runtime_paths.root)
        .await
        .map_err(|error| LaunchError::io("remove runtime directory", error))
}
