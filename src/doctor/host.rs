//! The host-scope probes: what this machine can do, before any project is consulted.
//!
//! Each probe reads one host condition and judges it; the judgments with content — version
//! floors, headroom arithmetic — are free functions with tests, because the host conditions they
//! describe cannot be conjured in a test environment. Readings that fail to read at all become
//! `skipped` / `not-applicable` with the fault named, never a fabricated pass: an assumption-shaped
//! check that cannot observe must say so rather than answer.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::{self, Environment};
use crate::launch::DescriptorBudget;

use super::{Finding, Inputs, Probe, descriptors};

/// The Nix floor spec/13 names: the latest stable release at decision time.
const NIX_FLOOR: (u64, u64) = (2, 34);

/// The kernel floor for the virtio feature set the backend uses. Cloud Hypervisor's supported
/// host range starts here; older kernels predate the virtio-fs and free-page-reporting shapes the
/// launch profile depends on.
const KERNEL_FLOOR: (u64, u64) = (5, 13);

/// One build cycle plus headroom, spec/13's store floor.
const STORE_FLOOR_BYTES: u64 = 10 * 1024 * 1024 * 1024;

/// The state-filesystem floor when no bound project names its volumes' claims.
const STATE_FLOOR_BYTES: u64 = 5 * 1024 * 1024 * 1024;

/// The host-memory reserve below which a launch is likely to thrash whatever the manifest asks.
const MEMORY_RESERVE_BYTES: u64 = 1024 * 1024 * 1024;

/// What a large workspace can pin: one descriptor per referenced inode, per share (ADR-0093).
const LARGE_WORKSPACE_DESCRIPTORS: u64 = 100_000;

pub(super) fn run<E: Environment>(probe: &'static Probe, inputs: &Inputs<'_, E>) -> Finding {
    match probe.id {
        "nix-present" => nix_present(probe),
        "nix-version" => nix_version(probe),
        "nix-flakes-enabled" => nix_flakes_enabled(probe),
        "kvm-device-present" => kvm_device_present(probe),
        "kvm-device-accessible" => kvm_device_accessible(probe),
        "hardware-virt-available" => hardware_virt_available(probe),
        "host-userns-available" => host_userns_available(probe),
        "runtime-dir-usable" => runtime_dir_usable(probe, inputs),
        "nix-store-disk-space" => nix_store_disk_space(probe),
        "state-dir-free-space" => state_dir_free_space(probe, inputs),
        "host-memory-headroom" => host_memory_headroom(probe, inputs),
        "host-cgroup2-delegation" => host_cgroup2_delegation(probe),
        "host-fd-limit-sufficient" => host_fd_limit_sufficient(probe),
        "kernel-version-supported" => kernel_version_supported(probe),
        "host-landlock-available" => host_landlock_available(probe),
        "state-dir-writable" => directory_writable(probe, &inputs.roots.state, "state"),
        "state-manifest-orphans" => state_manifest_orphans(probe, inputs),
        "cache-dir-writable" => directory_writable(probe, &inputs.roots.cache, "cache"),
        "data-dir-writable" => directory_writable(probe, &inputs.roots.data, "data"),
        "store-roots-intact" => store_roots_intact(probe, inputs),
        "host-linger" => host_linger(probe, inputs),
        _ => Finding::skipped(probe, "not-applicable", "not a host probe"),
    }
}

fn nix_present(probe: &'static Probe) -> Finding {
    match nix_version_output() {
        Ok(version) => Finding::pass(probe, format!("nix {version} on PATH")),
        Err(why) => Finding::tripped(
            probe,
            why,
            "install Nix (https://nixos.org/download) and re-run `viv doctor`",
        ),
    }
}

fn nix_version(probe: &'static Probe) -> Finding {
    let version = match nix_version_output() {
        Ok(version) => version,
        Err(why) => {
            // `nix-present` already failed and owns the remedy; this probe cannot read without it.
            return Finding::skipped(probe, "not-applicable", why);
        }
    };
    match version_at_least(&version, NIX_FLOOR) {
        Some(true) => Finding::pass(
            probe,
            format!("nix {version}, minimum {}.{}", NIX_FLOOR.0, NIX_FLOOR.1),
        ),
        Some(false) => Finding::tripped(
            probe,
            format!("nix {version}, minimum {}.{}", NIX_FLOOR.0, NIX_FLOOR.1),
            format!(
                "upgrade nix to {}.{} or newer, then re-run `viv doctor`",
                NIX_FLOOR.0, NIX_FLOOR.1
            ),
        ),
        None => Finding::skipped(
            probe,
            "not-applicable",
            format!("`nix --version` reported an unparsable version `{version}`"),
        ),
    }
}

fn nix_flakes_enabled(probe: &'static Probe) -> Finding {
    let output = Command::new("nix")
        .args(["config", "show", "experimental-features"])
        .output();
    let Ok(output) = output else {
        return Finding::skipped(probe, "not-applicable", "`nix` could not be run");
    };
    let features = String::from_utf8_lossy(&output.stdout);
    let enabled = |name: &str| features.split_whitespace().any(|feature| feature == name);
    if output.status.success() && enabled("nix-command") && enabled("flakes") {
        Finding::pass(probe, "nix-command flakes")
    } else {
        Finding::tripped(
            probe,
            format!(
                "`experimental-features` is `{}`",
                features.trim().replace('\n', " ")
            ),
            "add `experimental-features = nix-command flakes` to nix.conf",
        )
    }
}

fn kvm_device_present(probe: &'static Probe) -> Finding {
    if Path::new("/dev/kvm").exists() {
        Finding::pass(probe, "/dev/kvm exists")
    } else {
        Finding::tripped(
            probe,
            "this host has no /dev/kvm",
            "enable hardware virtualization in firmware and load the kvm module \
            for your processor",
        )
    }
}

fn kvm_device_accessible(probe: &'static Probe) -> Finding {
    if !Path::new("/dev/kvm").exists() {
        return Finding::skipped(probe, "not-applicable", "/dev/kvm does not exist");
    }
    match rustix::fs::access(
        "/dev/kvm",
        rustix::fs::Access::READ_OK | rustix::fs::Access::WRITE_OK,
    ) {
        Ok(()) => Finding::pass(probe, "/dev/kvm is readable and writable"),
        Err(errno) => Finding::tripped(
            probe,
            format!("/dev/kvm is not accessible: {errno}"),
            "join the `kvm` group (or the group that owns /dev/kvm), then log in again",
        ),
    }
}

fn hardware_virt_available(probe: &'static Probe) -> Finding {
    let Ok(cpuinfo) = std::fs::read_to_string("/proc/cpuinfo") else {
        return Finding::skipped(probe, "not-applicable", "/proc/cpuinfo could not be read");
    };
    if cpu_virtualization_flag(&cpuinfo) {
        Finding::pass(probe, "CPU virtualization extensions are present")
    } else {
        Finding::tripped(
            probe,
            "no vmx or svm flag in /proc/cpuinfo",
            "enable VT-x/AMD-V in firmware; some hosts ship with it disabled",
        )
    }
}

/// Whether the flags line names vmx (Intel) or svm (AMD). Pure over the file's text.
fn cpu_virtualization_flag(cpuinfo: &str) -> bool {
    cpuinfo
        .lines()
        .filter(|line| line.starts_with("flags") || line.starts_with("Features"))
        .any(|line| {
            line.split_whitespace()
                .any(|flag| flag == "vmx" || flag == "svm")
        })
}

fn host_userns_available(probe: &'static Probe) -> Finding {
    // Debian's knob first, because where it exists it is the decider; elsewhere the namespace
    // ceiling says whether the kernel grants any at all.
    if let Ok(clone) = std::fs::read_to_string("/proc/sys/kernel/unprivileged_userns_clone")
        && clone.trim() == "0"
    {
        return Finding::tripped(
            probe,
            "kernel.unprivileged_userns_clone is 0",
            "set `sysctl kernel.unprivileged_userns_clone=1`; the share sandbox (N20) \
            cannot be built without it",
        );
    }
    match std::fs::read_to_string("/proc/sys/user/max_user_namespaces") {
        Ok(ceiling) if ceiling.trim() == "0" => Finding::tripped(
            probe,
            "user.max_user_namespaces is 0",
            "raise `sysctl user.max_user_namespaces`; the share sandbox (N20) cannot \
            be built without it",
        ),
        Ok(ceiling) => Finding::pass(
            probe,
            format!("user.max_user_namespaces is {}", ceiling.trim()),
        ),
        Err(error) => Finding::skipped(
            probe,
            "not-applicable",
            format!("/proc/sys/user/max_user_namespaces could not be read: {error}"),
        ),
    }
}

fn runtime_dir_usable<E: Environment>(probe: &'static Probe, inputs: &Inputs<'_, E>) -> Finding {
    // The one resolver the whole product uses; its error names which of the four faults tripped
    // (ADR-0055), which is the whole value of failing loudly here.
    let root = match config::resolve_runtime_root(inputs.environment, config::effective_uid()) {
        Ok(root) => root,
        Err(error) => {
            return Finding::tripped(
                probe,
                error.to_string(),
                "log in through a session manager that provides XDG_RUNTIME_DIR, \
                or repair its ownership and mode",
            );
        }
    };
    // The resolver validates presence, ownership, and privacy; whether the subtree can be used —
    // created on first run, entered and written thereafter — is this probe's own reading, because
    // spec/13 counts "unwritable" among the faults the probe covers.
    match directory_admits_creation(&root) {
        Ok(_) => Finding::pass(probe, format!("{} is usable", root.display())),
        Err((target, why)) => Finding::tripped(
            probe,
            format!("`{}` {why}", target.display()),
            "repair the runtime directory's mode; vivarium creates and enters its subtree \
            beneath it",
        ),
    }
}

/// Whether new entries can be created at `root`: judged on `root` itself when it exists, else on
/// the nearest existing ancestor.
///
/// Creation needs a directory with both write and search permission — `WRITE_OK` alone would
/// pass a writable regular file squatting on the path, or a directory whose execute bit is off,
/// and either failure would otherwise surface mid-command, after side effects.
fn directory_admits_creation(root: &Path) -> Result<&Path, (&Path, String)> {
    // Occupancy is read with `symlink_metadata`, not `exists`: `exists` follows links, so a
    // dangling symlink would read as an absent path its ancestor can create, while the mkdir
    // first use performs fails on the link itself. Only absence continues the walk — a read that
    // fails any other way (`ENAMETOOLONG`, `ELOOP`, a traversal `EACCES`) is a fault of the path
    // itself, not an absent entry a shallower ancestor could create through.
    let mut target = root;
    for ancestor in root.ancestors() {
        match ancestor.symlink_metadata() {
            Ok(_) => {
                target = ancestor;
                break;
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) => {}
            Err(error) => return Err((ancestor, format!("cannot be inspected: {error}"))),
        }
    }
    if !target.is_dir() {
        // `is_dir` follows links, so a regular file and a dangling symlink both land here:
        // either occupies the path and blocks creation.
        let why = if target.is_symlink() && !target.exists() {
            "is a symlink that does not resolve"
        } else {
            "exists and is not a directory"
        };
        return Err((target, why.to_owned()));
    }
    match rustix::fs::access(
        target,
        rustix::fs::Access::WRITE_OK | rustix::fs::Access::EXEC_OK,
    ) {
        Ok(()) => Ok(target),
        Err(errno) => Err((target, format!("is not writable and searchable: {errno}"))),
    }
}

fn nix_store_disk_space(probe: &'static Probe) -> Finding {
    match free_bytes("/nix/store") {
        Some(free) if free < STORE_FLOOR_BYTES => Finding::tripped(
            probe,
            format!(
                "{} free on the store filesystem (warn under {})",
                gigabytes(free),
                gigabytes(STORE_FLOOR_BYTES)
            ),
            "reclaim space with `viv gc`; retained builds are rooted (spec/11) and survive \
            a collection",
        ),
        Some(free) => Finding::pass(
            probe,
            format!("{} free on the store filesystem", gigabytes(free)),
        ),
        None => Finding::skipped(probe, "not-applicable", "/nix/store could not be measured"),
    }
}

fn state_dir_free_space<E: Environment>(probe: &'static Probe, inputs: &Inputs<'_, E>) -> Finding {
    let Some(free) = free_bytes(&inputs.roots.state) else {
        return Finding::skipped(
            probe,
            "not-applicable",
            "the state filesystem could not be measured",
        );
    };
    // The bound project's declared ceilings are the claims its sparse volumes could still make;
    // with nothing bound the fixed floor stands in.
    let claim = inputs
        .project
        .as_ref()
        .and_then(|project| project.parsed.as_ref().ok())
        .map_or(STATE_FLOOR_BYTES, |manifest| {
            let declared: u64 = manifest
                .volumes
                .iter()
                .map(|volume| u64::from(volume.size_gib.unwrap_or(0)) * 1024 * 1024 * 1024)
                .sum();
            declared.max(STATE_FLOOR_BYTES)
        });
    if free < claim {
        Finding::tripped(
            probe,
            format!(
                "{} free on the state filesystem, below the {} the volumes could still claim",
                gigabytes(free),
                gigabytes(claim)
            ),
            "free space on the state filesystem, or prune volumes with `viv volume prune` — \
            a sparse volume that meets host exhaustion surfaces inside the guest as an I/O error",
        )
    } else {
        Finding::pass(
            probe,
            format!("{} free on the state filesystem", gigabytes(free)),
        )
    }
}

fn host_memory_headroom<E: Environment>(probe: &'static Probe, inputs: &Inputs<'_, E>) -> Finding {
    let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") else {
        return Finding::skipped(probe, "not-applicable", "/proc/meminfo could not be read");
    };
    let Some(available) = available_memory_bytes(&meminfo) else {
        return Finding::skipped(
            probe,
            "not-applicable",
            "/proc/meminfo carries no MemAvailable reading",
        );
    };
    let ceiling = inputs
        .project
        .as_ref()
        .and_then(|project| project.parsed.as_ref().ok())
        .and_then(|manifest| manifest.resources.as_ref())
        .and_then(|resources| resources.mem_mib)
        .map_or(0, |mem_mib| u64::from(mem_mib) * 1024 * 1024);
    let needed = MEMORY_RESERVE_BYTES.max(ceiling);
    if available < needed {
        Finding::tripped(
            probe,
            format!(
                "{} available, below the {} the reserve and the bound ceiling need",
                gigabytes(available),
                gigabytes(needed)
            ),
            "close memory-heavy processes, or lower the manifest's `resources.mem_mib` ceiling",
        )
    } else {
        Finding::pass(probe, format!("{} available", gigabytes(available)))
    }
}

/// `MemAvailable` in bytes, from `/proc/meminfo`'s text. Pure for the parse's own tests.
fn available_memory_bytes(meminfo: &str) -> Option<u64> {
    meminfo
        .lines()
        .find(|line| line.starts_with("MemAvailable:"))
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|kib| kib.parse::<u64>().ok())
        .map(|kib| kib * 1024)
}

fn host_cgroup2_delegation(probe: &'static Probe) -> Finding {
    let uid = config::effective_uid();
    let controllers_path =
        format!("/sys/fs/cgroup/user.slice/user-{uid}.slice/user@{uid}.service/cgroup.controllers");
    match std::fs::read_to_string(&controllers_path) {
        Ok(controllers) => {
            let has = |name: &str| controllers.split_whitespace().any(|c| c == name);
            if has("memory") && has("cpu") {
                Finding::pass(
                    probe,
                    format!("memory and cpu delegated ({})", controllers.trim()),
                )
            } else {
                Finding::tripped(
                    probe,
                    format!(
                        "delegated controllers are `{}`; memory and cpu are needed",
                        controllers.trim()
                    ),
                    "delegate the controllers to user sessions (systemd's user-slice \
                    Delegate=); reporting and weighting degrade to host-level readings \
                    without them",
                )
            }
        }
        Err(error) => Finding::tripped(
            probe,
            format!("the user's delegated cgroup could not be read: {error}"),
            "run under a systemd user session with controller delegation; reporting and \
            weighting degrade to host-level readings without it",
        ),
    }
}

fn host_fd_limit_sufficient(probe: &'static Probe) -> Finding {
    match descriptors::host_fd_limit_sufficient(DescriptorBudget::default()) {
        Ok(check) if check.guest_allowance < LARGE_WORKSPACE_DESCRIPTORS => Finding::tripped(
            probe,
            format!(
                "the declared budget leaves {} descriptors per share, under the {} a large \
                workspace can pin",
                check.guest_allowance, LARGE_WORKSPACE_DESCRIPTORS
            ),
            "raise the declared limit (ADR-0093); exhaustion is refused per request and \
            surfaces inside the guest",
        ),
        Ok(check) => Finding::pass(
            probe,
            format!(
                "{} descriptors per share against a declared limit of {}",
                check.guest_allowance, check.declared_limit
            ),
        ),
        Err(error) => Finding::tripped(
            probe,
            format!("the declared budget leaves no usable allowance: {error}"),
            "raise the declared limit (ADR-0093)",
        ),
    }
}

fn kernel_version_supported(probe: &'static Probe) -> Finding {
    let Ok(release) = std::fs::read_to_string("/proc/sys/kernel/osrelease") else {
        return Finding::skipped(
            probe,
            "not-applicable",
            "the kernel release could not be read",
        );
    };
    let release = release.trim();
    match version_at_least(release, KERNEL_FLOOR) {
        Some(true) => Finding::pass(probe, format!("kernel {release}")),
        Some(false) => Finding::tripped(
            probe,
            format!(
                "kernel {release}, minimum {}.{} for the required virtio features",
                KERNEL_FLOOR.0, KERNEL_FLOOR.1
            ),
            "upgrade the host kernel",
        ),
        None => Finding::skipped(
            probe,
            "not-applicable",
            format!("the kernel release `{release}` could not be parsed"),
        ),
    }
}

fn host_landlock_available(probe: &'static Probe) -> Finding {
    match std::fs::read_to_string("/sys/kernel/security/lsm") {
        Ok(lsm) if lsm.split(',').any(|name| name.trim() == "landlock") => {
            Finding::pass(probe, "landlock is an active LSM")
        }
        Ok(lsm) => Finding::tripped(
            probe,
            format!("landlock is not among the active LSMs ({})", lsm.trim()),
            "boot with `lsm=landlock,...`; the seccomp and capability-drop sandbox (N20) \
            still applies, without the filesystem-path allowlist",
        ),
        Err(error) => Finding::skipped(
            probe,
            "not-applicable",
            format!("the active LSM list could not be read: {error}"),
        ),
    }
}

fn directory_writable(probe: &'static Probe, root: &Path, name: &str) -> Finding {
    // Not created yet is the ordinary first-run state: the root is made on first use, so what
    // matters is that making and entering it will succeed, which the judged target — the root
    // itself, or the nearest existing ancestor — decides.
    match directory_admits_creation(root) {
        Ok(target) if target == root => {
            Finding::pass(probe, format!("{} is writable", root.display()))
        }
        Ok(target) => Finding::pass(
            probe,
            format!(
                "the {name} root does not exist yet; `{}` can create it",
                target.display()
            ),
        ),
        Err((target, why)) => Finding::tripped(
            probe,
            format!("the {name} root is unusable: `{}` {why}", target.display()),
            format!("repair ownership or mode on `{}`", target.display()),
        ),
    }
}

fn state_manifest_orphans<E: Environment>(
    probe: &'static Probe,
    inputs: &Inputs<'_, E>,
) -> Finding {
    let orphans = match manifest_orphans(inputs.roots) {
        Ok(orphans) => orphans,
        Err(why) => {
            return Finding::skipped(probe, "not-applicable", why);
        }
    };
    if orphans.is_empty() {
        return Finding::pass(probe, "every retained sandbox key has a manifest");
    }
    let retained = orphans
        .iter()
        .map(|(name, paths)| {
            format!(
                "`{name}` at {}",
                paths
                    .iter()
                    .map(|path| format!("`{}`", path.display()))
                    .collect::<Vec<_>>()
                    .join(" and ")
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    Finding::tripped(
        probe,
        format!("retained sandbox state has no manifest: {retained}"),
        concat!(
            "restore the named manifest or inspect and remove the retained state deliberately; ",
            "vivarium did not delete it",
        ),
    )
}

/// Sandbox directories whose manifest-name key is absent from the current manifest library.
///
/// Both durable roots are observations only. A caller can show the retained paths, but nothing in
/// this probe removes them: ADR-0054 keeps that judgment with the operator.
fn manifest_orphans(roots: &config::XdgRoots) -> Result<Vec<(String, Vec<PathBuf>)>, String> {
    let manifests = config::artifact_names(&roots.config, config::ArtifactKind::Manifest)
        .map_err(|error| format!("the manifest library cannot be enumerated: {error}"))?
        .into_iter()
        .collect::<BTreeSet<_>>();
    let mut retained = BTreeMap::<String, Vec<PathBuf>>::new();
    for root in [&roots.state, &roots.data] {
        let projects = root.join("projects");
        let entries = match std::fs::read_dir(&projects) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!(
                    "retained sandbox state cannot be enumerated at `{}`: {error}",
                    projects.display()
                ));
            }
        };
        for entry in entries {
            let entry = entry.map_err(|error| {
                format!(
                    "retained sandbox state cannot be enumerated at `{}`: {error}",
                    projects.display()
                )
            })?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if !manifests.contains(&name) {
                retained.entry(name).or_default().push(path);
            }
        }
    }
    Ok(retained.into_iter().collect())
}

/// The running VMs' identities, from live pid records under the runtime root.
fn running_projects(runtime_root: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(runtime_root) else {
        return Vec::new();
    };
    let mut running = Vec::new();
    for entry in entries.flatten() {
        let project_dir = entry.path();
        let Ok(targets) = std::fs::read_dir(&project_dir) else {
            continue;
        };
        for target in targets.flatten() {
            let pid_file = target.path().join("vm.pid");
            let Ok(raw) = std::fs::read_to_string(&pid_file) else {
                continue;
            };
            let Ok(pid) = raw.trim().parse::<u32>() else {
                continue;
            };
            if Path::new(&format!("/proc/{pid}")).exists()
                && let Some(name) = entry.file_name().to_str()
            {
                running.push(name.to_owned());
            }
        }
    }
    running
}

fn store_roots_intact<E: Environment>(probe: &'static Probe, inputs: &Inputs<'_, E>) -> Finding {
    let running = inputs
        .runtime_root
        .as_deref()
        .map(running_projects)
        .unwrap_or_default();
    if running.is_empty() {
        // Conditioned on a running VM (spec/13): with nothing running there is nothing to corrupt.
        return Finding::pass(probe, "no VM is running");
    }
    for sandbox_id in &running {
        let record = inputs
            .roots
            .state
            .join("projects")
            .join(sandbox_id)
            .join("default")
            .join("running-build");
        let Ok(store_path) = std::fs::read_to_string(&record) else {
            continue;
        };
        let store_path = store_path.trim();
        let output = Command::new("nix-store")
            .args(["--query", "--requisites", store_path])
            .output();
        let Ok(output) = output else {
            return Finding::skipped(probe, "not-applicable", "`nix-store` could not be run");
        };
        if !output.status.success() {
            return Finding::tripped(
                probe,
                format!(
                    "`{sandbox_id}` is running and its closure could not be queried from \
                    `{store_path}` — the root itself may be gone"
                ),
                "stop the VM and start it again; something outside a root's reach removed \
                store paths under it",
            );
        }
        let requisites = String::from_utf8_lossy(&output.stdout);
        let missing: Vec<&str> = requisites
            .lines()
            .filter(|path| !path.is_empty() && !Path::new(path).exists())
            .take(3)
            .collect();
        if !missing.is_empty() {
            return Finding::tripped(
                probe,
                format!(
                    "`{sandbox_id}` is running and {} of its closure paths are gone, \
                    first `{}`",
                    missing.len(),
                    missing[0]
                ),
                "stop the VM and start it again; the overlay's behaviour over a collected \
                lower layer is undefined",
            );
        }
    }
    Finding::pass(
        probe,
        format!(
            "every running guest's closure is present ({} running)",
            running.len()
        ),
    )
}

fn host_linger<E: Environment>(probe: &'static Probe, inputs: &Inputs<'_, E>) -> Finding {
    let running = inputs
        .runtime_root
        .as_deref()
        .map(running_projects)
        .unwrap_or_default();
    if running.is_empty() {
        // Conditioned on a running VM (spec/13): lingering is off by default on most hosts, and
        // warning with nothing to lose would be noise.
        return Finding::pass(probe, "no VM is running");
    }
    let uid = config::effective_uid();
    let output = Command::new("loginctl")
        .args(["show-user", &uid.to_string(), "--property=Linger"])
        .output();
    let Ok(output) = output else {
        return Finding::skipped(probe, "not-applicable", "`loginctl` could not be run");
    };
    let linger = String::from_utf8_lossy(&output.stdout);
    if output.status.success() && linger.trim() == "Linger=yes" {
        Finding::pass(probe, "lingering is enabled; running VMs survive logout")
    } else {
        Finding::tripped(
            probe,
            format!(
                "{} VM(s) are running and lingering is off, so they end at the final logout",
                running.len()
            ),
            format!("run `loginctl enable-linger {uid}`"),
        )
    }
}

/// The `nix --version` reading: `nix (Nix) 2.34.8` → `2.34.8`.
fn nix_version_output() -> Result<String, String> {
    let output = Command::new("nix")
        .arg("--version")
        .output()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => "`nix` is not on PATH".to_owned(),
            _ => format!("`nix` could not be run: {error}"),
        })?;
    if !output.status.success() {
        return Err("`nix --version` failed".to_owned());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .split_whitespace()
        .last()
        .map(str::to_owned)
        .ok_or_else(|| "`nix --version` printed nothing".to_owned())
}

/// Whether `version` (leading `major.minor…`, tolerant of a suffix) meets the floor.
///
/// `None` when the text has no leading numeric pair — the caller reports rather than guesses.
fn version_at_least(version: &str, floor: (u64, u64)) -> Option<bool> {
    let mut parts = version.split(['.', '-']);
    let major: u64 = parts.next()?.parse().ok()?;
    let minor: u64 = parts.next()?.parse().ok()?;
    Some((major, minor) >= floor)
}

/// Available bytes on the filesystem holding `path`.
fn free_bytes(path: impl AsRef<Path>) -> Option<u64> {
    let stat = rustix::fs::statvfs(path.as_ref()).ok()?;
    Some(stat.f_bavail * stat.f_frsize)
}

fn gigabytes(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let gib = bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    format!("{gib:.1} GiB")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScratchDirectory;

    #[test]
    fn orphaned_state_is_named_and_never_deleted() -> Result<(), Box<dyn std::error::Error>> {
        let scratch = ScratchDirectory::new()?;
        let config = scratch.path().join("config");
        let state = scratch.path().join("state");
        let data = scratch.path().join("data");
        let cache = scratch.path().join("cache");
        std::fs::create_dir_all(config.join("manifests"))?;
        std::fs::write(config.join("manifests/current.toml"), "")?;
        let current = state.join("projects/current/default");
        let orphan_state = state.join("projects/gone/default");
        let orphan_data = data.join("projects/gone/default");
        for path in [&current, &orphan_state, &orphan_data] {
            std::fs::create_dir_all(path)?;
        }
        let roots = config::XdgRoots {
            config,
            data,
            state,
            cache,
        };

        let orphans = manifest_orphans(&roots)?;
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].0, "gone");
        assert_eq!(orphans[0].1.len(), 2);
        assert!(orphan_state.is_dir());
        assert!(orphan_data.is_dir());
        assert!(current.is_dir());
        Ok(())
    }

    #[test]
    fn the_version_floor_is_a_pair_comparison_not_a_string_one() {
        // `2.4` under a `2.34` floor is the case a string comparison gets wrong.
        assert_eq!(version_at_least("2.4.1", NIX_FLOOR), Some(false));
        assert_eq!(version_at_least("2.34.0", NIX_FLOOR), Some(true));
        assert_eq!(version_at_least("2.34.8", NIX_FLOOR), Some(true));
        assert_eq!(version_at_least("3.0", NIX_FLOOR), Some(true));
        assert_eq!(
            version_at_least("7.1.4-1-default", KERNEL_FLOOR),
            Some(true)
        );
        assert_eq!(version_at_least("5.10.0", KERNEL_FLOOR), Some(false));
        assert_eq!(version_at_least("garbage", NIX_FLOOR), None);
    }

    #[test]
    fn the_virtualization_flag_is_read_from_the_flags_line_only() {
        assert!(cpu_virtualization_flag("flags\t\t: fpu vmx sse2"));
        assert!(cpu_virtualization_flag("flags\t\t: fpu svm sse2"));
        // `svm` appearing outside a flags line — a model name, say — is not the fact.
        assert!(!cpu_virtualization_flag("model name\t: svm-optimized 9000"));
        assert!(!cpu_virtualization_flag("flags\t\t: fpu sse2"));
    }

    #[test]
    fn memavailable_is_read_in_bytes() {
        let meminfo = "MemTotal: 32456568 kB\nMemFree: 1413812 kB\nMemAvailable: 13648760 kB\n";
        assert_eq!(available_memory_bytes(meminfo), Some(13_648_760 * 1024));
        assert_eq!(available_memory_bytes("MemTotal: 1 kB\n"), None);
    }
}
