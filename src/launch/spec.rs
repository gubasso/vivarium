//! Versioned, strict launch contract consumed from the Nix-built JSON handoff.

use crate::launch::LaunchError;
use crate::launch::mounts::encode_entry;
use crate::protocol::CredentialId;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::os::unix::fs::FileTypeExt;
use std::path::{Component, Path, PathBuf};

/// The launch handoff's own version, bumped to 6 by guest networking.
///
/// 6 adds the `egress` and `network` objects and six backend programs (`unshare`, `nsenter`,
/// `ip`, `nft`, `pasta`, `sleep`): the supervisor now creates a per-VM namespace pair, configures
/// a tap, and — under allowlist mode — applies a ruleset and spawns the gating resolver, none of
/// which an older handoff describes. 5 was named volumes: the launcher takes one `--volume-dir`
/// and joins each image path from a name the build owns. It caught the same class at 4
/// (ADR-0100): `ShareSpec::mount_point` for the workspace stopped being the path a session starts
/// in and became the share's internal one, and both shapes parse, so the version is the only
/// thing that separates them.
///
/// It does not separate every pairing. The runner's own argument names moved with the bump to 5,
/// and an old build kept reachable on purpose — an old generation, `--no-rebuild` — may still be
/// hand-invoked, refusing at its usage line before rendering any JSON for this constant to check.
/// `viv` itself reads the built output's `share/vivarium/launch-contract-schema` against this
/// constant before boot (spec/10), so the ordinary path refuses with both numbers named.
///
/// 7 since the installation supplies vivarium (ADR-0102): the supervisor left the build-side
/// JSON and enters the rendered specification from the running installation, and the runner
/// stopped exec-ing a built `viv`.
///
/// 8 since declared mounts reach the guest (slice 019): the share list stopped being a fixed
/// pair. Every share's socket token took the one `@SHARE_SOCKET_<TAG>@` shape, and a declared
/// mount's share carries a [`MountPlan`] — whether the declared source was a directory or a
/// regular file served through its parent — which the supervisor relays to the guest's bind
/// unit on the kernel command line.
///
/// 9 since a file mount serves only its file (ADR-0105): under a file plan, `source` stopped
/// meaning the parent directory and became the declared file itself, which the supervisor stages
/// as the only entry of that share's export root. The field kept its name and changed its
/// meaning, which is exactly the skew this constant exists to catch — a schema-8 record paired
/// with this code would hand a daemon a directory where a file is required, and the pairing is
/// refused before that can happen.
pub const LAUNCH_SCHEMA_VERSION: u32 = 9;
pub const VIRTIOFSD_RLIMIT_NOFILE: u64 = 524_288;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DescriptorBudget {
    pub limit: u64,
    pub worker_pool_size: u64,
}

impl DescriptorBudget {
    pub const PINNED_FIXED_RESERVE: u64 = 609;

    /// The worker count the daemon actually charges against the reserve.
    ///
    /// virtiofsd computes `INTERNAL_FD_RESERVE + max(thread_pool_size, 1)`, because a
    /// thread opens descriptors before it is accounted for. A declared pool of `0` — which
    /// is what ADR-0096 fixes — therefore still costs one descriptor, so the raw field is
    /// not the effective count and using it claims one more descriptor for the guest than
    /// the daemon leaves. Verified against v1.13.3 and v1.14.0, which agree.
    #[must_use]
    pub const fn effective_worker_count(self) -> u64 {
        if self.worker_pool_size == 0 {
            1
        } else {
            self.worker_pool_size
        }
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
    /// Creates and holds the per-VM user+net namespace pair (spec/05).
    pub unshare: PathBuf,
    /// Joins the pair by the holder's pid for every in-namespace step.
    pub nsenter: PathBuf,
    /// Configures the tap inside the pair, as launch-time one-shots.
    pub ip: PathBuf,
    /// Applies the egress ruleset and its per-answer elements under allowlist mode.
    pub nft: PathBuf,
    /// The unprivileged uplink process, one per VM, connecting the namespace to the
    /// host's network.
    pub pasta: PathBuf,
    /// The holder's body: keeps the pair referenced for the VM's lifetime.
    pub sleep: PathBuf,
}

/// The launch half of `sandbox.egress`: the mode and the destinations the build
/// channel declared, carried across so host-side enforcement needs no evaluation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EgressSpec {
    pub mode: LaunchEgressMode,
    #[serde(default)]
    pub allow: Vec<String>,
}

/// spec/05's two egress modes, as the launch handoff spells them.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LaunchEgressMode {
    Open,
    Allowlist,
}

/// The addressing the namespace, the guest, and the resolver agree on.
///
/// Owned by the build (`nix/default.nix` names the values once) and carried here so
/// the supervisor's tap setup and the guest's interface configuration are two
/// readers of one declaration rather than two spellings of it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct NetworkSpec {
    /// The tap the VMM opens by name, inside the pair.
    pub tap_name: String,
    /// The namespace side of the guest link — the guest's gateway, and the address
    /// the gating resolver serves on under allowlist mode.
    pub gateway_address: std::net::IpAddr,
    /// The guest link's prefix length.
    pub prefix_length: u8,
    /// The guest's own address, configured by the guest image.
    pub guest_address: std::net::IpAddr,
    /// The address the uplink maps to the host's own resolver. Deliberately outside
    /// the guest link's subnet, so the guest routes it through the gateway instead
    /// of asking the local link for a neighbour that does not exist.
    pub dns_forward_address: std::net::IpAddr,
    /// The guest NIC's MAC, fixed so the guest's interface match is stable.
    pub guest_mac: String,
    /// The gating resolver's UDP port on the gateway address.
    pub resolver_port: u16,
}

impl NetworkSpec {
    /// The gateway address in CIDR notation, as `ip addr add` takes it.
    #[must_use]
    pub fn gateway_cidr(&self) -> String {
        format!("{}/{}", self.gateway_address, self.prefix_length)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RuntimePaths {
    pub root: PathBuf,
    pub launch_spec: PathBuf,
    /// The per-target startup lock spec/12 places here.
    ///
    /// Declared rather than derived by whoever takes it, because the supervisor's teardown sweep
    /// refuses any runtime-directory entry it was not told about — and refuses the whole sweep, not
    /// the one file. A lock nobody declared would turn every `stop` into an incomplete teardown.
    pub lock: PathBuf,
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

/// The one backend a launch uses today (ADR-0025).
///
/// Named once because two sides read it: the supervisor stamps it into the boot record, and
/// spec/12 step 3 compares that record against what this invocation expects. A second literal
/// would let the writer and the comparison drift into a reuse check that can never fail.
pub const BACKEND: &str = "cloud-hypervisor";

/// The permissive pre-read every record parser runs first.
///
/// Only the version, with unknown fields tolerated, so a launch or boot record written by any
/// vivarium version yields its `schemaVersion` to every other version forever — which is what
/// lets a mismatch be diagnosed as skew, with both numbers named, instead of collapsing into
/// the corruption case (spec/14: a channel that fails is `74`/`69`; a record read whose
/// generation this binary cannot accept is `78`). The strict `deny_unknown_fields` types below
/// are reached only when the number matches.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionEnvelope {
    pub schema_version: u32,
}

impl VersionEnvelope {
    /// Whether the record's generation is this binary's own.
    #[must_use]
    pub const fn current(self) -> bool {
        self.schema_version == LAUNCH_SCHEMA_VERSION
    }
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
    /// Where the guest's own `/etc/fstab` mounts this share.
    ///
    /// The guest module owns the value, which is what lets `tests/nix/contract.sh` compare every
    /// share against the guest's fstab as a total check rather than an exception list.
    ///
    /// For the workspace this is no longer the path a session starts in. N16 puts the project at
    /// the host's own absolute path, this field is build output, and N19 keeps a host path out of
    /// one — so the share mounts here and the guest binds it at `source` (ADR-0100).
    pub mount_point: PathBuf,
    pub socket: PathBuf,
    pub cache: String,
    pub read_only: bool,
    /// Present exactly on the shares that came from declared mounts (spec/06, ADR-0020).
    ///
    /// `viv` decided it at launch when it expanded the declared source: a directory is served
    /// whole, a regular file is served through its parent directory with `entry` naming the one
    /// file inside the share the guest binds at the target. The supervisor relays the plan on the
    /// kernel command line as `vivarium.mount.<tag>=...`; the target itself is baked into the
    /// guest's own bind table and never crosses here.
    #[serde(default)]
    pub mount_plan: Option<MountPlan>,
    #[serde(default)]
    pub extra_args: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MountPlanKind {
    Dir,
    File,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MountPlan {
    pub kind: MountPlanKind,
    /// The percent-encoded basename a `file` mount serves, kept encoded end to end: the guest's
    /// bind unit is the one decoder, checking the allowlist before decoding (`mount-bind.sh`).
    pub entry: Option<String>,
}

impl ShareSpec {
    /// Where this share's daemon finds its export root, for the shares that are staged.
    ///
    /// A file mount's daemon is never pointed at the declared file's parent (ADR-0105): the
    /// supervisor gives it a private directory holding that one file and nothing else. The path is
    /// derived rather than declared, and derived in exactly one place, because two parties need to
    /// agree on it and disagreeing is silent — the confinement profile renders it into the daemon's
    /// command, and the teardown sweep, which refuses a runtime-directory entry nobody named, has
    /// to recognise the same name.
    #[must_use]
    pub fn stage_dir(&self, runtime_root: &Path) -> Option<PathBuf> {
        matches!(
            self.mount_plan,
            Some(MountPlan {
                kind: MountPlanKind::File,
                ..
            })
        )
        .then(|| runtime_root.join(format!("{}.stage", self.tag)))
    }
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

/// What a guest process gets that no host variable could supply.
///
/// spec/12 makes host passthrough deny-by-default, which settles what may *cross* from the host and
/// says nothing about what the guest itself provides. These four are the latter: facts of the guest
/// image that a process started through the control socket would otherwise not have, because the
/// agent clears the environment before every spawn and inherits nothing.
///
/// `PATH` is the load-bearing one. Without it a non-login `exec` cannot resolve a bare program name
/// at all, so `viv exec -- true` would fail to spawn while looking like a missing guest tool.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GuestSession {
    /// The default user a session runs as, which is the one the agent's own unit runs as.
    pub user: String,
    pub home: PathBuf,
    pub shell: PathBuf,
    /// Rendered from the guest's own profile list, so a profile added to the image reaches a
    /// session rather than only an interactive login.
    pub path: String,
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
    pub guest_session: GuestSession,
    pub egress: EgressSpec,
    pub network: NetworkSpec,
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

/// The share whose mount point a session starts in, and whose host path `boot.json` records.
pub const WORKSPACE_SHARE_TAG: &str = "workspace";

impl LaunchSpec {
    /// The workspace share, by the one tag both halves of the launch agree on.
    #[must_use]
    pub fn workspace_share(&self) -> Option<&ShareSpec> {
        self.shares
            .iter()
            .find(|share| share.tag == WORKSPACE_SHARE_TAG)
    }

    /// The guest path the workspace occupies, which the mirror makes equal to the host's own.
    ///
    /// Derived rather than declared. The guest binds the share at the path the launcher put on the
    /// kernel command line, and that path is this share's `source`; a second field would be a
    /// second spelling of one value that has to agree with itself byte for byte.
    #[must_use]
    pub fn workspace_guest_path(&self) -> Option<&Path> {
        self.workspace_share().map(|share| share.source.as_path())
    }

    /// Deserialize and validate the strict internal launch schema.
    ///
    /// The permissive [`VersionEnvelope`] read comes first, so a record another vivarium version
    /// wrote is reported as [`LaunchError::SchemaSkew`] with both numbers named rather than
    /// collapsing into the strict parser's corruption case (spec/10). A record that yields no
    /// envelope at all was written by nothing this tool ever shipped, and stays corruption.
    ///
    /// # Errors
    ///
    /// Returns an error when JSON decoding or any launch invariant fails.
    pub fn from_json(bytes: &[u8]) -> Result<Self, LaunchError> {
        if let Ok(envelope) = serde_json::from_slice::<VersionEnvelope>(bytes)
            && !envelope.current()
        {
            return Err(LaunchError::SchemaSkew {
                record: envelope.schema_version,
                current: LAUNCH_SCHEMA_VERSION,
            });
        }
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
            // The belt behind `from_json`'s envelope pre-read, for a spec built in memory.
            return Err(LaunchError::SchemaSkew {
                record: self.schema_version,
                current: LAUNCH_SCHEMA_VERSION,
            });
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
            &self.backend_programs.unshare,
            &self.backend_programs.nsenter,
            &self.backend_programs.ip,
            &self.backend_programs.nft,
            &self.backend_programs.pasta,
            &self.backend_programs.sleep,
        ] {
            require_absolute_resolved(program)?;
            if !program.starts_with("/nix/store") {
                return Err(LaunchError::InvalidSpec(
                    "backend programs must be store paths",
                ));
            }
        }
        // The supervisor is the one program the build does not supply: it comes from the running
        // installation (ADR-0102), named by the invoking `viv`, so it is held to absolute and
        // resolved but not to the store.
        require_absolute_resolved(&self.backend_programs.supervisor)?;
        // The grammar backstop for a hand-written or stale handoff: the manifest
        // refused malformed entries first and with a diagnostic, and under allowlist
        // mode the resolver and the filter both parse this list again to enforce it.
        if crate::net::allowlist::Allowlist::parse(&self.egress.allow).is_err() {
            return Err(LaunchError::InvalidSpec(
                "egress allowlist entry is malformed",
            ));
        }
        if self.network.tap_name.is_empty()
            || self.network.tap_name.len() > 15
            || !self
                .network
                .tap_name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            // 15 is IFNAMSIZ minus the terminator; a longer name fails at `ip` with
            // less to say.
            return Err(LaunchError::InvalidSpec(
                "tap name must be a short alphanumeric-and-dash identifier",
            ));
        }
        if self.network.prefix_length == 0 || self.network.prefix_length > 30 {
            return Err(LaunchError::InvalidSpec(
                "network prefix must leave room for a gateway and a guest",
            ));
        }
        if self.network.gateway_address.is_ipv4() != self.network.guest_address.is_ipv4() {
            return Err(LaunchError::InvalidSpec(
                "gateway and guest addresses must share a family",
            ));
        }
        if self.network.resolver_port == 0 {
            return Err(LaunchError::InvalidSpec("resolver port must be non-zero"));
        }
        // The MAC goes verbatim into the VMM's device arguments and the guest's
        // interface match; a malformed one would surface as the VMM's own parse
        // error mid-boot, with this spec long out of the picture.
        if !is_mac_address(&self.network.guest_mac) {
            return Err(LaunchError::InvalidSpec(
                "guest MAC must be six colon-separated hex octets",
            ));
        }
        require_absolute_resolved(&self.runtime_paths.root)?;
        let runtime_paths = [
            &self.runtime_paths.launch_spec,
            &self.runtime_paths.lock,
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
        // An empty `PATH` is the one value here that fails silently: the guest spawns, resolves
        // nothing, and reports a missing program rather than a missing environment.
        if self.guest_session.user.is_empty() || self.guest_session.path.is_empty() {
            return Err(LaunchError::InvalidSpec(
                "guest session user and path are required",
            ));
        }
        require_absolute_resolved(&self.guest_session.home)?;
        require_absolute_resolved(&self.guest_session.shell)?;
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
            require_absolute_resolved(&share.mount_point)?;
            require_exact_child(&share.socket, &self.runtime_paths.root)?;
            // The staging directory is held to the same rule as the socket beside it, and for a
            // sharper reason: it is derived from the tag rather than carried, so a tag holding a
            // separator or a leading `/` would have `Path::join` name a directory outside the
            // runtime root — one the supervisor then creates and the teardown sweep then removes.
            // The build side already constrains a tag to `[a-z0-9]+` (`nix/guest.nix`); this
            // validator exists for the record no build wrote.
            if let Some(stage) = share.stage_dir(&self.runtime_paths.root) {
                require_exact_child(&stage, &self.runtime_paths.root)?;
            }
            if !sockets.insert(&share.socket) {
                return Err(LaunchError::InvalidSpec("duplicate socket path"));
            }
            reject_session_source(&share.source, &self.runtime_paths.root)?;
            // The contract backstop, not the diagnostic: `src/cli/lifecycle.rs` refuses the same
            // paths first and with an explanation, and this catches a hand-written or stale
            // specification that walked past it.
            if share.tag == WORKSPACE_SHARE_TAG
                && let Some(reason) = unmirrorable(&share.source)
            {
                return Err(LaunchError::InvalidSpec(reason));
            }
            // A share source is a directory, except under a file mount plan, where it is the
            // declared regular file itself and the supervisor stages it as the only entry of the
            // share's export root (spec/06, ADR-0071, ADR-0105). The authoritative type check
            // reads through the descriptor held across the spawn (`src/launch/supervisor.rs`);
            // this is the spec-shaped backstop for a hand-written record.
            let metadata = std::fs::metadata(&share.source)
                .map_err(|error| LaunchError::io("inspect share source", error))?;
            let staged = matches!(
                share.mount_plan,
                Some(MountPlan {
                    kind: MountPlanKind::File,
                    ..
                })
            );
            if staged {
                if !metadata.is_file() {
                    return Err(LaunchError::InvalidSpec(
                        "a file mount plan's share source is not a regular file",
                    ));
                }
            } else if !metadata.is_dir() {
                return Err(LaunchError::InvalidSpec("share source is not a directory"));
            }
            match &share.mount_plan {
                None
                | Some(MountPlan {
                    kind: MountPlanKind::Dir,
                    entry: None,
                }) => {}
                Some(MountPlan {
                    kind: MountPlanKind::Dir,
                    entry: Some(_),
                }) => {
                    return Err(LaunchError::InvalidSpec(
                        "a directory mount plan carries no entry",
                    ));
                }
                Some(MountPlan {
                    kind: MountPlanKind::File,
                    entry,
                }) => {
                    if !entry.as_deref().is_some_and(is_encoded_entry) {
                        return Err(LaunchError::InvalidSpec(
                            "a file mount plan needs a well-formed encoded entry",
                        ));
                    }
                    // One file, named twice: the staging step reads the export root's only entry
                    // from the source's own basename (`stage-share`), while the guest binds the
                    // entry this plan carries. Nothing downstream reconciles the two, so a record
                    // whose spellings disagree stages one name and asks the guest for another,
                    // and the mount unit refuses at boot for a reason that looks like a missing
                    // file. Both spellings are in hand here, so the disagreement is refused here.
                    if share.source.file_name().map(encode_entry).as_deref() != entry.as_deref() {
                        return Err(LaunchError::InvalidSpec(
                            "a file mount plan's entry does not name its source",
                        ));
                    }
                }
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
        // Volumes were unchecked while there were exactly two of them and the host named both
        // paths on the command line. Since the launcher joins each one from `--volume-dir` and a
        // build decides how many there are, a defect in that join reaches here instead of a
        // developer. The duplicate-label check is the load-bearing one: the guest mounts by
        // `/dev/disk/by-label`, so two volumes sharing a label is not an error at boot but the
        // wrong data at the right path, which is what the disk-ordering contract in
        // `nix/guest.nix` exists to prevent.
        let mut labels = HashSet::new();
        let mut images = HashSet::new();
        for volume in &self.volumes {
            require_absolute_resolved(&volume.path)?;
            if volume.label.is_empty() {
                return Err(LaunchError::InvalidSpec("volume label is empty"));
            }
            if !labels.insert(&volume.label) {
                return Err(LaunchError::InvalidSpec("duplicate volume label"));
            }
            if !images.insert(&volume.path) {
                return Err(LaunchError::InvalidSpec("duplicate volume image path"));
            }
        }
        Ok(())
    }
}

/// Whether `mac` is exactly six colon-separated hex octets, e.g. `02:56:49:56:41:00`.
fn is_mac_address(mac: &str) -> bool {
    let bytes = mac.as_bytes();
    bytes.len() == 17
        && bytes.chunks(3).all(|octet| {
            octet[0].is_ascii_hexdigit()
                && octet[1].is_ascii_hexdigit()
                && (octet.len() == 2 || octet[2] == b':')
        })
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
    if contains_unresolved_token(&path.to_string_lossy()) {
        return Err(LaunchError::InvalidRuntimePath("unresolved token"));
    }
    Ok(())
}

/// Whether a rendered path still carries one of `runner.sh`'s `@NAME@` substitution sentinels.
///
/// Matched as the sentinel shape rather than as a bare `@`, which is what this was. Since ADR-0100
/// the workspace share's source is also the guest's mount path, so a project directory holding an
/// `@` — a version-pinned checkout such as `pkg@1.0` is the ordinary case — would be refused here
/// and the refusal would read as a mirroring defect rather than as the false positive it is.
fn contains_unresolved_token(rendered: &str) -> bool {
    let mut rest = rendered;
    while let Some(open) = rest.find('@') {
        let after = &rest[open + 1..];
        match after.find('@') {
            // Every sentinel `launch-arguments.nix` emits is upper-case ASCII with underscores
            // and digits — a share's socket token embeds its tag, and `mnt0` upper-cases to
            // `MNT0` — and is never empty, so `a@b@c` is a filename and `@SHARE_SOCKET_MNT0@`
            // is a leftover.
            Some(close)
                if close > 0
                    && after[..close].bytes().all(|byte| {
                        byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_'
                    }) =>
            {
                return true;
            }
            Some(close) => rest = &after[close..],
            None => return false,
        }
    }
    false
}

/// Guest paths the image owns, which a mirrored workspace may neither occupy nor contain.
///
/// The guest unit re-derives the same answer from the booted image and is the authority; this list
/// exists so the refusal carries a diagnostic before a VM is started, and so a generation whose
/// `/etc` is not a mount of its own still refuses `/etc/...`.
///
/// Two whole subtrees are deliberately absent, and both for the same reason: a host keeps real
/// projects under them, so a blanket denial refuses ordinary work. `/var`, because ostree-based
/// hosts put home directories at `/var/home/<user>`. And `/run`, because a removable drive is
/// mounted at `/run/media/<user>/<label>` on an ordinary desktop — measured, by this repository's
/// own acceptance fixtures, which live on exactly such a drive and were refused by the first
/// version of this list. What the guest owns under those two roots is named individually instead.
const GUEST_OWNED_PATHS: &[&str] = &[
    "/nix",
    "/proc",
    "/sys",
    "/dev",
    "/etc",
    "/boot",
    "/usr",
    "/bin",
    "/sbin",
    "/lib",
    "/lib64",
    "/root",
    "/tmp",
    "/var/lib",
    "/var/log",
    "/var/tmp",
    "/var/empty",
    // The guest's own `/run` names. `/run/vivarium` is the agent's `RuntimeDirectory`, and
    // `/run/vivarium-workspace` is where the share itself mounts; the guest re-derives the latter
    // from the value it was built with, so this spelling is the host's copy and `tests/nix`
    // asserts the Nix constant against it.
    "/run/vivarium",
    "/run/vivarium-workspace",
    "/run/user",
    "/run/current-system",
    "/run/booted-system",
    "/run/wrappers",
    "/run/systemd",
    "/run/udev",
    "/run/dbus",
    "/run/lock",
    "/run/log",
    "/run/keys",
    "/run/credentials",
    "/run/binfmt",
    "/run/nscd",
    "/run/opengl-driver",
    // The guest home, and not a corner case: a host user named `vivarium` keeps their projects
    // under exactly this path, and it is an ext4 volume, so mirroring into it would create
    // root-owned directories inside a persistent home the user cannot clear. Refused rather than
    // worked around; moving the guest home is `Q-017` in `docs/plan/open-questions.md`.
    "/home/vivarium",
];

/// Why a host path cannot be mirrored as the guest's workspace, or `None` when it can (ADR-0100).
///
/// Returns the message the refusal is stated with, so the CLI diagnostic and this module's contract
/// check share one vocabulary instead of describing the same rule twice.
#[must_use]
pub fn unmirrorable(path: &Path) -> Option<&'static str> {
    if !path.is_absolute() {
        return Some("workspace path must be absolute to be mirrored into the guest");
    }
    if path.parent().is_none() {
        return Some("the filesystem root cannot be mirrored into the guest");
    }
    for owned in GUEST_OWNED_PATHS {
        let owned = Path::new(owned);
        // `Path::starts_with` compares whole components, so `/nix` does not match
        // `/nixos-projects`. A string prefix would, which is the bug this notes rather than risks.
        if path.starts_with(owned) || owned.starts_with(path) {
            return Some("workspace path collides with a path the guest owns");
        }
    }
    None
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

/// Whether a file plan's entry is in the percent-encoded shape the guest's decoder accepts.
///
/// The allowlist has no `/` and no bare `%`, so a well-formed entry cannot name a path or smuggle
/// bytes past the decode; `.` and `..` are refused literally here and again, after decoding, by
/// the guest — which is the authority, this being the backstop for a hand-written record.
fn is_encoded_entry(entry: &str) -> bool {
    if entry.is_empty() || entry == "." || entry == ".." {
        return false;
    }
    let bytes = entry.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                let hex = |offset: usize| {
                    bytes
                        .get(index + offset)
                        .is_some_and(|byte| byte.is_ascii_digit() || (b'A'..=b'F').contains(byte))
                };
                if !(hex(1) && hex(2)) {
                    return false;
                }
                index += 3;
            }
            byte if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'~' | b'-') => {
                index += 1;
            }
            _ => return false,
        }
    }
    true
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
pub mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// A complete, valid spec.
    ///
    /// Reachable from sibling modules on purpose: without it a renderer's test
    /// can only compare a hand-written argument list against another
    /// hand-written argument list, which passes however the renderer changes.
    pub fn fixture() -> LaunchSpec {
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
                cloud_hypervisor: "/nix/store/a/bin/cloud-hypervisor".into(),
                ch_remote: "/nix/store/a/bin/ch-remote".into(),
                virtiofsd: "/nix/store/b/bin/virtiofsd".into(),
                setpriv: "/nix/store/c/bin/setpriv".into(),
                truncate: "/nix/store/d/bin/truncate".into(),
                mkfs_ext4: "/nix/store/e/bin/mkfs.ext4".into(),
                systemd_run: "/nix/store/f/bin/systemd-run".into(),
                supervisor: "/nix/store/g/bin/vivarium-supervisor".into(),
                unshare: "/nix/store/c/bin/unshare".into(),
                nsenter: "/nix/store/c/bin/nsenter".into(),
                ip: "/nix/store/i/bin/ip".into(),
                nft: "/nix/store/j/bin/nft".into(),
                pasta: "/nix/store/k/bin/pasta".into(),
                sleep: "/nix/store/d/bin/sleep".into(),
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
                shell: "/nix/store/h/bin/bash".into(),
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
                tag: WORKSPACE_SHARE_TAG.into(),
                source,
                mount_point: "/run/vivarium-workspace".into(),
                socket: child("workspace.sock"),
                cache: "auto".into(),
                read_only: false,
                mount_plan: None,
                extra_args: vec![],
            }],
            volumes: vec![],
            vm_create: serde_json::json!({"payload": {"kernel": "/nix/store/a/vmlinux"}}),
            landlock_available: false,
            console_log_enabled: true,
        }
    }

    /// A record another version wrote reads as skew with both numbers named, not corruption.
    ///
    /// Through `from_json` itself rather than a caller's wrapper, because every parser of the
    /// launch record — the private handoff, the supervisor's entrypoints — funnels through it,
    /// and spec/10 requires the skew diagnostic at each of them.
    #[test]
    fn a_foreign_version_record_reads_as_skew_with_both_numbers_named() {
        let mut record = serde_json::to_value(fixture()).unwrap();
        record["schemaVersion"] = serde_json::json!(LAUNCH_SCHEMA_VERSION - 1);
        let error = LaunchSpec::from_json(record.to_string().as_bytes()).unwrap_err();
        let message = error.to_string();
        assert!(
            matches!(
                error,
                LaunchError::SchemaSkew { record, current }
                    if record == LAUNCH_SCHEMA_VERSION - 1 && current == LAUNCH_SCHEMA_VERSION
            ),
            "skew collapsed into another case: {message}"
        );
        assert!(
            message.contains(&(LAUNCH_SCHEMA_VERSION - 1).to_string())
                && message.contains(&LAUNCH_SCHEMA_VERSION.to_string()),
            "the diagnostic does not name both schema versions: {message}"
        );

        // Bytes that yield no envelope at all were written by nothing vivarium shipped: still
        // the corruption case, so skew has not widened what counts as readable.
        assert!(matches!(
            LaunchSpec::from_json(b"{\"not\":\"a record\"}"),
            Err(LaunchError::InvalidSpec(_))
        ));
    }

    /// One volume as the launcher renders it, for the mutations below.
    fn volume(label: &str, path: &str) -> VolumeSpec {
        VolumeSpec {
            label: label.into(),
            path: path.into(),
            size_mib: 32768,
            image_type: "raw".into(),
            inode_ratio: None,
        }
    }

    /// The cross-language pairings, asserted against the embedded product tree.
    ///
    /// With no Nix-built host binary left to inspect (ADR-0102), a constant shared by the crate
    /// and the Nix half is compared against the embedded copy the binary actually ships — still
    /// two independently realised artifacts: the Rust constant and the Nix source the guest
    /// evaluates.
    #[test]
    fn the_crate_and_the_embedded_nix_tree_agree_on_shared_constants() {
        let launch_arguments =
            std::str::from_utf8(crate::config::embedded_file("nix/launch-arguments.nix").unwrap())
                .unwrap();
        assert!(
            launch_arguments.contains(&format!("schemaVersion = {LAUNCH_SCHEMA_VERSION};")),
            "nix/launch-arguments.nix does not declare schemaVersion = {LAUNCH_SCHEMA_VERSION}"
        );

        let product =
            std::str::from_utf8(crate::config::embedded_file("nix/default.nix").unwrap()).unwrap();
        let workspace_internal = "/run/vivarium-workspace";
        assert!(
            GUEST_OWNED_PATHS.contains(&workspace_internal),
            "the host no longer refuses to mirror onto the share's internal mount point"
        );
        assert!(
            product.contains(&format!(
                "workspaceInternalMountPoint = \"{workspace_internal}\";"
            )),
            "nix/default.nix does not declare the internal mount point the host refuses"
        );
    }

    #[test]
    fn accepts_the_volume_list_a_build_declares() {
        // The shape `--volume-dir` produces: distinct names joined onto one directory, in the
        // order `nix/guest.nix` fixes. A positive case, because every other volume assertion here
        // is a refusal and a validator that refused everything would pass all of them.
        let mut spec = fixture();
        spec.volumes = vec![
            volume("vivarium-default", "/s/projects/p/t/volumes/default.img"),
            volume("vivarium-store", "/s/projects/p/t/volumes/store.img"),
            volume("viv-cache", "/s/projects/p/t/volumes/cache.img"),
        ];
        spec.validate().unwrap();
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
            // ADR-0100: the workspace source is also the guest's mount path, so a source the guest
            // owns is now a refusal here and not only in the CLI that usually gets there first.
            Box::new(|s| s.shares[0].source = PathBuf::from("/home/vivarium/work")),
            Box::new(|s| s.shares[0].source = PathBuf::from("/home")),
            // The volume half. Every one of these is a way the `--volume-dir` join can go wrong,
            // and none of them was reachable while the host named two image paths itself.
            Box::new(|s| {
                s.volumes.push(volume("vivarium-default", "/v/default.img"));
                s.volumes.push(volume("vivarium-default", "/v/other.img"));
            }),
            Box::new(|s| {
                s.volumes.push(volume("vivarium-default", "/v/default.img"));
                s.volumes.push(volume("viv-cache", "/v/default.img"));
            }),
            Box::new(|s| s.volumes.push(volume("", "/v/default.img"))),
            Box::new(|s| {
                s.volumes
                    .push(volume("viv-cache", "@VOLUME_DIR@/cache.img"));
            }),
            Box::new(|s| s.volumes.push(volume("viv-cache", "v/cache.img"))),
            Box::new(|s| s.volumes.push(volume("viv-cache", "/v/../cache.img"))),
            // The networking half of schema 6: a malformed allowlist entry, an
            // unrenderable tap name, a subnet with no room, a family split, a MAC
            // the VMM would choke on, and a backend program that is not a store
            // path.
            Box::new(|s| s.egress.allow.push("https://example.com".into())),
            Box::new(|s| s.network.tap_name = "a name with spaces".into()),
            Box::new(|s| s.network.tap_name = String::new()),
            Box::new(|s| s.network.prefix_length = 31),
            Box::new(|s| s.network.guest_address = "2001:db8::2".parse().unwrap()),
            Box::new(|s| s.network.resolver_port = 0),
            Box::new(|s| s.network.guest_mac = "02-56-49-56-41-00".into()),
            Box::new(|s| s.network.guest_mac = "02:56:49:56:41".into()),
            Box::new(|s| s.backend_programs.pasta = "/usr/bin/pasta".into()),
            // The mount-plan half of schema 8: a directory plan carrying an entry, a file plan
            // with none, and file plans whose entry the guest's decoder would refuse — a path,
            // a dot-name, a bare `%`.
            Box::new(|s| {
                s.shares[0].mount_plan = Some(MountPlan {
                    kind: MountPlanKind::Dir,
                    entry: Some("stray".into()),
                });
            }),
            Box::new(|s| {
                s.shares[0].mount_plan = Some(MountPlan {
                    kind: MountPlanKind::File,
                    entry: None,
                });
            }),
            Box::new(|s| {
                s.shares[0].mount_plan = Some(MountPlan {
                    kind: MountPlanKind::File,
                    entry: Some("a/b".into()),
                });
            }),
            Box::new(|s| {
                s.shares[0].mount_plan = Some(MountPlan {
                    kind: MountPlanKind::File,
                    entry: Some("..".into()),
                });
            }),
            Box::new(|s| {
                s.shares[0].mount_plan = Some(MountPlan {
                    kind: MountPlanKind::File,
                    entry: Some("%2".into()),
                });
            }),
            Box::new(|s| {
                s.shares[0].mount_plan = Some(MountPlan {
                    kind: MountPlanKind::File,
                    entry: Some(String::new()),
                });
            }),
        ];
        for mutate in mutations {
            let mut spec = fixture();
            assert_eq!(spec.shares.len(), 1);
            mutate(&mut spec);
            assert!(spec.validate().is_err());
        }
    }

    /// The declared-mount shapes schema 9 admits, and the pairing it now requires.
    ///
    /// A plan does not only say what the guest does with the share; since ADR-0105 it says what
    /// kind of thing `source` is. A directory plan names a directory, a file plan names the file
    /// itself — the parent stopped appearing anywhere — so each plan is asserted against both
    /// kinds of source, and the two crossed pairings are refusals rather than shapes that boot
    /// into a daemon serving the wrong tree.
    #[test]
    fn a_plan_and_its_source_kind_must_agree() {
        let mut spec = fixture();
        let mut declared = spec.shares[0].clone();
        declared.tag = "mnt0".into();
        declared.mount_point = "/run/vivarium-mounts/mnt0".into();
        declared.socket = spec.runtime_paths.root.join("mnt0.sock");
        declared.mount_plan = Some(MountPlan {
            kind: MountPlanKind::Dir,
            entry: None,
        });
        spec.shares.push(declared);
        spec.validate().unwrap();

        let directory = spec.shares[1].source.clone();
        let file = std::env::current_dir().unwrap().join("Cargo.toml");
        let file_plan = Some(MountPlan {
            kind: MountPlanKind::File,
            entry: Some("Cargo.toml".into()),
        });

        spec.shares[1].mount_plan.clone_from(&file_plan);
        spec.shares[1].source.clone_from(&file);
        spec.validate().unwrap();
        assert!(spec.shares[1].stage_dir(&spec.runtime_paths.root).is_some());

        // A file plan whose source is the parent directory again: the shape this slice removed.
        spec.shares[1].source.clone_from(&directory);
        assert!(spec.validate().is_err());

        // And the mirror image, a directory plan handed a regular file.
        spec.shares[1].source.clone_from(&file);
        spec.shares[1].mount_plan = Some(MountPlan {
            kind: MountPlanKind::Dir,
            entry: None,
        });
        assert!(spec.validate().is_err());
        assert!(spec.shares[1].stage_dir(&spec.runtime_paths.root).is_none());

        // A well-formed entry that names a different file than the source. The staging step reads
        // the export root's one entry from the source, so a record that disagrees with itself
        // stages one name and asks the guest for another; refused here rather than at the guest's
        // mount unit, which would report a missing file.
        spec.shares[1].source = file;
        spec.shares[1].mount_plan = Some(MountPlan {
            kind: MountPlanKind::File,
            entry: Some("Cargo.lock".into()),
        });
        assert!(spec.validate().is_err());
    }

    /// A tag that is not the `[a-z0-9]+` token the build emits cannot walk the export root out of
    /// the runtime directory.
    ///
    /// The staging path is derived from the tag rather than carried in the record, so a tag
    /// holding a separator turns `Path::join` into a path constructor: the supervisor would create
    /// a directory the launch never named, point a daemon at it, and the teardown sweep would then
    /// remove it. Held to the socket's own rule instead — an exact child of the runtime root.
    #[test]
    fn a_staged_share_cannot_name_an_export_root_outside_the_runtime_directory() {
        let base = fixture();
        let file = std::env::current_dir().unwrap().join("Cargo.toml");
        for tag in ["/tmp/escaped", "../escaped", "nested/escaped"] {
            let mut spec = base.clone();
            let mut declared = spec.shares[0].clone();
            declared.tag = tag.into();
            declared.source.clone_from(&file);
            declared.mount_point = "/run/vivarium-mounts/mnt0".into();
            declared.socket = spec.runtime_paths.root.join("mnt0.sock");
            declared.mount_plan = Some(MountPlan {
                kind: MountPlanKind::File,
                entry: Some("Cargo.toml".into()),
            });
            spec.shares.push(declared);
            assert!(
                spec.validate().is_err(),
                "a share tagged {tag} derived an export root the runtime root does not hold"
            );
        }
    }

    #[test]
    fn unmirrorable_names_the_paths_the_guest_owns() {
        // Refused: the guest owns these, or the mirror would contain something it owns.
        for refused in [
            "/",
            "/nix",
            "/nix/store/x",
            "/etc/projects",
            "/home/vivarium",
            "/home/vivarium/work",
            // A proper ancestor: mirroring here would bury `/home/vivarium` under the share.
            "/home",
            "/var/lib/thing",
            "/run/vivarium/x",
            "/run/vivarium-workspace/x",
            "/run/user/1000/x",
            "relative/path",
        ] {
            assert!(
                unmirrorable(Path::new(refused)).is_some(),
                "{refused} should be refused"
            );
        }
        // Allowed. `/nixos-projects` is the component-wise check earning its keep: a string prefix
        // would read it as `/nix`. `/var/home/u` is the ostree layout a blanket `/var` would break.
        for allowed in [
            "/home/u/Projects/foo",
            "/nixos-projects/foo",
            "/var/home/u/foo",
            // A removable drive on an ordinary desktop, and the reason `/run` is not denied
            // wholesale: this repository's own acceptance fixtures live at exactly this shape.
            "/run/media/u/drive/projects/foo",
            "/mnt/work/foo",
            "/srv/foo",
            "/data/foo",
            "/home/vivariumesque/foo",
        ] {
            assert!(
                unmirrorable(Path::new(allowed)).is_none(),
                "{allowed} should be allowed"
            );
        }
    }

    #[test]
    fn unresolved_token_matches_the_sentinel_shape_only() {
        for leftover in [
            "/x/@WORKSPACE_SOURCE@",
            "@VOLUME_DIR@",
            // The joined shape, which is what a volume path looks like when the launcher failed
            // to substitute: the sentinel is no longer the whole string, only its head.
            "@VOLUME_DIR@/cache.img",
            "/a/@GID@/b",
            "/@A@",
            // A share socket token embeds its tag, and a declared mount's tag holds digits —
            // the leftover a runner that never learned the tag would leave (slice 019).
            "/x/@SHARE_SOCKET_MNT0@",
        ] {
            assert!(contains_unresolved_token(leftover), "{leftover}");
        }
        // Ordinary paths that the previous bare-`@` check refused. The first is the common one: a
        // version-pinned checkout directory, which since ADR-0100 is also a guest mount path.
        for ordinary in [
            "/home/u/src/pkg@1.0",
            "/home/u/@",
            "/home/u/a@b",
            "/home/u/@@",
            "/home/u/@lower@",
            "/home/u/node_modules/@scope/pkg",
        ] {
            assert!(!contains_unresolved_token(ordinary), "{ordinary}");
        }
    }
}
