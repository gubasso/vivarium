//! The shared probe catalog spec/13 fixes: one registry, three call sites.
//!
//! `viv doctor` runs the whole catalog, every command's preflight guard runs the hard subset
//! before any side effect, and setup flows reuse individual probes — one list, so the three can
//! never drift (`ADR-0023`). The catalog rows carry everything `--list` enumerates without
//! probing: id, category, scope, severity, title.
//!
//! A probe reads the host and judges; it repairs nothing and evaluates nothing (`ADR-0022`,
//! `ADR-0049`). The judgments that are pure — version floors, secret heuristics, path lints,
//! name comparisons — live as free functions their tests exercise without the host conditions
//! they describe, because the agent environment is not the target host and a probe that could
//! only be tested by breaking a host would go untested.

pub mod descriptors;
mod host;
mod network;
mod project;

// The one-reader rule spec/17 puts on host memory: `status` reads the same parse the
// `host-memory-headroom` probe does rather than growing a sibling, and `start`'s admission
// gate acts on the same reserve the probe warns about.
pub(crate) use host::{MEMORY_RESERVE_BYTES, available_memory_bytes, total_memory_bytes};

use std::path::PathBuf;

use crate::config::{Environment, Manifest, ResolvedArtifact, XdgRoots};
use crate::exit::ExitKind;

/// The category column, spec/13's grouping for the human report.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Category {
    Tooling,
    Virtualization,
    Permissions,
    Disk,
    Capacity,
    Config,
    Network,
}

impl Category {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tooling => "tooling",
            Self::Virtualization => "virtualization",
            Self::Permissions => "permissions",
            Self::Disk => "disk",
            Self::Capacity => "capacity",
            Self::Config => "config",
            Self::Network => "network",
        }
    }
}

/// Where a probe can run: out-of-scope probes skip, never fail (spec/13).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Scope {
    /// Always runnable.
    Host,
    /// Needs a bound manifest; skips with `no-manifest-bound`.
    Project,
    /// Needs `--online`; skips with `offline-mode`.
    Network,
}

impl Scope {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::Project => "project",
            Self::Network => "network",
        }
    }
}

/// Hard probes form the preflight subset; soft ones only warn. There is no per-invocation
/// waiver of a hard probe — what could be waived is soft by definition (spec/13).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    Hard,
    Soft,
}

impl Severity {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hard => "hard",
            Self::Soft => "soft",
        }
    }
}

/// One catalog row: everything `--list` says without running anything.
pub struct Probe {
    /// The stable kebab-case id — also the diagnostic id's condition under `host.` when the
    /// preflight refuses on it (spec/01).
    pub id: &'static str,
    pub category: Category,
    pub scope: Scope,
    pub severity: Severity,
    pub title: &'static str,
    /// The failure's exit code, present exactly on the hard probes.
    pub code: Option<ExitKind>,
}

/// The catalog, in spec/13's order: cheapest-and-most-fundamental first, so the earliest failure
/// is the most actionable. The first eight are the preflight subset.
pub static CATALOG: &[Probe] = &[
    // Hard — host scope.
    Probe {
        id: "nix-present",
        category: Category::Tooling,
        scope: Scope::Host,
        severity: Severity::Hard,
        title: "`nix` is on `$PATH`",
        code: Some(ExitKind::Unavailable),
    },
    Probe {
        id: "nix-version",
        category: Category::Tooling,
        scope: Scope::Host,
        severity: Severity::Hard,
        title: "Nix meets the minimum version",
        code: Some(ExitKind::Unavailable),
    },
    Probe {
        id: "nix-flakes-enabled",
        category: Category::Tooling,
        scope: Scope::Host,
        severity: Severity::Hard,
        title: "`experimental-features` includes `nix-command flakes`",
        code: Some(ExitKind::Config),
    },
    Probe {
        id: "kvm-device-present",
        category: Category::Virtualization,
        scope: Scope::Host,
        severity: Severity::Hard,
        title: "`/dev/kvm` exists",
        code: Some(ExitKind::Unavailable),
    },
    Probe {
        id: "kvm-device-accessible",
        category: Category::Permissions,
        scope: Scope::Host,
        severity: Severity::Hard,
        title: "the user can read and write `/dev/kvm`",
        code: Some(ExitKind::NoPerm),
    },
    Probe {
        id: "hardware-virt-available",
        category: Category::Virtualization,
        scope: Scope::Host,
        severity: Severity::Hard,
        title: "CPU virtualization extensions are present and enabled",
        code: Some(ExitKind::Unavailable),
    },
    Probe {
        id: "host-userns-available",
        category: Category::Permissions,
        scope: Scope::Host,
        severity: Severity::Hard,
        title: "unprivileged user namespaces are available",
        code: Some(ExitKind::Unavailable),
    },
    Probe {
        id: "runtime-dir-usable",
        category: Category::Permissions,
        scope: Scope::Host,
        severity: Severity::Hard,
        title: "`$XDG_RUNTIME_DIR` is absolute, present, owned, and private",
        code: Some(ExitKind::NoPerm),
    },
    // Soft — host scope.
    Probe {
        id: "nix-store-disk-space",
        category: Category::Disk,
        scope: Scope::Host,
        severity: Severity::Soft,
        title: "the store filesystem has one build cycle of headroom",
        code: None,
    },
    Probe {
        id: "state-dir-free-space",
        category: Category::Disk,
        scope: Scope::Host,
        severity: Severity::Soft,
        title: "the state filesystem can still hold the volumes' claims",
        code: None,
    },
    Probe {
        id: "host-memory-headroom",
        category: Category::Capacity,
        scope: Scope::Host,
        severity: Severity::Soft,
        title: "available host memory covers the reserve and the bound ceiling",
        code: None,
    },
    Probe {
        id: "host-cgroup2-delegation",
        category: Category::Permissions,
        scope: Scope::Host,
        severity: Severity::Soft,
        title: "the user's cgroup hierarchy delegates memory and CPU",
        code: None,
    },
    Probe {
        id: "host-fd-limit-sufficient",
        category: Category::Permissions,
        scope: Scope::Host,
        severity: Severity::Soft,
        title: "the declared share descriptor budget covers a large workspace",
        code: None,
    },
    Probe {
        id: "kernel-version-supported",
        category: Category::Virtualization,
        scope: Scope::Host,
        severity: Severity::Soft,
        title: "the host kernel carries the required virtio features",
        code: None,
    },
    Probe {
        id: "host-landlock-available",
        category: Category::Virtualization,
        scope: Scope::Host,
        severity: Severity::Soft,
        title: "the kernel offers Landlock for the launch profile's path allowlist",
        code: None,
    },
    Probe {
        id: "state-dir-writable",
        category: Category::Permissions,
        scope: Scope::Host,
        severity: Severity::Soft,
        title: "the state root is writable",
        code: None,
    },
    Probe {
        id: "state-manifest-orphans",
        category: Category::Config,
        scope: Scope::Host,
        severity: Severity::Soft,
        title: "retained sandbox state still has a manifest",
        code: None,
    },
    Probe {
        id: "cache-dir-writable",
        category: Category::Permissions,
        scope: Scope::Host,
        severity: Severity::Soft,
        title: "the cache root is writable",
        code: None,
    },
    Probe {
        id: "data-dir-writable",
        category: Category::Permissions,
        scope: Scope::Host,
        severity: Severity::Soft,
        title: "the data root is writable",
        code: None,
    },
    Probe {
        id: "store-roots-intact",
        category: Category::Disk,
        scope: Scope::Host,
        severity: Severity::Soft,
        title: "no running guest's closure has been collected under it",
        code: None,
    },
    Probe {
        id: "host-linger",
        category: Category::Permissions,
        scope: Scope::Host,
        severity: Severity::Soft,
        title: "running VMs survive the user's final logout",
        code: None,
    },
    // Soft — project scope.
    Probe {
        id: "config-parses",
        category: Category::Config,
        scope: Scope::Project,
        severity: Severity::Soft,
        title: "the bound manifest parses",
        code: None,
    },
    Probe {
        id: "manifest-resolves",
        category: Category::Config,
        scope: Scope::Project,
        severity: Severity::Soft,
        title: "the binding resolves to exactly one defined manifest",
        code: None,
    },
    Probe {
        id: "working-directory-declared",
        category: Category::Config,
        scope: Scope::Project,
        severity: Severity::Soft,
        title: "the working directory is declared as a workspace",
        code: None,
    },
    Probe {
        id: "shared-layer-paths-portable",
        category: Category::Config,
        scope: Scope::Project,
        severity: Severity::Soft,
        title: "no shared layer declares a literal personal path",
        code: None,
    },
    Probe {
        id: "manifest-no-inline-secret",
        category: Category::Config,
        scope: Scope::Project,
        severity: Severity::Soft,
        title: "no manifest `[env]` value looks like a credential",
        code: None,
    },
    Probe {
        id: "mount-source-not-session-dir",
        category: Category::Config,
        scope: Scope::Project,
        severity: Severity::Soft,
        title: "no mount source resolves under a session directory",
        code: None,
    },
    Probe {
        id: "lock-covers-declared-inputs",
        category: Category::Config,
        scope: Scope::Project,
        severity: Severity::Soft,
        title: "the effective lock carries a node for every declared input",
        code: None,
    },
    Probe {
        id: "agent-source-usable",
        category: Category::Config,
        scope: Scope::Project,
        severity: Severity::Soft,
        title: "every declared agent channel has a usable host source",
        code: None,
    },
    // Soft — network scope.
    Probe {
        id: "nix-version-currency",
        category: Category::Network,
        scope: Scope::Network,
        severity: Severity::Soft,
        title: "the installed Nix does not trail the latest stable release",
        code: None,
    },
    Probe {
        id: "substituter-reachability",
        category: Category::Network,
        scope: Scope::Network,
        severity: Severity::Soft,
        title: "every configured substituter is reachable",
        code: None,
    },
    Probe {
        id: "egress-allowlist-dns",
        category: Category::Network,
        scope: Scope::Network,
        severity: Severity::Soft,
        title: "every wildcard-free allowlisted hostname resolves",
        code: None,
    },
];

/// One probe's answer for one run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    Pass,
    Warn,
    Fail,
    Skipped,
}

impl Status {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Warn => "warn",
            Self::Fail => "fail",
            Self::Skipped => "skipped",
        }
    }
}

/// What one probe found: the row the report renders.
pub struct Finding {
    pub probe: &'static Probe,
    pub status: Status,
    /// What was observed, in the wording rules spec/14 fixes. Empty on a skip.
    pub message: String,
    /// The remedy, present when there is one worth naming.
    pub hint: Option<String>,
    /// Why a skipped probe skipped: `no-manifest-bound`, `offline-mode`, or `not-applicable`.
    pub reason: Option<&'static str>,
}

impl Finding {
    fn pass(probe: &'static Probe, message: impl Into<String>) -> Self {
        Self {
            probe,
            status: Status::Pass,
            message: message.into(),
            hint: None,
            reason: None,
        }
    }

    fn tripped(probe: &'static Probe, message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            probe,
            status: match probe.severity {
                Severity::Hard => Status::Fail,
                Severity::Soft => Status::Warn,
            },
            message: message.into(),
            hint: Some(hint.into()),
            reason: None,
        }
    }

    fn skipped(probe: &'static Probe, reason: &'static str, message: impl Into<String>) -> Self {
        Self {
            probe,
            status: Status::Skipped,
            message: message.into(),
            hint: None,
            reason: Some(reason),
        }
    }
}

/// What the bound project contributes to the project-scope probes, when one is bound.
///
/// The two fallible halves arrive as results carrying the fault's own words, because two probes
/// exist exactly to report those faults early: `manifest-resolves` reads `artifact`'s error and
/// `config-parses` reads `parsed`'s.
pub struct ProjectInputs {
    /// The bound manifest's name.
    pub manifest: String,
    /// The resolved artifact, or why resolution refused.
    pub artifact: Result<ResolvedArtifact, String>,
    /// The parsed manifest, or why parsing refused.
    pub parsed: Result<Manifest, String>,
    /// ADR-0109's one ownership finding, retained here so doctor reports instead of refusing.
    pub workspace_refusal: Option<(String, String)>,
    /// The per-target lock in force, when one exists.
    pub lock_path: Option<PathBuf>,
}

/// Everything a run reads, injected: the same seam every other subsystem uses.
pub struct Inputs<'a, E: Environment> {
    pub environment: &'a E,
    pub roots: &'a XdgRoots,
    /// `None` when no manifest is bound; project probes then skip with `no-manifest-bound`.
    pub project: Option<ProjectInputs>,
    /// ADR-0109's second-claimant finding, as a message and a hint.
    ///
    /// Separate from `project` because it is precisely the case where there is no single project
    /// to gather: two manifests claim the directory, so neither is the one. Without this the
    /// finding would be erased — every project probe would skip with `no-manifest-bound`, which
    /// says the opposite of what happened.
    pub ownership_ambiguity: Option<(String, String)>,
    /// Whether `--online` admitted the network probes.
    pub online: bool,
    /// The runtime root, when the environment could resolve one; `store-roots-intact` and
    /// `host-linger` look here for running VMs.
    pub runtime_root: Option<PathBuf>,
}

/// Runs the whole catalog in order and returns one finding per probe.
#[must_use]
pub fn run<E: Environment>(inputs: &Inputs<'_, E>) -> Vec<Finding> {
    CATALOG
        .iter()
        .map(|probe| dispatch(probe, inputs))
        .collect()
}

/// Runs the hard subset in catalog order and returns the first failure, if any.
///
/// This is the preflight guard's whole implementation: same probes, same order, same messages as
/// `viv doctor`, so the two cannot disagree about what a usable host is.
#[must_use]
pub fn first_hard_failure<E: Environment>(inputs: &Inputs<'_, E>) -> Option<Finding> {
    CATALOG
        .iter()
        .filter(|probe| probe.severity == Severity::Hard)
        .map(|probe| dispatch(probe, inputs))
        .find(|finding| finding.status == Status::Fail)
}

fn dispatch<E: Environment>(probe: &'static Probe, inputs: &Inputs<'_, E>) -> Finding {
    match probe.scope {
        Scope::Host => host::run(probe, inputs),
        Scope::Project => {
            // Ambiguity answers two probes before the project-scope skip can hide them, and it is
            // the same finding every manifest-resolving verb refuses on (ADR-0109). `doctor`
            // reports rather than refuses, which is the whole point of the second consumer.
            if let Some((message, hint)) = &inputs.ownership_ambiguity
                && matches!(probe.id, "manifest-resolves" | "working-directory-declared")
            {
                return Finding::tripped(probe, message.clone(), hint.clone());
            }
            inputs.project.as_ref().map_or_else(
                || Finding::skipped(probe, "no-manifest-bound", ""),
                |project| project::run(probe, project, inputs),
            )
        }
        Scope::Network => {
            if inputs.online {
                network::run(probe, inputs)
            } else {
                Finding::skipped(probe, "offline-mode", "")
            }
        }
    }
}
