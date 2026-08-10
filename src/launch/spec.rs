//! Versioned, strict launch contract consumed from the Nix-built JSON handoff.

use crate::launch::LaunchError;
use crate::protocol::CredentialId;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::os::unix::fs::FileTypeExt;
use std::path::{Component, Path, PathBuf};

pub const LAUNCH_SCHEMA_VERSION: u32 = 2;
pub const VIRTIOFSD_RLIMIT_NOFILE: u64 = 524_288;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DescriptorBudget {
    pub limit: u64,
    pub worker_pool_size: u64,
}

impl DescriptorBudget {
    pub const PINNED_FIXED_RESERVE: u64 = 609;

    #[must_use]
    pub const fn effective_worker_count(self) -> u64 {
        self.worker_pool_size
    }

    /// Derive the descriptors available to guest activity after daemon reserve.
    ///
    /// # Errors
    ///
    /// Returns an error for arithmetic overflow or a non-positive allowance.
    pub fn guest_allowance(self) -> Result<u64, LaunchError> {
        let reserve = Self::PINNED_FIXED_RESERVE
            .checked_add(self.effective_worker_count())
            .ok_or(LaunchError::InvalidSpec("descriptor reserve overflow"))?;
        self.limit
            .checked_sub(reserve)
            .filter(|allowance| *allowance > 0)
            .ok_or(LaunchError::InvalidSpec(
                "descriptor limit does not exceed reserve",
            ))
    }
}

impl Default for DescriptorBudget {
    fn default() -> Self {
        Self {
            limit: VIRTIOFSD_RLIMIT_NOFILE,
            worker_pool_size: 0,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BackendPrograms {
    pub cloud_hypervisor: PathBuf,
    pub ch_remote: PathBuf,
    pub virtiofsd: PathBuf,
    pub setpriv: PathBuf,
    pub truncate: PathBuf,
    pub mkfs_ext4: PathBuf,
    pub systemd_run: PathBuf,
    pub supervisor: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RuntimePaths {
    pub root: PathBuf,
    pub launch_spec: PathBuf,
    pub ready_socket: PathBuf,
    pub api_socket: PathBuf,
    pub console_socket: PathBuf,
    pub console_log: PathBuf,
    pub control_socket: PathBuf,
    pub vm_pid: PathBuf,
    pub boot_json: PathBuf,
    pub vm_create_json: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CredentialSpec {
    pub id: CredentialId,
    pub host_socket: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SocketLegs {
    pub api: PathBuf,
    pub console: PathBuf,
    #[serde(default)]
    pub credentials: Vec<CredentialSpec>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BootMetadata {
    pub schema_version: u32,
    pub boot_identity: String,
    pub project_id: String,
    pub target: String,
    pub backend: String,
    pub workspace_host_path: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ShareSpec {
    pub tag: String,
    pub source: PathBuf,
    pub socket: PathBuf,
    pub cache: String,
    pub read_only: bool,
    #[serde(default)]
    pub extra_args: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VolumeSpec {
    pub label: String,
    pub path: PathBuf,
    // `MiB` is the unit's own capitalization, which is what the Nix producer writes and
    // what every size field in the launch handoff uses. serde's `camelCase` would derive
    // `sizeMib`, so the wire name is pinned here rather than left to the rename rule.
    #[serde(rename = "sizeMiB")]
    pub size_mib: u64,
    pub image_type: String,
    pub inode_ratio: Option<u64>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ResourceSpec {
    pub vcpus: u16,
    // See `VolumeSpec::size_mib`: the producer writes the unit's own capitalization.
    #[serde(rename = "memoryMiB")]
    pub memory_mib: u64,
    pub cpu_weight: u16,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct IdentityTranslation {
    pub guest_uid: u32,
    pub guest_gid: u32,
    pub host_uid: u32,
    pub host_gid: u32,
    pub overflow_uid: u32,
    pub overflow_gid: u32,
    pub id_max: u32,
}

#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LaunchSpec {
    pub schema_version: u32,
    pub project_id: String,
    pub target: String,
    pub runtime_paths: RuntimePaths,
    pub backend_programs: BackendPrograms,
    pub socket_legs: SocketLegs,
    pub resources: ResourceSpec,
    pub descriptor_budget: DescriptorBudget,
    pub identity_translation: IdentityTranslation,
    pub shares: Vec<ShareSpec>,
    pub volumes: Vec<VolumeSpec>,
    pub vm_create: Value,
    #[serde(default)]
    pub landlock_available: bool,
    #[serde(default = "default_console_log")]
    pub console_log_enabled: bool,
}

const fn default_console_log() -> bool {
    true
}

impl LaunchSpec {
    /// Deserialize and validate the strict internal launch schema.
    ///
    /// # Errors
    ///
    /// Returns an error when JSON decoding or any launch invariant fails.
    pub fn from_json(bytes: &[u8]) -> Result<Self, LaunchError> {
        let spec: Self = serde_json::from_slice(bytes)
            .map_err(|_| LaunchError::InvalidSpec("JSON does not match launch schema"))?;
        spec.validate()?;
        Ok(spec)
    }

    /// Validate paths, endpoints, resource arithmetic, and closed-policy inputs.
    ///
    /// # Errors
    ///
    /// Returns an error for any unsafe, unresolved, duplicated, or unsupported value.
    #[allow(clippy::too_many_lines)]
    pub fn validate(&self) -> Result<(), LaunchError> {
        if self.schema_version != LAUNCH_SCHEMA_VERSION {
            return Err(LaunchError::InvalidSpec("unsupported schema version"));
        }
        if self.project_id.is_empty() || self.target.is_empty() || self.shares.is_empty() {
            return Err(LaunchError::InvalidSpec(
                "project, target, and shares are required",
            ));
        }
        self.descriptor_budget.guest_allowance()?;
        for program in [
            &self.backend_programs.cloud_hypervisor,
            &self.backend_programs.ch_remote,
            &self.backend_programs.virtiofsd,
            &self.backend_programs.setpriv,
            &self.backend_programs.truncate,
            &self.backend_programs.mkfs_ext4,
            &self.backend_programs.systemd_run,
            &self.backend_programs.supervisor,
        ] {
            require_absolute_resolved(program)?;
            if !program.starts_with("/nix/store") {
                return Err(LaunchError::InvalidSpec(
                    "backend programs must be store paths",
                ));
            }
        }
        require_absolute_resolved(&self.runtime_paths.root)?;
        let runtime_paths = [
            &self.runtime_paths.launch_spec,
            &self.runtime_paths.ready_socket,
            &self.runtime_paths.api_socket,
            &self.runtime_paths.console_socket,
            &self.runtime_paths.console_log,
            &self.runtime_paths.control_socket,
            &self.runtime_paths.vm_pid,
            &self.runtime_paths.boot_json,
            &self.runtime_paths.vm_create_json,
        ];
        for path in runtime_paths {
            require_exact_child(path, &self.runtime_paths.root)?;
        }
        if self.socket_legs.api != self.runtime_paths.api_socket
            || self.socket_legs.console != self.runtime_paths.console_socket
        {
            return Err(LaunchError::InvalidSpec(
                "socket legs must name exact runtime sockets",
            ));
        }
        let mut sockets = HashSet::new();
        for socket in [
            &self.socket_legs.api,
            &self.socket_legs.console,
            &self.runtime_paths.control_socket,
        ] {
            if !sockets.insert(socket) {
                return Err(LaunchError::InvalidSpec("duplicate socket path"));
            }
        }
        let mut credential_ids = HashSet::new();
        for credential in &self.socket_legs.credentials {
            if !credential_ids.insert(credential.id) {
                return Err(LaunchError::InvalidSpec("duplicate credential id"));
            }
            require_absolute_resolved(&credential.host_socket)?;
            let metadata = std::fs::symlink_metadata(&credential.host_socket)
                .map_err(|error| LaunchError::io("inspect credential socket", error))?;
            if metadata.file_type().is_symlink() || !metadata.file_type().is_socket() {
                return Err(LaunchError::InvalidSpec(
                    "credential must name the exact socket object",
                ));
            }
            if !sockets.insert(&credential.host_socket) {
                return Err(LaunchError::InvalidSpec("duplicate socket path"));
            }
        }
        let mut tags = HashSet::new();
        for share in &self.shares {
            if !tags.insert(&share.tag) {
                return Err(LaunchError::InvalidSpec("duplicate share tag"));
            }
            require_absolute_resolved(&share.source)?;
            require_exact_child(&share.socket, &self.runtime_paths.root)?;
            if !sockets.insert(&share.socket) {
                return Err(LaunchError::InvalidSpec("duplicate socket path"));
            }
            reject_session_source(&share.source, &self.runtime_paths.root)?;
            let metadata = std::fs::metadata(&share.source)
                .map_err(|error| LaunchError::io("inspect share source", error))?;
            if !(metadata.is_dir() || metadata.is_file()) {
                return Err(LaunchError::InvalidSpec(
                    "share source is not a regular file or directory",
                ));
            }
            if !matches!(
                share.cache.as_str(),
                "auto" | "always" | "metadata" | "never"
            ) {
                return Err(LaunchError::InvalidSpec("unsupported share cache"));
            }
            if share.extra_args.iter().any(|arg| {
                [
                    "--socket-path",
                    "--shared-dir",
                    "--sandbox",
                    "--seccomp",
                    "--rlimit-nofile",
                    "--thread-pool-size",
                    "--inode-file-handles",
                ]
                .iter()
                .any(|fixed| arg == fixed || arg.starts_with(&format!("{fixed}=")))
            }) {
                return Err(LaunchError::InvalidSpec(
                    "extra share argument overrides mandatory policy",
                ));
            }
        }
        Ok(())
    }
}

fn require_absolute_resolved(path: &Path) -> Result<(), LaunchError> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(LaunchError::InvalidRuntimePath(
            "path must be absolute and normalized",
        ));
    }
    if path.to_string_lossy().contains('@') {
        return Err(LaunchError::InvalidRuntimePath("unresolved token"));
    }
    Ok(())
}

fn require_exact_child(path: &Path, root: &Path) -> Result<(), LaunchError> {
    require_absolute_resolved(path)?;
    if path.parent() != Some(root) {
        return Err(LaunchError::InvalidRuntimePath(
            "runtime artifact must be an exact child",
        ));
    }
    Ok(())
}

fn reject_session_source(source: &Path, runtime_root: &Path) -> Result<(), LaunchError> {
    for forbidden in [Path::new("/tmp"), Path::new("/var/tmp"), runtime_root] {
        if source.starts_with(forbidden) || forbidden.starts_with(source) {
            return Err(LaunchError::InvalidSpec("share source violates N24"));
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::type_complexity, clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture() -> LaunchSpec {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let source = std::env::current_dir().unwrap();
        let root = PathBuf::from(format!("/run/user/1000/vivarium/p/t-{unique}"));
        let child = |name: &str| root.join(name);
        LaunchSpec {
            schema_version: LAUNCH_SCHEMA_VERSION,
            project_id: "p".into(),
            target: "t".into(),
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
                cloud_hypervisor: "/nix/store/a/bin/cloud-hypervisor".into(),
                ch_remote: "/nix/store/a/bin/ch-remote".into(),
                virtiofsd: "/nix/store/b/bin/virtiofsd".into(),
                setpriv: "/nix/store/c/bin/setpriv".into(),
                truncate: "/nix/store/d/bin/truncate".into(),
                mkfs_ext4: "/nix/store/e/bin/mkfs.ext4".into(),
                systemd_run: "/nix/store/f/bin/systemd-run".into(),
                supervisor: "/nix/store/g/bin/vivarium-supervisor".into(),
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
                source,
                socket: child("workspace.sock"),
                cache: "auto".into(),
                read_only: false,
                extra_args: vec![],
            }],
            volumes: vec![],
            vm_create: serde_json::json!({"payload": {"kernel": "/nix/store/a/vmlinux"}}),
            landlock_available: false,
            console_log_enabled: true,
        }
    }

    #[test]
    fn validates_positive_absent_and_exact_credential_cases() {
        let mut spec = fixture();
        assert_eq!(spec.shares.len(), 1);
        spec.validate().unwrap();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let agent = std::env::temp_dir().join(format!("vivarium-agent-{unique}.sock"));
        let listener = std::os::unix::net::UnixListener::bind(&agent).unwrap();
        spec.socket_legs.credentials.push(CredentialSpec {
            id: CredentialId::Ssh,
            host_socket: agent.clone(),
        });
        spec.validate().unwrap();
        spec.socket_legs.credentials[0].host_socket = agent.parent().unwrap().to_path_buf();
        assert!(spec.validate().is_err());
        drop(listener);
        std::fs::remove_file(agent).unwrap();
    }

    #[test]
    fn rejects_mutated_contracts() {
        let mutations: Vec<Box<dyn Fn(&mut LaunchSpec)>> = vec![
            Box::new(|s| s.runtime_paths.api_socket = s.runtime_paths.console_socket.clone()),
            Box::new(|s| s.shares[0].socket = s.runtime_paths.api_socket.clone()),
            Box::new(|s| s.shares[0].source = s.runtime_paths.root.clone()),
            Box::new(|s| s.shares[0].source = PathBuf::from("@WORKSPACE_SOURCE@")),
            Box::new(|s| s.shares[0].extra_args.push("--sandbox=none".into())),
            Box::new(|s| {
                let duplicate = s.shares[0].clone();
                s.shares.push(duplicate);
            }),
        ];
        for mutate in mutations {
            let mut spec = fixture();
            assert_eq!(spec.shares.len(), 1);
            mutate(&mut spec);
            assert!(spec.validate().is_err());
        }
    }
}
