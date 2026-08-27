//! The reclaim verbs: the memory rung, and the `viv trim` fan-out over both rungs (ADR-0113).
//!
//! `viv memory trim` is the one reclaim N23 leaves the user: the guest returns free memory
//! continuously, but page cache is not free memory, so only an explicit ask can bring it back.
//! The ask is the balloon — inflate to the target so the guest reclaims its own caches first,
//! hold until the scope charge settles, deflate so the guest can take everything back the moment
//! it needs it — and the report is a before/after pair of the same scope charge `viv status`
//! reports, floored at zero (spec/17, spec/01). The disk rung lives in [`super::volume`], beside
//! the enumeration it reads; this module owns its face and the fan-out over both rungs.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use super::grammar::Output;
use super::lifecycle::{self, Runtime};
use super::{Context, Failure, Success, diagnosed, render};
use crate::config::{self, Environment};
use crate::diagnostic::{Locus, Namespace};
use crate::exit::ExitKind;
use crate::launch::BootMetadata;
use crate::launch::control::{self, ControlCallError};

/// How long the guest has to answer the meminfo query; a `/proc/meminfo` read is fast, and a
/// guest that cannot answer this quickly cannot be asked to reclaim either.
const MEMINFO_TIMEOUT: Duration = Duration::from_secs(2);

/// How long the guest has to finish a requested trim. Generous, because `fstrim` walks every
/// free extent of each filesystem and the acknowledgement promises completion (spec/12).
const TRIM_TIMEOUT: Duration = Duration::from_mins(2);

/// The settle cadence and bound between the inflate and the deflate.
///
/// The charge is the reported metric, and it moves only on the actual free of the backing
/// object's pages (ADR-0082) — so the charge stabilizing after a fall is the operation
/// completing, observed on the same instrument the record uses. The bound turns a guest that
/// never reaches the target (a busy one, or `deflate_on_oom` pushing back) into a smaller
/// honest figure rather than a hang.
const SETTLE_POLL: Duration = Duration::from_millis(250);
const SETTLE_DEADLINE: Duration = Duration::from_secs(10);

/// The least headroom the derived target adds above the working set.
///
/// spec/17 fixes the intent — enough to drop cache, not enough to disturb running work — and
/// the floor keeps a small working set from deriving a target so tight the guest starts
/// reclaiming what it is about to need again.
const HEADROOM_FLOOR_BYTES: u64 = 512 * 1024 * 1024;

const MIB: u64 = 1024 * 1024;

/// What the memory rung measured: the target it asked for, and the pair it read (spec/01).
pub(super) struct MemoryTrimReading {
    pub target_mib: u64,
    pub before: u64,
    pub after: u64,
}

/// `viv memory trim [--to <MiB>] [--json]` (spec/17, ADR-0113).
pub(super) async fn memory<E: Environment + Sync>(
    context: &Context<'_, E>,
    to_mib: Option<u64>,
    output: Output,
) -> Result<Success, Failure> {
    let (sandbox_id, reading) = memory_rung(context, to_mib).await?;
    Ok(Success::plain(if output.is_json() {
        render::memory_trim_json(&sandbox_id, &reading)
    } else {
        render::memory_trim_line(&reading)
    }))
}

/// `viv volume trim [<name>] [--json]` (spec/17, ADR-0113).
pub(super) async fn volume<E: Environment + Sync>(
    context: &Context<'_, E>,
    name: Option<&str>,
    output: Output,
) -> Result<Success, Failure> {
    let rows = super::volume::trim_rows(context, name).await?;
    Ok(Success::plain(if output.is_json() {
        render::volume_trim_json(&super::volume::trim_rows_json(&rows))
    } else {
        render::volume_trim_human(&super::volume::trim_rows_human(&rows), trim_total(&rows))
    }))
}

/// The volume record's top-level total: the sum of the per-row floors, so the common question
/// needs no client-side arithmetic (spec/01).
fn trim_total(rows: &[super::volume::TrimRow]) -> u64 {
    rows.iter()
        .map(|row| row.before.saturating_sub(row.after))
        .sum()
}

/// `viv trim [--json]` — both rungs in one invocation, memory first (ADR-0113).
///
/// Memory leads because it is the only reclaim N23 leaves the user, while the guest trims
/// volumes on its own schedule. The partial-failure rule is `viv stop --all`'s, inherited whole:
/// a failing rung never skips the other, each failure is named on stderr as it happens, the
/// first failure's category is the exit, and a failed run emits no record. The human face
/// streams each rung's line as that rung completes (spec/01), so a person sees what came back
/// before a later failure is reported — the machine face cannot, which is the cost ADR-0113
/// records.
pub(super) async fn fan_out<E: Environment + Sync>(
    context: &Context<'_, E>,
    output: Output,
) -> Result<Success, Failure> {
    let mut first_failure = None;

    let memory_record = match memory_rung(context, None).await {
        Ok((sandbox_id, reading)) => {
            stream_line(output, &render::memory_trim_line(&reading));
            Some((sandbox_id, reading))
        }
        Err(failure) => {
            context.ui.warn("could not trim memory");
            lifecycle::record_failure(&mut first_failure, failure);
            None
        }
    };

    let volume_record = match super::volume::trim_rows(context, None).await {
        Ok(rows) => {
            let total = trim_total(&rows);
            stream_line(
                output,
                &render::volume_trim_human(&super::volume::trim_rows_human(&rows), total),
            );
            Some(rows)
        }
        Err(failure) => {
            context.ui.warn("could not trim volumes");
            lifecycle::record_failure(&mut first_failure, failure);
            None
        }
    };

    if let Some(failure) = first_failure {
        return Err(failure);
    }
    // Both rungs succeeded, so both subtrees exist — the only condition under which a record is
    // emitted at all (spec/01).
    let (Some((sandbox_id, memory)), Some(volumes)) = (memory_record, volume_record) else {
        unreachable!("a missing rung result always records a failure");
    };
    Ok(Success::plain(if output.is_json() {
        render::trim_json(
            &sandbox_id,
            &memory,
            &super::volume::trim_rows_json(&volumes),
        )
    } else {
        // Already streamed rung by rung; an end-of-run repeat would print every figure twice.
        String::new()
    }))
}

/// The human face's streaming write: each rung's line lands as that rung completes (spec/01).
///
/// Straight to stdout rather than through [`Success`], because the point is what a person sees
/// before a later rung fails; the machine face stays silent here so `--json` remains one object
/// on one line.
fn stream_line(output: Output, line: &str) {
    if !output.is_json() {
        print!("{line}");
        let _ = std::io::Write::flush(&mut std::io::stdout());
    }
}

/// The whole memory rung, one routine for both faces (the rule `stop_one` carries: the fan-out
/// reporting success for the rung is exactly this function returning `Ok`).
async fn memory_rung<E: Environment + Sync>(
    context: &Context<'_, E>,
    to_mib: Option<u64>,
) -> Result<(String, MemoryTrimReading), Failure> {
    // Manifest first, then the runtime root — ADR-0109's ordering contract; see `status`.
    let resolved = super::resolve_manifest_for_launch(context)?;
    let runtime_root = config::resolve_runtime_root(context.environment, config::effective_uid())
        .map_err(|error| super::resolution_failure(&error))?;
    let sandbox_id = resolved.selected.name.clone();
    let runtime = Runtime::locate(&runtime_root, &sandbox_id, super::DEFAULT_TARGET)?;
    // One reclaim at a time per target (ADR-0053's lock, the one `start` and the pruning
    // verbs already take): overlapping trims would overwrite each other's balloon size, and
    // neither pair would bracket the operation whose target it reports. Held across the whole
    // sequence, deflate included, and taken before the state is read so the answer stays true
    // for as long as it is used.
    let _lock = lifecycle::TargetLock::acquire(&runtime)?;
    require_running(context, &runtime, &sandbox_id)?;
    let boot = live_boot(&runtime)?;

    // The pair's metric is the scope charge `status` reports (spec/01 requires the two commands
    // to be directly comparable), so a host that cannot read it cannot substantiate a figure —
    // and the one outcome the slice's core forbids is reporting one anyway.
    let before = measured_charge(&runtime)?;

    let spec = lifecycle::launch_spec_record(&runtime)?;
    let declared_mib = spec.resources.memory_mib;
    let target_mib = derive_target(&runtime, &boot, to_mib, declared_mib, context).await?;
    let balloon_mib = declared_mib.saturating_sub(target_mib);

    if balloon_mib > 0 {
        let ch_remote = &spec.backend_programs.ch_remote;
        let api_socket = &spec.runtime_paths.api_socket;
        resize_balloon(ch_remote, api_socket, balloon_mib)?;
        settle(&runtime, before).await;
        // Restoring the headroom is part of the same operation, never a later step (spec/17):
        // every path that inflated deflates, a timed-out settle included, so the guest can take
        // memory back the moment it needs it.
        resize_balloon(ch_remote, api_socket, 0)?;
    }

    let after = measured_charge(&runtime)?;
    Ok((
        sandbox_id,
        MemoryTrimReading {
            target_mib,
            before,
            after,
        },
    ))
}

/// The `75` refusal both rungs share: reclaiming is an act on a running guest (spec/14).
pub(super) fn require_running<E: Environment>(
    context: &Context<'_, E>,
    runtime: &Runtime,
    sandbox_id: &str,
) -> Result<(), Failure> {
    let built = lifecycle::last_build(&context.roots, sandbox_id, super::DEFAULT_TARGET).is_some();
    if matches!(
        lifecycle::discriminate(runtime, built),
        lifecycle::State::Running
    ) {
        return Ok(());
    }
    Err(diagnosed(
        Namespace::Vm,
        "trim-not-running",
        "this project's VM is not running",
        Locus::Named("runtime directory"),
        "a reclaim asks the running guest to give something back, so there is nothing to ask",
        ExitKind::TempFail,
    )
    .with_hint("`viv start` first, then trim"))
}

/// The boot metadata a reclaim call authorizes against, or the refusal reading it earns.
pub(super) fn live_boot(runtime: &Runtime) -> Result<BootMetadata, Failure> {
    match lifecycle::read_boot_record(runtime) {
        lifecycle::BootRecord::Ready(boot) => Ok(boot),
        lifecycle::BootRecord::Skewed(theirs) => Err(lifecycle::boot_record_skew(runtime, theirs)),
        lifecycle::BootRecord::Absent | lifecycle::BootRecord::Unreadable => Err(diagnosed(
            Namespace::Vm,
            "boot-record-unreadable",
            "the running VM's boot record cannot be read",
            Locus::File(runtime.directory.join("boot.json")),
            "without it no call against the running guest can be authorized",
            ExitKind::IoErr,
        )),
    }
}

/// The two-way failure split every reclaim control call shares (spec/14).
pub(super) fn control_failure(error: ControlCallError, asked_for: &str) -> Failure {
    match error {
        ControlCallError::Unreachable => diagnosed(
            Namespace::Vm,
            "control-socket-unavailable",
            "the running VM's agent cannot be reached",
            Locus::Named("control socket"),
            format!("the guest agent did not answer the {asked_for} request"),
            ExitKind::Unavailable,
        ),
        ControlCallError::Protocol => diagnosed(
            Namespace::Vm,
            "control-socket-failed",
            "the control channel broke mid-exchange",
            Locus::Named("control socket"),
            format!("the {asked_for} request failed after an authorized handshake"),
            ExitKind::IoErr,
        ),
    }
}

/// The guest-side trim ask the volume rung makes, wrapped so the timeout has one owner.
pub(super) async fn request_guest_trim(
    runtime: &Runtime,
    boot: &BootMetadata,
    mountpoints: Vec<String>,
) -> Result<(), Failure> {
    control::request_trim(&runtime.control_socket(), boot, mountpoints, TRIM_TIMEOUT)
        .await
        .map_err(|error| control_failure(error, "trim"))
}

/// One scope-charge reading, refused rather than fabricated when the controller is not there.
fn measured_charge(runtime: &Runtime) -> Result<u64, Failure> {
    lifecycle::unit_memory_and_cgroup(&runtime.unit)
        .0
        .ok_or_else(|| {
            diagnosed(
                Namespace::Host,
                "memory-unreadable",
                "the sandbox's memory charge cannot be read",
                Locus::Named("systemd user scope"),
                "the scope charge is the figure this command reports, and a figure it cannot read \
            is a figure it will not fabricate",
                ExitKind::IoErr,
            )
            .with_hint("`viv doctor` checks the user manager's cgroup memory accounting")
        })
}

/// The figure the guest is asked to reach, in MiB, never absent for a completed run (spec/01).
///
/// The operator's `--to` wins; without it the target is the guest's own working set plus
/// headroom — total minus `MemAvailable`, the kernel's estimate of what can be reclaimed
/// without swapping, which is exactly "enough to drop cache, not enough to disturb running
/// work" (spec/17). An operator figure below the guest's floor is clamped up with a note,
/// because the guest would refuse the difference anyway (`deflate_on_oom`); a guest that cannot
/// answer the query leaves an operator figure standing and a derived target impossible.
async fn derive_target<E: Environment + Sync>(
    runtime: &Runtime,
    boot: &BootMetadata,
    to_mib: Option<u64>,
    declared_mib: u64,
    context: &Context<'_, E>,
) -> Result<u64, Failure> {
    let report = control::memory_report(&runtime.control_socket(), boot, MEMINFO_TIMEOUT).await;
    if let Some(to) = to_mib {
        // The clamp is best-effort: an operator figure stands even when the guest cannot
        // answer, because the guest refuses an under-floor target itself (`deflate_on_oom`).
        if let Ok(report) = &report {
            let floor = working_set_mib(report.total_bytes, report.available_bytes);
            if to < floor {
                context.ui.warn(&format!(
                    "raising the target from {to} MiB to the guest's own floor of {floor} MiB"
                ));
                return Ok(floor);
            }
        }
        Ok(to)
    } else {
        // Without `--to` the derivation is the run: a guest that cannot report leaves no
        // honest target, and the reported `target_mib` is never absent for a completed run.
        let report = report.map_err(|error| control_failure(error, "memory"))?;
        let working_set = working_set_mib(report.total_bytes, report.available_bytes);
        Ok(headroom_target(working_set).min(declared_mib.max(1)))
    }
}

/// The working set the guest itself reports, rounded up to whole MiB.
const fn working_set_mib(total_bytes: u64, available_bytes: u64) -> u64 {
    let working_set = total_bytes.saturating_sub(available_bytes);
    working_set.div_ceil(MIB)
}

/// Working set plus headroom: a quarter of itself, floored at half a GiB.
const fn headroom_target(working_set_mib: u64) -> u64 {
    let floor_mib = HEADROOM_FLOOR_BYTES / MIB;
    let headroom = if working_set_mib / 4 > floor_mib {
        working_set_mib / 4
    } else {
        floor_mib
    };
    working_set_mib + headroom
}

/// One `ch-remote resize --balloon` against the pinned backend (ADR-0049: never `$PATH`).
///
/// A missing socket and a refused resize are both the backend being unaskable (`69`); a binary
/// that cannot run at all is host I/O (`74`). The distinction is spec/14's own line for these
/// verbs: agent or backend unreachable against control-socket or trim I/O.
fn resize_balloon(ch_remote: &Path, api_socket: &Path, balloon_mib: u64) -> Result<(), Failure> {
    if !api_socket.exists() {
        return Err(diagnosed(
            Namespace::Vm,
            "api-socket-unavailable",
            "the backend's API socket is gone",
            Locus::File(api_socket.to_path_buf()),
            "the balloon is driven over the backend's API socket, and the running VM's \
            runtime directory no longer holds one",
            ExitKind::Unavailable,
        ));
    }
    let output = Command::new(ch_remote)
        .args([
            "--api-socket",
            &api_socket.display().to_string(),
            "resize",
            "--balloon",
            &format!("{balloon_mib}M"),
        ])
        .output()
        .map_err(|error| {
            diagnosed(
                Namespace::Vm,
                "balloon-resize-failed",
                "could not run the pinned `ch-remote`",
                Locus::File(ch_remote.to_path_buf()),
                error.to_string(),
                ExitKind::IoErr,
            )
        })?;
    if !output.status.success() {
        return Err(diagnosed(
            Namespace::Vm,
            "balloon-resize-refused",
            "the backend refused the balloon resize",
            Locus::File(api_socket.to_path_buf()),
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            ExitKind::Unavailable,
        ));
    }
    Ok(())
}

/// Wait, bounded, for the scope charge to stop moving after the inflate.
///
/// Advisory only: the authoritative "after" is read once the headroom is restored, so a settle
/// that times out costs a smaller figure, never a wrong one. Two consecutive readings within
/// one percent of each other, after the charge has fallen below its starting point, count as
/// settled; an unreadable poll ends the wait rather than extending it.
async fn settle(runtime: &Runtime, before: u64) {
    let deadline = tokio::time::Instant::now() + SETTLE_DEADLINE;
    let mut last: Option<u64> = None;
    while tokio::time::Instant::now() < deadline {
        tokio::time::sleep(SETTLE_POLL).await;
        let Some(now) = lifecycle::unit_memory_and_cgroup(&runtime.unit).0 else {
            return;
        };
        if let Some(last) = last
            && now < before
            && last.abs_diff(now) * 100 <= last.max(1)
        {
            return;
        }
        last = Some(now);
    }
}
