//! The `boot` lane: the workflow trials that start a real microVM.
//!
//! Every trial here builds a guest closure and boots it behind a hardware
//! virtualization boundary, then asserts on what the running guest does — a command's
//! exit code, a mount's contents, an egress denial, a reclaimed page. This is the
//! authoritative lane and the expensive one: it costs a guest kernel, a VMM, and one
//! virtiofsd per share before it asserts anything.
//!
//! The lane declares `/dev/kvm`, a systemd user manager, `XDG_RUNTIME_DIR` and Nix,
//! once, in `main`. A host without them fails every trial with that reason. It never
//! skips: a skipped acceptance trial that reads as a pass is the failure mode the
//! lane split exists to remove, and `docs/reference/implementation-status.md` already
//! refuses to count one as evidence.
//!
//! `.config/nextest.toml` bounds this binary to one boot at a time. The bound is on
//! the binary rather than on a list of names, so a trial added here is inside it by
//! construction.
//!
//! Its siblings are `local_workflows` (needs nothing) and `eval_workflows` (needs
//! Nix). The lane register is `docs/reference/testing-lanes.md`.

mod support;

use std::fs;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use libtest_mimic::{Arguments, Failed, Trial};
use support::{
    EX_CONFIG, EX_DATAERR, EX_IOERR, EX_TEMPFAIL, EX_UNAVAILABLE, TempProject, VOLUME_TAIL,
    VivOutput, arrange_egress_fixture, arrange_manifest, arrange_manifest_with_image, check,
    expect_code, expect_derived_manifest_visible, expect_json_array_items, expect_json_keys,
    expect_json_string, expect_nonzero, expect_stderr_mentions, expect_stdout_lacks,
    expect_stdout_mentions, expect_tree_unchanged, expect_volume_image, fail, io_failed,
    json_record, preflight, project_rows, run_viv, snapshot_tree, viv, viv_at, viv_environment,
    viv_with_env, volume_image, write_file, write_piece,
};

fn main() -> std::process::ExitCode {
    // The egress fixture re-executes this binary inside the VM's namespaces, so a
    // fixture-mode invocation never reaches the trial harness. First statement of
    // `main`, before argument parsing, because the mode word is not a filter.
    if let Some(code) = support::egress::run_mode() {
        return code;
    }
    let args = Arguments::from_args();
    // After argument parsing and never before it: nextest builds its test list by
    // running this binary with `--list`, and a listing that refuses on an unmet need
    // would abort the whole run rather than failing this lane's own trials.
    if !args.list
        && let Err(reason) = preflight::boot()
    {
        eprintln!("the boot lane needs a virtualization-capable host: {reason}");
        return std::process::ExitCode::from(69);
    }
    let trials = vec![
        Trial::test(
            "workflow_01_manifest_workspace_resolution_boot",
            workflow_01_boot,
        ),
        Trial::test(
            "workflow_05_restrict_egress_allowlist",
            workflow_05_enforcement,
        ),
        Trial::test(
            "workflow_06_exec_exit_code_propagation",
            workflow_06_propagation,
        ),
        Trial::test("workflow_06_shell_interactive_session", workflow_06_shell),
        Trial::test(
            "workflow_07_stop_restart_preserving_volumes",
            workflow_07_warmth,
        ),
        Trial::test(
            "workflow_07_stop_completes_within_budget",
            workflow_07_shutdown_bounded,
        ),
        Trial::test("workflow_08_destroy_cold_rebuild", workflow_08_rebuild),
        Trial::test("workflow_09_workspace_round_trip", workflow_09_round_trip),
        Trial::test(
            "workflow_15_contract_skew_refusal",
            workflow_15_contract_skew,
        ),
        Trial::test(
            "workflow_15_contract_skew_live",
            workflow_15_contract_skew_live,
        ),
        Trial::test("workflow_17_declared_mounts_refusals", workflow_17_refusals),
        Trial::test(
            "workflow_17_declared_mounts_round_trip",
            workflow_17_round_trip,
        ),
        Trial::test(
            "workflow_17_linked_worktree_reaches_main",
            workflow_17_worktree,
        ),
        Trial::test(
            "workflow_20_many_workspaces_one_sandbox",
            workflow_20_many_workspaces,
        ),
        Trial::test(
            "workflow_22_file_mount_serves_only_its_file",
            workflow_22_file_mount_confinement,
        ),
        Trial::test("workflow_23_agent_channel_relay", workflow_23_relay),
        Trial::test("workflow_24_generations_retention", workflow_24_retention),
        Trial::test("workflow_25_sessions_counted", workflow_25_sessions_count),
        Trial::test(
            "workflow_25_fleet_two_sandboxes",
            workflow_25_fleet_two_sandboxes,
        ),
        Trial::test(
            "workflow_26_admission_refusal",
            workflow_26_admission_refusal,
        ),
        Trial::test("workflow_26_start_attach_stream", workflow_26_attach_stream),
        Trial::test(
            "workflow_27_stop_agent_rung_evidence",
            workflow_27_agent_rung,
        ),
        Trial::test("workflow_27_stop_slow_guest_grace", workflow_27_slow_guest),
        Trial::test("workflow_27_stop_all_sweep", workflow_27_sweep),
        Trial::test(
            "workflow_28_memory_trim_reclaims",
            workflow_28_memory_reclaims,
        ),
        Trial::test(
            "workflow_28_volume_trim_returns_blocks",
            workflow_28_volume_trim,
        ),
    ];
    libtest_mimic::run(&args, trials).exit_code()
}

/// Resolves a fetch tool inside the guest instead of assuming one. The guest image is
/// specified as carrying Nix-with-flakes and direnv (spec/06) and nothing else, so a
/// hard-coded `wget` would be the same unguaranteed-toolchain assumption the plan
/// removed for `bash`. The paired guide writes this as a `fetch-command` placeholder.
const GUEST_FETCH: &str = concat!(
    "if command -v curl >/dev/null 2>&1; then curl -fsS \"$1\"; ",
    "elif command -v wget >/dev/null 2>&1; then wget -qO- \"$1\"; ",
    "else echo 'no fetch tool in guest image' >&2; exit 69; fi"
);

/// How long a denied fetch may take before the denial counts as a drop rather than a
/// rejection (spec/05, ADR-0044). Generous on purpose: a rejected connection fails in
/// milliseconds, while a dropped one burns through SYN retries for a minute or more, so
/// anything in between still separates the two without making the trial flaky on a slow
/// guest.
const DENIAL_BUDGET: Duration = Duration::from_secs(20);

// Guide: docs/guides/first-time-workspace-boot.md
fn workflow_01_boot() -> Result<(), Failed> {
    let tp = TempProject::new().map_err(io_failed)?;
    arrange_manifest(&tp, "rust-web", "", "")?;
    let before = snapshot_tree(tp.project()).map_err(io_failed)?;

    check(expect_code(&viv(&tp, &["start"])?, 0))?;
    // N9 is absolute: even the first command that boots a sandbox leaves the complete user tree,
    // including the absence of `.vivarium`, byte-for-byte unchanged.
    check(expect_tree_unchanged(tp.project(), &before))?;
    check(expect_code(&viv(&tp, &["start"])?, 0))?;

    let status = viv(&tp, &["status", "--json"])?;
    check(expect_code(&status, 0))?;
    check(expect_json_keys(&status, &["state", "stale"]))?;
    // `start` without `--attach` exiting 0 guarantees `running` at the moment it
    // returned (spec/10). This is the strongest check available from outside the
    // process: it catches an implementation that returns early and leaves the VM
    // `starting`, but not one that returns early and happens to settle before this
    // separate `status` runs. Proving the return boundary itself needs a fixture that
    // can hold the readiness handshake open, which arrives with the agent (section E).
    check(expect_json_string(&status, "state", "running"))?;

    // This manifest declares no egress, so this guest runs open mode, which spec/05
    // ships with no filter and no resolver — absent, not inert. The absence is
    // asserted on the guest this trial already booted rather than paying a second
    // boot for it elsewhere.
    let vm_pid = support::egress::read_vm_pid(tp.runtime()).map_err(Failed::from)?;
    check(support::egress::expect_open_mode_absence(vm_pid))
}

// Guide: docs/guides/restrict-egress-allowlist.md
fn workflow_05_enforcement() -> Result<(), Failed> {
    use support::egress;

    let tp = arrange_egress_fixture()?;
    check(expect_code(&viv(&tp, &["start"])?, 0))?;

    // spec/05's proving fixture: a stub upstream and an endpoint listener inside
    // the VM's own network namespace, two `.test` names on two different
    // addresses, only the first in `egress.allow`. The VMM's recorded pid is the
    // handle into the pair.
    let vm_pid = egress::read_vm_pid(tp.runtime()).map_err(Failed::from)?;
    let fixture = egress::EgressFixture::install(vm_pid).map_err(Failed::from)?;

    // The denial code, read at the resolver itself: `REFUSED` means policy and
    // nothing else may wear it, while an allowlisted name whose upstream answer
    // is `NXDOMAIN` passes that through verbatim. This pair of observations is
    // Q-022's exit — a guest exit code cannot carry the distinction.
    let denied_rcode = fixture
        .resolver_rcode(egress::DENIED_NAME)
        .map_err(Failed::from)?;
    if denied_rcode != "Refused" {
        return fail(format!(
            "a denied name must be REFUSED at the resolver; got {denied_rcode}"
        ));
    }
    let ghost_rcode = fixture
        .resolver_rcode(egress::GHOST_NAME)
        .map_err(Failed::from)?;
    if ghost_rcode != "NXDomain" {
        return fail(format!(
            "an allowlisted name whose upstream says NXDOMAIN must pass it \
            through verbatim; got {ghost_rcode}"
        ));
    }

    // The positive leg an absent uplink cannot fake: the allowed name resolves,
    // its address is installed before release, and the connection crosses the
    // filter's forward chain into the endpoint namespace and returns bytes.
    let allowed = viv(
        &tp,
        &[
            "exec",
            "--",
            "sh",
            "-lc",
            GUEST_FETCH,
            "fetch",
            &format!("http://{}/", egress::ALLOWED_NAME),
        ],
    )?;
    check(expect_code(&allowed, 0))?;
    check(expect_stdout_mentions(&allowed, egress::ALLOWED_BODY))?;

    // The denied name: non-zero, promptly. No exit code describes a denial —
    // enforcement is host-side but the failure is observed by a guest process,
    // and after guest-process start `exec` returns that process's status
    // verbatim — so the status assertion stays at "non-zero", and reject-not-drop
    // is held by timing rather than by whichever fetch tool the image ships.
    let started = Instant::now();
    let denied = viv(
        &tp,
        &[
            "exec",
            "--",
            "sh",
            "-lc",
            GUEST_FETCH,
            "fetch",
            &format!("http://{}/", egress::DENIED_NAME),
        ],
    )?;
    let elapsed = started.elapsed();
    check(expect_nonzero(&denied))?;
    if elapsed >= DENIAL_BUDGET {
        return fail(format!(
            "denied fetch took {elapsed:?}; a rejection must fail fast rather \
            than hang on connect retries (spec/05, ADR-0044)"
        ));
    }

    // The address-level half: the second endpoint is reachable in the fixture and
    // admitted by nothing, so a connect by literal address must be rejected on
    // the forward chain — fast, as a reset rather than a drop. Two names on two
    // addresses is what separates this name-level decision from an address-level
    // one, which is why the addresses must differ (spec/05).
    let started = Instant::now();
    let denied_addr = viv(
        &tp,
        &[
            "exec",
            "--",
            "sh",
            "-lc",
            GUEST_FETCH,
            "fetch",
            &format!("http://{}/", egress::DENIED_ADDR),
        ],
    )?;
    let elapsed = started.elapsed();
    check(expect_nonzero(&denied_addr))?;
    if elapsed >= DENIAL_BUDGET {
        return fail(format!(
            "denied literal connect took {elapsed:?}; the reject rules must \
            answer with a reset rather than a drop (spec/05, ADR-0044)"
        ));
    }
    drop(fixture);
    Ok(())
}

// Guide: docs/guides/run-agent-command.md
fn workflow_06_propagation() -> Result<(), Failed> {
    let tp = TempProject::new().map_err(io_failed)?;
    arrange_manifest(&tp, "agent-command", "", "")?;
    // No `viv start` first, on purpose: spec/12's ensure-running says a command that needs a VM
    // brings one up. This very invocation is the cold start, and it is also the first half of the
    // reuse assertion: the guest's own boot id, which the kernel mints once per boot, is read here
    // so the value being compared against belongs to the VM this command booted. Reading it any
    // later would leave the first reuse decision — the one immediately after the cold start —
    // outside the comparison, and an implementation that replaced the VM exactly once would pass.
    //
    // Wall clock cannot stand in for this. Nothing in this trial asserts elapsed time, so a `stop`
    // and a re-boot between every command would satisfy every other assertion in it.
    let boot_id = |out: &VivOutput| String::from_utf8_lossy(&out.stdout).trim().to_owned();
    let first = viv(
        &tp,
        &["exec", "--", "cat", "/proc/sys/kernel/random/boot_id"],
    )?;
    check(expect_code(&first, 0))?;
    let booted = boot_id(&first);
    if booted.is_empty() {
        return Err(Failed::from(
            "the guest reported no boot id to compare reuse against",
        ));
    }
    check(expect_json_string(
        &viv(&tp, &["status", "--json"])?,
        "state",
        "running",
    ))?;

    check(expect_code(&viv(&tp, &["exec", "--", "false"])?, 1))?;
    check(expect_code(
        &viv(&tp, &["exec", "--", "sh", "-lc", "exit 42"])?,
        42,
    ))?;

    // A second `--` after the boundary is an ordinary guest argument, passed through
    // byte-for-byte with no shell splitting or expansion.
    let boundary = viv(
        &tp,
        &["exec", "--", "sh", "-lc", "printf '%s' \"$1\"", "sh", "--"],
    )?;
    check(expect_code(&boundary, 0))?;
    check(expect_stdout_mentions(&boundary, "--"))?;

    // Host environment passthrough is deny-by-default (spec/12, N17), and this is the assertion
    // that makes that a fact about a running guest rather than about a function. `TMPDIR` is the
    // load-bearing name: the harness deliberately passes it through to `viv` itself, so a guest
    // that could see it would be seeing a variable this very process was given — which is exactly
    // the leak the rule exists to stop, and exactly what a unit test of the policy cannot catch.
    // `PATH` is the other half: the guest gets its own, never the host's.
    let environment = viv(
        &tp,
        &[
            "exec",
            "--env",
            "NAMED=carried",
            "--",
            "sh",
            "-lc",
            "printf 'named=%s tmpdir=%s path=%s' \"${NAMED-unset}\" \"${TMPDIR-unset}\" \"$PATH\"",
        ],
    )?;
    check(expect_code(&environment, 0))?;
    check(expect_stdout_mentions(&environment, "named=carried"))?;
    check(expect_stdout_mentions(&environment, "tmpdir=unset"))?;
    check(expect_stdout_mentions(
        &environment,
        "/run/current-system/sw/bin",
    ))?;
    check(expect_stdout_lacks(&environment, "path=/nix/store"))?;

    // Assumes the specified guest toolchain supplies POSIX sh. The exec'd shell
    // signals itself, so this is a genuinely signal-killed guest process — not a
    // shell converting a signal into an exit code.
    check(expect_code(
        &viv(
            &tp,
            &["exec", "--", "sh", "-lc", "exec sh -c 'kill -TERM $$'"],
        )?,
        143,
    ))?;

    // Reuse, closed. Every command above ran against the VM the first `exec` booted, and this is
    // the read that says so: a guest that had been stopped and re-booted anywhere in between would
    // answer with a different boot id here, and the equality would fail.
    let last = viv(
        &tp,
        &["exec", "--", "cat", "/proc/sys/kernel/random/boot_id"],
    )?;
    check(expect_code(&last, 0))?;
    let after = boot_id(&last);
    if after != booted {
        return Err(Failed::from(format!(
            "ensure-running booted a second VM: the guest boot id was {booted} \
            before the session commands and {after} after"
        )));
    }

    // The other side of the exit-status boundary, and the one a passing suite is least likely to
    // notice is missing. Everything above returns a number the guest chose; this returns a number
    // vivarium chose, because the guest's own answer was lost in transit.
    //
    // spec/12 is explicit that the transport dying after guest start is `74` and that no
    // guest-shaped code may be guessed for it — the command may well have succeeded, and any
    // number reported for it would be inventing the one fact that went missing. So the session is
    // put in flight against a guest that is then taken away underneath it.
    let mut inflight = spawn_viv(&tp, &["exec", "--", "sh", "-lc", "sleep 60"])?;
    // Long enough for the guest process to exist, which is what puts this after the boundary
    // rather than before it. A refusal before spawn is a different code and a different clause.
    std::thread::sleep(Duration::from_secs(3));
    check(expect_code(&viv(&tp, &["stop"])?, 0))?;
    let severed = inflight.wait().map_err(io_failed)?;
    if severed.code() != Some(EX_IOERR) {
        return Err(Failed::from(format!(
            "a transport that died after guest start returned {:?}, not {EX_IOERR}",
            severed.code()
        )));
    }
    Ok(())
}

/// Starts `viv` without waiting for it, for the one assertion that needs a session still running.
///
/// [`run_viv`] runs to completion by construction, which is right for every other trial here and
/// cannot express "kill the VM while this is talking to it".
fn spawn_viv(tp: &TempProject, args: &[&str]) -> Result<std::process::Child, Failed> {
    let mut command = std::process::Command::new(preflight::viv());
    command
        .args(args)
        .current_dir(tp.project())
        .env_clear()
        .envs(support::viv_environment(tp))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    command.spawn().map_err(io_failed)
}

/// The one trial that cannot use [`run_viv`], because the thing under test is the terminal.
///
/// Everything else in this file runs `viv` with its streams captured and no controlling terminal,
/// which is the right shape for a verb that produces output. `viv shell` produces a session: it
/// puts the host terminal in raw mode, sends its size before the guest process starts, follows a
/// later resize, and has to put the terminal back on every way out. None of those is observable
/// through a pipe, and three of them are acceptance assertions of slice 013.
///
/// So the child gets a real pty and this trial holds the other end. What it asserts from there is
/// what a user would notice if it broke: the guest agreed about the size before the shell drew
/// anything, job control is on, a window resize reached the guest, and a terminal handed to a
/// session that is then killed comes back the way it was lent.
// Guide: docs/guides/run-agent-command.md
fn workflow_06_shell() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("shell-project").map_err(io_failed)?;
    arrange_manifest(&tp, "interactive", "", "")?;
    check(expect_code(&viv(&tp, &["start"])?, 0))?;

    let (mut pty, pts) = pty_process::blocking::open().map_err(|error| pty_failed(&error))?;
    // Non-blocking, so [`ask`]'s deadline is a deadline. A blocking read waiting for output a
    // broken session will never produce hangs until the harness kills the whole run, and reports
    // "timed out" rather than what did come back — which is the one thing worth knowing.
    rustix::io::ioctl_fionbio(&pty, true).map_err(errno_failed)?;
    // Wider than the default 80 columns because every line this trial sends is echoed back by the
    // guest's readline, and a line that wraps arrives with escape sequences threaded through the
    // token being matched. The width is asserted rather than assumed, so it costs nothing to
    // choose one that leaves room.
    pty.resize(pty_process::Size::new(40, 120))
        .map_err(|error| pty_failed(&error))?;
    // Read before the session starts, so the comparison after it ends is against this host's own
    // settings rather than against an assumption about what a terminal looks like.
    let lent = rustix::termios::tcgetattr(&pty).map_err(errno_failed)?;

    let mut child = pty_process::blocking::Command::new(preflight::viv())
        .arg("shell")
        .current_dir(tp.project())
        .env_clear()
        .envs(support::viv_environment(&tp))
        .spawn(pts)
        .map_err(|error| pty_failed(&error))?;

    // Wait for the shell before typing at it. A login shell inside a guest that has just booted is
    // not ready the instant the session opens, and a line delivered before its readline is prepared
    // is echoed by the line discipline and then lost — which reads as a broken session and is not
    // one. Measured: the first attempt at this trial failed exactly that way, with the guest's own
    // prompt visible in the capture beside a command that never ran.
    settle(&mut pty)?;
    // A marker rather than a prompt: the guest's shell prompt is the image's business, and a trial
    // that waited for one would be asserting something spec/12 does not fix.
    let ready = ask(&mut pty, "printf 'READY-%s\\n' ok")?;
    if !ready.contains("READY-ok") {
        let _ = child.kill();
        return fail(format!("the guest shell never answered: {ready:?}"));
    }

    // Raw mode, observed from the other end of the same pty. This is what makes the interrupt
    // reach the guest's own foreground process group as a byte rather than being turned into a
    // host signal by the host's line discipline (spec/12).
    let live = rustix::termios::tcgetattr(&pty).map_err(errno_failed)?;
    if live
        .local_modes
        .contains(rustix::termios::LocalModes::ICANON)
        || live.local_modes.contains(rustix::termios::LocalModes::ECHO)
    {
        let _ = child.kill();
        return fail("`viv shell` did not put the host terminal in raw mode");
    }

    // The size the guest saw. Sent before `Start`, so it is right from the first frame the shell
    // draws rather than after a correction the user would see.
    let initial = ask(&mut pty, "stty size")?;
    if !initial.contains("40 120") {
        let _ = child.kill();
        return fail(format!(
            "the guest did not receive the initial size: {initial:?}"
        ));
    }

    // Job control. `$-` carries `m` exactly when the shell has monitor mode, which is the thing
    // that makes a background job, `jobs`, and `fg` work at all.
    let flags = ask(&mut pty, "echo \"flags:$-\"")?;
    if !flags
        .lines()
        .any(|line| line.starts_with("flags:") && line.contains('m'))
    {
        let _ = child.kill();
        return fail(format!("the guest shell has no job control: {flags:?}"));
    }
    let jobs = ask(&mut pty, "sleep 30 & jobs")?;
    if !jobs.contains("[1]") {
        let _ = child.kill();
        return fail(format!("a background job was not tracked: {jobs:?}"));
    }

    // A later resize has to reach the guest, which is what keeps a full-screen program correct
    // when the user drags the window rather than only when they start it.
    pty.resize(pty_process::Size::new(30, 100))
        .map_err(|error| pty_failed(&error))?;
    let resized = ask(&mut pty, "stty size")?;
    if !resized.contains("30 100") {
        let _ = child.kill();
        return fail(format!(
            "a host resize did not reach the guest: {resized:?}"
        ));
    }

    // The exit path that matters, because it is the one that used to leave a terminal unusable
    // after the process that broke it was gone.
    let pid = rustix::process::Pid::from_raw(
        i32::try_from(child.id()).map_err(|error| Failed::from(error.to_string()))?,
    )
    .ok_or_else(|| Failed::from("the shell child has no pid"))?;
    rustix::process::kill_process(pid, rustix::process::Signal::TERM).map_err(errno_failed)?;
    child.wait().map_err(io_failed)?;

    let returned = rustix::termios::tcgetattr(&pty).map_err(errno_failed)?;
    if returned.local_modes != lent.local_modes || returned.input_modes != lent.input_modes {
        return fail("the host terminal was not restored after the session was signalled");
    }
    Ok(())
}

/// Waits for the guest's login shell to finish starting, by waiting for it to stop talking.
///
/// Deliberately not "wait for a prompt": the prompt is the guest image's to choose, and a trial
/// that matched one would fail on an image that changed it for reasons spec/12 says nothing about.
/// Quiet after output is the property that actually matters here — the shell has drawn whatever it
/// draws and is now reading.
fn settle(pty: &mut pty_process::blocking::Pty) -> Result<(), Failed> {
    use std::io::Read as _;
    let deadline = Instant::now() + SHELL_BUDGET;
    let mut buffer = [0u8; 4096];
    let mut seen = false;
    let mut quiet = Instant::now();
    while Instant::now() < deadline {
        match pty.read(&mut buffer) {
            Ok(0) => break,
            Ok(_) => {
                seen = true;
                quiet = Instant::now();
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if seen && quiet.elapsed() >= SHELL_QUIET {
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) => return Err(io_failed(error)),
        }
    }
    fail("the guest shell produced no prompt within the budget")
}

/// Sends one line to the guest shell and collects what comes back before the next marker.
///
/// Bounded by a deadline rather than by a read count, because a shell's output arrives in whatever
/// pieces the pty hands over and a trial that read once would be timing-dependent.
fn ask(pty: &mut pty_process::blocking::Pty, line: &str) -> Result<String, Failed> {
    use std::io::{Read as _, Write as _};
    let token = format!("done-{}", line.len());
    // `\r` rather than `\n`: the guest's terminal is in canonical mode and its line discipline
    // maps carriage return to newline, which is what a real key press delivers.
    write!(pty, "{line}; echo {token}\r").map_err(io_failed)?;
    pty.flush().map_err(io_failed)?;
    let deadline = Instant::now() + SHELL_BUDGET;
    let mut collected = String::new();
    let mut buffer = [0u8; 4096];
    while Instant::now() < deadline {
        match pty.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                collected.push_str(&String::from_utf8_lossy(&buffer[..read]));
                // The echo of the command carries the token too, so the second occurrence is the
                // one the shell produced.
                if collected.matches(&token).count() >= 2 {
                    return Ok(collected);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) => return Err(io_failed(error)),
        }
    }
    fail(format!(
        "the guest shell did not answer `{line}` within {}s: {collected:?}",
        SHELL_BUDGET.as_secs()
    ))
}

fn pty_failed(error: &pty_process::Error) -> Failed {
    Failed::from(error.to_string())
}

fn errno_failed(error: rustix::io::Errno) -> Failed {
    Failed::from(error.to_string())
}

/// How long one round trip through a guest shell may take.
///
/// Generous, because the first one waits for a login shell to finish starting inside a guest that
/// has just booted, and a trial that timed that tightly would report on the host's load.
const SHELL_BUDGET: Duration = Duration::from_mins(1);

/// How long the guest shell must stay silent before it counts as ready for input.
const SHELL_QUIET: Duration = Duration::from_millis(750);

// Guide: docs/guides/stop-restart-preserving-volumes.md
fn workflow_07_warmth() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("volume-project").map_err(io_failed)?;
    arrange_manifest(&tp, "volumes", "", VOLUME_TAIL)?;
    check(expect_code(&viv(&tp, &["start"])?, 0))?;
    let volumes = viv(&tp, &["volume", "list", "--json"])?;
    check(expect_code(&volumes, 0))?;
    check(expect_json_array_items(
        &volumes,
        "volumes",
        &[
            "name",
            "mount",
            "declared_by",
            "orphan",
            "allocated_bytes",
            "virtual_bytes",
        ],
    ))?;
    check(expect_stdout_mentions(&volumes, "default"))?;
    check(expect_stdout_mentions(&volumes, "cache"))?;
    check(expect_volume_image(&tp, "volumes", "default"))?;
    check(expect_volume_image(&tp, "volumes", "cache"))?;

    check(expect_code(&viv(&tp, &["stop"])?, 0))?;
    check(expect_code(&viv(&tp, &["stop"])?, 0))?;
    let status = viv(&tp, &["status", "--json"])?;
    check(expect_code(&status, 0))?;
    // A clean stop lands in `built`, never `absent` — the build output stays pinned
    // (spec/10). Structural, so a `built` appearing elsewhere in the record cannot pass.
    check(expect_json_string(&status, "state", "built"))?;

    check(expect_code(&viv(&tp, &["start"])?, 0))?;
    check(expect_code(
        &viv(
            &tp,
            &["exec", "--", "sh", "-lc", "printf warm > \"$HOME/warm\""],
        )?,
        0,
    ))?;
    check(expect_code(&viv(&tp, &["stop"])?, 0))?;
    check(expect_code(&viv(&tp, &["start"])?, 0))?;
    check(expect_code(
        &viv(&tp, &["exec", "--", "sh", "-lc", "test -f \"$HOME/warm\""])?,
        0,
    ))
}

/// How long a clean `viv stop` may take before the shutdown counts as hung. The guest's
/// store is the host's, shared read-only (ADR-0038), so everything the guest needs in
/// order to shut down — the unmount tooling included — lives on a share the shutdown
/// transaction must not try to release first. spec/06 states that ordering as a contract,
/// and a violation of it does not fail, it hangs: a bounded wall clock is the only
/// falsifier. Generous on purpose, because a deadlock burns the whole stop timeout and
/// then the hard-poweroff ladder behind it (spec/10), so it cannot land under the budget.
const SHUTDOWN_BUDGET: Duration = Duration::from_secs(45);

/// Guide: docs/guides/stop-restart-preserving-volumes.md. Split from `workflow_07_warmth`
/// because it asserts a *bound*, not a result — folding it in would let a shutdown that
/// merely took four minutes still report the trial as passing.
fn workflow_07_shutdown_bounded() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("volume-project").map_err(io_failed)?;
    arrange_manifest(&tp, "volumes", "", VOLUME_TAIL)?;
    check(expect_code(&viv(&tp, &["start"])?, 0))?;

    let started = Instant::now();
    let stopped = viv(&tp, &["stop"])?;
    let elapsed = started.elapsed();
    check(expect_code(&stopped, 0))?;
    if elapsed > SHUTDOWN_BUDGET {
        return fail(format!(
            concat!(
                "viv stop took {:?}, over the {:?} budget - the guest store share's ",
                "mount is likely inside the ordinary shutdown ordering (spec/06)"
            ),
            elapsed, SHUTDOWN_BUDGET
        ));
    }
    let status = viv(&tp, &["status", "--json"])?;
    check(expect_code(&status, 0))?;
    check(expect_json_string(&status, "state", "built"))
}

// Guide: docs/guides/destroy-cold-rebuild.md
fn workflow_08_rebuild() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("destroy-project").map_err(io_failed)?;
    arrange_manifest(&tp, "teardown-demo", "", "")?;
    check(expect_code(&viv(&tp, &["start"])?, 0))?;
    check(expect_code(
        &viv(
            &tp,
            &["exec", "--", "sh", "-lc", "printf old > \"$HOME/old\""],
        )?,
        0,
    ))?;
    // The built output, captured before teardown: the unroot assertion below is against the
    // store, not the filesystem — a symlink that was never registered is slice 032's defect.
    let status = viv(&tp, &["status", "--json"])?;
    check(expect_code(&status, 0))?;
    let record: serde_json::Value = serde_json::from_slice(&status.stdout)
        .map_err(|error| Failed::from(format!("status --json was not JSON: {error}")))?;
    let built = record["store_path"]
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| Failed::from("status --json reported no store_path before destroy"))?;
    check(expect_code(&viv(&tp, &["destroy", "--yes"])?, 0))?;
    // Slice 032: no generation of the destroyed project stays reachable from any root.
    let state_project = tp
        .state()
        .join("vivarium")
        .join("projects")
        .join("teardown-demo");
    if state_project.exists() {
        return fail(format!(
            "destroy left the generation profile at {}",
            state_project.display()
        ));
    }
    let roots = std::process::Command::new("nix-store")
        .args(["--query", "--roots"])
        .arg(&built)
        .output()
        .map_err(io_failed)?;
    let named = String::from_utf8_lossy(&roots.stdout);
    if named
        .lines()
        .any(|line| line.contains(&state_project.display().to_string()))
    {
        return fail(format!(
            "a destroyed generation is still a root of {built}: {named}"
        ));
    }
    // The derived owner survives teardown; no workspace-local identity artifact exists.
    check(expect_derived_manifest_visible(
        &viv(&tp, &["config", "--json"])?,
        "teardown-demo",
    ))?;
    check(expect_code(&viv(&tp, &["destroy", "--yes"])?, 0))?;

    // Variant A — cold: the next start is a clean first run.
    check(expect_code(&viv(&tp, &["start"])?, 0))?;
    check(expect_code(
        &viv(&tp, &["exec", "--", "sh", "-lc", "test ! -e \"$HOME/old\""])?,
        0,
    ))?;

    // Variant B — warm: `--keep-volumes` leaves the images in place.
    check(expect_code(
        &viv(
            &tp,
            &["exec", "--", "sh", "-lc", "printf warm > \"$HOME/warm\""],
        )?,
        0,
    ))?;
    let default_image = volume_image(&tp, "teardown-demo", "default");
    check(expect_code(
        &viv(&tp, &["destroy", "--keep-volumes", "--yes"])?,
        0,
    ))?;
    if !default_image.is_file() {
        return fail(format!(
            "--keep-volumes removed {}",
            default_image.display()
        ));
    }
    check(expect_code(&viv(&tp, &["start"])?, 0))?;
    check(expect_code(
        &viv(&tp, &["exec", "--", "sh", "-lc", "test -f \"$HOME/warm\""])?,
        0,
    ))
}

/// Slice 015 item 4: a selected build whose launch contract differs from the running binary is
/// refused before boot, with both numbers and the remedy named.
///
/// The refusal reads the built output — never a compile-time constant — which is why the trial
/// can seed a plain directory as the "build": `--no-rebuild` selects whatever the generation
/// profile's `current` pins, the check reads only `share/vivarium/launch-contract-schema`, and
/// the refusal must land before anything in the tree is executed. Behind the virtualization
/// gate because `start` preflights the host before selecting a build, not because anything
/// boots — no case here reaches a launcher.
fn workflow_15_contract_skew() -> Result<(), Failed> {
    let ours = vivarium::launch::LAUNCH_SCHEMA_VERSION;
    let theirs = ours - 1;
    let tp = TempProject::with_project_name("skew-project").map_err(io_failed)?;
    arrange_manifest(&tp, "skew-demo", "", "")?;

    // A stand-in for an old generation: a tree that publishes an older contract number at the
    // stable path, retained as the profile's `current` — `--no-rebuild` boots whatever that
    // pins, and the reader is a symlink chain plus an existence check, so no store is needed.
    let build = tp.root().join("stale-build");
    let schema = build
        .join("share")
        .join("vivarium")
        .join("launch-contract-schema");
    std::fs::create_dir_all(schema.parent().unwrap_or(&build)).map_err(io_failed)?;
    write_file(&schema, &format!("{theirs}\n")).map_err(io_failed)?;
    fabricate_generation(&tp, "skew-demo", 1, &build)?;

    let refused = viv(&tp, &["start", "--no-rebuild"])?;
    check(expect_code(&refused, 78))?;
    check(expect_stderr_mentions(&refused, "launch-contract-skew"))?;
    // Both numbers named, and the remedy — the acceptance's own wording.
    check(expect_stderr_mentions(
        &refused,
        &format!("launch contract schema {theirs}"),
    ))?;
    check(expect_stderr_mentions(&refused, &format!("speaks {ours}")))?;
    check(expect_stderr_mentions(&refused, "--rebuild"))?;

    // Every build made before the schema's publication meets the missing-file case, and it is
    // the same refusal with the migration named rather than a bare parse error.
    std::fs::remove_file(&schema).map_err(io_failed)?;
    let predates = viv(&tp, &["start", "--no-rebuild"])?;
    check(expect_code(&predates, 78))?;
    check(expect_stderr_mentions(
        &predates,
        "predates the launch-contract publication",
    ))
}

/// Slice 015 item 7, the live half: build, doctor a copy of the built tree so its contract
/// schema changes, and show the next start refuses before boot and `--rebuild` clears it.
///
/// The doctored copy stands in for a real old generation, which no test can mint without a
/// second vivarium version on hand: the store is immutable, so "move the tree so the contract
/// schema changes" is a copy whose published number is bumped, selected the way any old build
/// is selected — through the profile's `current` and `--no-rebuild`.
fn workflow_15_contract_skew_live() -> Result<(), Failed> {
    let ours = vivarium::launch::LAUNCH_SCHEMA_VERSION;
    let foreign = ours + 1;
    let tp = TempProject::with_project_name("skew-live").map_err(io_failed)?;
    arrange_manifest(&tp, "skew-live", "", "")?;

    // A real build and boot of the current shape, then a clean stop so the refusal below is
    // about the selected build rather than about a running VM.
    check(expect_code(&viv(&tp, &["start"])?, 0))?;
    check(expect_code(&viv(&tp, &["stop"])?, 0))?;

    let profile = tp
        .state()
        .join("vivarium")
        .join("projects")
        .join("skew-live")
        .join("default");
    let current = std::fs::read_link(profile.join("current")).map_err(io_failed)?;
    let built = std::fs::read_link(profile.join(&current)).map_err(io_failed)?;
    let built = built.to_string_lossy().into_owned();
    let built = built.as_str();

    // The copy: the runner output is a small symlink forest, so copying it moves kilobytes;
    // the schema symlink is replaced by a plain file carrying a number this binary does not
    // speak. Everything else still points into the store the real build populated.
    let doctored = tp.root().join("doctored-build");
    // `--no-preserve=mode`, because a store tree's read-only modes would survive the copy and
    // refuse the doctoring below.
    let copy = std::process::Command::new("cp")
        .args([
            "-r",
            "--no-preserve=mode",
            built,
            doctored.to_str().unwrap_or_default(),
        ])
        .output()
        .map_err(io_failed)?;
    if !copy.status.success() {
        return fail(format!(
            "could not copy the built tree: {}",
            String::from_utf8_lossy(&copy.stderr)
        ));
    }
    let schema = doctored
        .join("share")
        .join("vivarium")
        .join("launch-contract-schema");
    std::fs::remove_file(&schema).map_err(io_failed)?;
    write_file(&schema, &format!("{foreign}\n")).map_err(io_failed)?;
    fabricate_generation(&tp, "skew-live", 2, &doctored)?;

    let refused = viv(&tp, &["start", "--no-rebuild"])?;
    check(expect_code(&refused, 78))?;
    check(expect_stderr_mentions(&refused, "launch-contract-skew"))?;
    check(expect_stderr_mentions(
        &refused,
        &format!("launch contract schema {foreign}"),
    ))?;
    check(expect_stderr_mentions(&refused, &format!("speaks {ours}")))?;
    check(expect_stderr_mentions(&refused, "--rebuild"))?;

    // The named remedy clears it: `--rebuild` re-evaluates, records the current build, and
    // boots it.
    check(expect_code(&viv(&tp, &["start", "--rebuild"])?, 0))?;
    let status = viv(&tp, &["status", "--json"])?;
    check(expect_code(&status, 0))?;
    check(expect_json_string(&status, "state", "running"))?;
    check(expect_code(&viv(&tp, &["stop"])?, 0))
}

/// The workspace is the product's central promise, and until this trial nothing asserted it.
///
/// Three claims, and the third is the one that has no other home. The project is reachable inside
/// the guest at the absolute path it occupies on the host (N16, ADR-0100); an edit crosses the
/// boundary in both directions; and a file the guest creates belongs, on the host, to the user who
/// ran `viv` (ADR-0066). That last one cannot be checked from inside the guest at all — the share's
/// identity translation is precisely what makes the guest see its own uid there — so it is read
/// from the host side of the same file.
fn workflow_09_round_trip() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("workspace-project").map_err(io_failed)?;
    arrange_manifest(&tp, "workspace-demo", "", "")?;

    // Written before the VM exists, so the guest cannot have observed the host's write through
    // some later synchronisation: the file is part of the tree at the moment the share is served.
    let from_host = tp.project().join("from-host.txt");
    write_file(&from_host, "host wrote this\n").map_err(io_failed)?;

    // No `viv start`: ensure-running cold-starts, and `pwd` in the session is the assertion. The
    // guest is told nothing about the host's layout except through the mirror, so a `pwd` that
    // equals the host path is the whole of N16 observed rather than argued.
    let host_project = tp.project().to_string_lossy().into_owned();
    let cwd = viv(&tp, &["exec", "--", "sh", "-lc", "pwd"])?;
    check(expect_code(&cwd, 0))?;
    check(expect_stdout_mentions(&cwd, &host_project))?;
    if String::from_utf8_lossy(&cwd.stdout).trim() != host_project {
        return fail(format!(
            "the guest session starts at `{}`, not at the project's own host path `{host_project}`",
            String::from_utf8_lossy(&cwd.stdout).trim()
        ));
    }

    // Host to guest. Read by absolute path rather than relative to the cwd, so this stays an
    // assertion about the mount and not a second assertion about the working directory.
    let read_back = viv(
        &tp,
        &[
            "exec",
            "--",
            "sh",
            "-lc",
            // The path is an argument rather than shell source, and every use below does the same.
            // The fixture root follows `VIVARIUM_HEAVY_DRIVE`, which is routinely a removable drive
            // mounted at `/run/media/<user>/<label>` — and a volume label is a place spaces live.
            // Interpolated bare, such a path would word-split and fail this trial for a reason that
            // has nothing to do with the mirror, while the encoder it exercises supports spaces on
            // purpose.
            "cat \"$1\"/from-host.txt",
            "sh",
            &host_project,
        ],
    )?;
    check(expect_code(&read_back, 0))?;
    check(expect_stdout_mentions(&read_back, "host wrote this"))?;

    // Guest to host.
    check(expect_code(
        &viv(
            &tp,
            &[
                "exec",
                "--",
                "sh",
                "-lc",
                "printf 'guest wrote this\\n' > \"$1\"/from-guest.txt",
                "sh",
                &host_project,
            ],
        )?,
        0,
    ))?;
    let from_guest = tp.project().join("from-guest.txt");
    let landed = fs::read_to_string(&from_guest).map_err(io_failed)?;
    if landed.trim() != "guest wrote this" {
        return fail(format!(
            "the host reads `{}` at {}, not what the guest wrote",
            landed.trim(),
            from_guest.display()
        ));
    }

    // ADR-0066, from the only side that can see it. Without the share's bidirectional translation
    // this file would land owned by the guest's own fixed uid, which on the host is either some
    // unrelated account or nobody at all — the failure being that a user cannot edit, commit, or
    // delete what their own sandbox produced.
    let owner = fs::metadata(&from_guest).map_err(io_failed)?.uid();
    if owner != vivarium::config::effective_uid() {
        return fail(format!(
            "the guest's file is owned by uid {owner} on the host, not by the invoking user {}",
            vivarium::config::effective_uid()
        ));
    }

    // And back the other way once more, over a file the guest already owns: a mount that served
    // the initial tree and then diverged would satisfy everything above.
    write_file(&from_guest, "host overwrote this\n").map_err(io_failed)?;
    let overwritten = viv(
        &tp,
        &[
            "exec",
            "--",
            "sh",
            "-lc",
            "cat \"$1\"/from-guest.txt",
            "sh",
            &host_project,
        ],
    )?;
    check(expect_code(&overwritten, 0))?;
    check(expect_stdout_mentions(&overwritten, "host overwrote this"))
}

/// Slice 019's launch tier: a declared source that is unset, missing, not a regular file or
/// directory, or a session directory only expansion reveals refuses `viv start` with `78`
/// before any boot, each under its own diagnostic id, and leaves no VM behind.
#[allow(clippy::too_many_lines)]
fn workflow_17_refusals() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("mounts-refusals").map_err(io_failed)?;
    bind_after_arrange(
        &tp,
        "\n[[mounts]]\nsource = \"${VIVARIUM_WF17_UNSET}/tools\"\ntarget = \"/workspaces/tools\"\n",
    )?;
    // The variable is guaranteed unset: the harness clears the child's environment.
    let unset = viv(&tp, &["start"])?;
    check(expect_code(&unset, EX_CONFIG))?;
    check(expect_stderr_mentions(
        &unset,
        "mount-source-unset-variable",
    ))?;
    check(expect_stderr_mentions(&unset, "VIVARIUM_WF17_UNSET"))?;
    check(expect_resting(&tp))?;

    bind_after_arrange(
        &tp,
        concat!(
            "\n[[mounts]]\nsource = \"${HOME}/wf17-definitely-missing\"\n",
            "target = \"/workspaces/tools\"\n"
        ),
    )?;
    let missing = viv(&tp, &["start"])?;
    check(expect_code(&missing, EX_CONFIG))?;
    check(expect_stderr_mentions(&missing, "mount-source-missing"))?;
    check(expect_resting(&tp))?;

    // ADR-0071: filesystem data only. A FIFO is the cheapest special file a fixture can make.
    let fifo = tp.home().join("wf17-fifo");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .map_err(io_failed)?;
    if !made.success() {
        return fail("mkfifo could not create the fixture FIFO".to_owned());
    }
    bind_after_arrange(
        &tp,
        "\n[[mounts]]\nsource = \"${HOME}/wf17-fifo\"\ntarget = \"/workspaces/tools\"\n",
    )?;
    let special = viv(&tp, &["start"])?;
    check(expect_code(&special, EX_CONFIG))?;
    check(expect_stderr_mentions(
        &special,
        "mount-source-not-mountable",
    ))?;
    check(expect_resting(&tp))?;

    // N24's launch half: `${VIVARIUM_WF17_SESSION}/agent` is not decidable from text — the
    // evaluation tier passed it — and expansion lands it under `/tmp`, where the launch
    // refuses with `78` (spec/06's two-tier shape, Q-012's exit).
    bind_after_arrange(
        &tp,
        concat!(
            "\n[[mounts]]\nsource = \"${VIVARIUM_WF17_SESSION}/agent\"\n",
            "target = \"/workspaces/tools\"\n"
        ),
    )?;
    check(expect_code(
        &viv_with_env(
            &tp,
            &["config", "eval", "--json"],
            &[("VIVARIUM_WF17_SESSION", "/tmp/wf17-session")],
        )?,
        0,
    ))?;
    let hidden = viv_with_env(
        &tp,
        &["start"],
        &[("VIVARIUM_WF17_SESSION", "/tmp/wf17-session")],
    )?;
    check(expect_code(&hidden, EX_CONFIG))?;
    check(expect_stderr_mentions(
        &hidden,
        "mount-source-session-directory",
    ))?;
    check(expect_resting(&tp))?;

    // ADR-0100 generalized by ADR-0108: equality, containment, and descent against the
    // guest-owned set are three distinct shapes. Each declaration includes the invocation so the
    // refusal measured here is mirroring, not ADR-0109's earlier ownership precondition.
    for (source, cwd) in [
        ("/tmp", Path::new("/tmp")),
        ("/", tp.project()),
        ("/run/user/1000", Path::new("/run/user/1000")),
    ] {
        arrange_manifest(
            &tp,
            "mounts-refusals",
            &format!("\n[[workspaces]]\nsource = '{source}'\n"),
            "",
        )?;
        let refused = viv_at(&tp, cwd, &["start"])?;
        check(expect_code(&refused, EX_CONFIG))?;
        check(expect_stderr_mentions(
            &refused,
            "workspace-path-unmirrorable",
        ))?;
        check(expect_stderr_mentions(&refused, source))?;
        // Every resolving verb answers the same way about the same defect. Since ADR-0110 the
        // mirroring rules are decided at resolution rather than inside `start`, so `status` meets
        // them too — the deliberate consistency cost ADR-0109 already established for a broken
        // selected manifest, now covering a declaration that cannot be mounted. Asserted rather
        // than tolerated: a verb that answered `built` here would be reporting on a sandbox this
        // manifest can never have.
        let resting = viv_at(&tp, cwd, &["status", "--json"])?;
        check(expect_code(&resting, EX_CONFIG))?;
        check(expect_stderr_mentions(
            &resting,
            "workspace-path-unmirrorable",
        ))?;
    }

    // Different variable spellings conceal the nesting from the decidable textual tier. Launch
    // expansion must still refuse the complete pair before boot and name both resolved paths.
    let parent = tp.home().join("wf20-overlap");
    let child = parent.join("child");
    fs::create_dir_all(&child).map_err(io_failed)?;
    arrange_manifest(
        &tp,
        "mounts-refusals",
        concat!(
            "\n[[workspaces]]\nsource = '${VIVARIUM_WF20_PARENT}'\n",
            "\n[[workspaces]]\nsource = '${VIVARIUM_WF20_CHILD}'\n"
        ),
        "",
    )?;
    let parent_value = parent.to_string_lossy().into_owned();
    let child_value = child.to_string_lossy().into_owned();
    let overlap = support::run_viv_with_env(
        preflight::viv(),
        &tp,
        &child,
        &["start"],
        &[
            ("VIVARIUM_WF20_PARENT", &parent_value),
            ("VIVARIUM_WF20_CHILD", &child_value),
        ],
    )
    .map_err(io_failed)?;
    check(expect_code(&overlap, EX_CONFIG))?;
    check(expect_stderr_mentions(&overlap, "workspace-paths-overlap"))?;
    check(expect_stderr_mentions(&overlap, &parent_value))?;
    check(expect_stderr_mentions(&overlap, &child_value))?;
    let resting = support::run_viv_with_env(
        preflight::viv(),
        &tp,
        &child,
        &["status", "--json"],
        &[
            ("VIVARIUM_WF20_PARENT", &parent_value),
            ("VIVARIUM_WF20_CHILD", &child_value),
        ],
    )
    .map_err(io_failed)?;
    check(expect_code(&resting, EX_CONFIG))?;
    check(expect_stderr_mentions(&resting, "workspace-paths-overlap"))
}

/// Slice 019's acceptance heart: a piece-declared directory mount with a portable source is
/// readable and writable in the guest at its declared target, a manifest-declared regular-file
/// mount lands at its target through its parent directory, and `readonly = true` refuses a
/// guest write with the host tree unchanged — all in one boot, so the two extra confined
/// daemons demonstrably serve side by side.
#[allow(clippy::too_many_lines)]
fn workflow_17_round_trip() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("mounts-project").map_err(io_failed)?;
    // The portable-variable piece: the same declaration would work in anyone's manifest, and
    // the trial proves it against this host's `${HOME}` (spec/07).
    write_piece(
        &tp,
        "wf17cache",
        r#"{ ... }: {
    vivarium.mounts = [
        {
            source = "\${HOME}/.cache/wf17-tool";
            target = "~/.cache/wf17-tool";
            readonly = false;
        }
    ];
}
"#,
    )?;
    // The file mount's basename holds a space on purpose: the entry crosses the kernel command
    // line percent-encoded, and a name that needs the encoding is the case worth exercising.
    arrange_manifest(
        &tp,
        "mounts-demo",
        concat!(
            "pieces = [ \"wf17cache\" ]\n\n[[mounts]]\n",
            "source = \"${HOME}/wf17-ro/wf17 config.toml\"\n",
            "target = \"/workspaces/wf17-config.toml\"\nreadonly = true\n"
        ),
        "",
    )?;

    // Written before the VM exists, like the workspace round trip: the files are part of the
    // trees at the moment the shares are served.
    let cache_dir = tp.home().join(".cache").join("wf17-tool");
    write_file(&cache_dir.join("marker.txt"), "host cache marker\n").map_err(io_failed)?;
    let ro_dir = tp.home().join("wf17-ro");
    write_file(&ro_dir.join("wf17 config.toml"), "key = \"wf17-value\"\n").map_err(io_failed)?;

    // Host to guest, at the declared target. `~` in the declaration is the GUEST home, so the
    // session's own `$HOME` is exactly where it must appear.
    let read_cache = viv(
        &tp,
        &[
            "exec",
            "--",
            "sh",
            "-lc",
            "cat \"$HOME\"/.cache/wf17-tool/marker.txt",
        ],
    )?;
    check(expect_code(&read_cache, 0))?;
    check(expect_stdout_mentions(&read_cache, "host cache marker"))?;

    // Guest to host through the read-write mount, and the identity translation with it: the
    // file must land owned by the invoking user, exactly as the workspace's does.
    check(expect_code(
        &viv(
            &tp,
            &[
                "exec",
                "--",
                "sh",
                "-lc",
                "printf 'guest cache write\\n' > \"$HOME\"/.cache/wf17-tool/from-guest.txt",
            ],
        )?,
        0,
    ))?;
    let landed = cache_dir.join("from-guest.txt");
    let content = fs::read_to_string(&landed).map_err(io_failed)?;
    if content.trim() != "guest cache write" {
        return fail(format!(
            "the host reads `{}` at {}, not what the guest wrote",
            content.trim(),
            landed.display()
        ));
    }
    let owner = fs::metadata(&landed).map_err(io_failed)?.uid();
    if owner != vivarium::config::effective_uid() {
        return fail(format!(
            "the guest's file is owned by uid {owner} on the host, not by the invoking user {}",
            vivarium::config::effective_uid()
        ));
    }

    // The regular-file mount: served through its parent, bound at the declared absolute target.
    let read_ro = viv(
        &tp,
        &[
            "exec",
            "--",
            "sh",
            "-lc",
            "cat /workspaces/wf17-config.toml",
        ],
    )?;
    check(expect_code(&read_ro, 0))?;
    check(expect_stdout_mentions(&read_ro, "wf17-value"))?;

    // `readonly = true`: the guest write fails and the host tree is byte-identical after.
    let before = snapshot_tree(&ro_dir).map_err(io_failed)?;
    check(expect_nonzero(&viv(
        &tp,
        &[
            "exec",
            "--",
            "sh",
            "-lc",
            "printf 'guest defaced this' > /workspaces/wf17-config.toml",
        ],
    )?))?;
    check(expect_tree_unchanged(&ro_dir, &before))?;

    // One confined daemon per share (N20): while the guest is up, each declared mount's own
    // socket exists under the runtime root — `mnt0.sock` and `mnt1.sock` beside the two
    // reserved shares' — which is the host-visible edge of "its own daemon". The rendered
    // profile validated over all four or the boot above would have refused.
    for socket in ["mnt0.sock", "mnt1.sock"] {
        if !path_named_exists(tp.runtime(), socket) {
            return fail(format!(
                "no `{socket}` under the runtime root while the guest runs: a declared mount \
                is not being served by its own daemon"
            ));
        }
    }
    check(expect_code(&viv(&tp, &["stop"])?, 0))
}

/// Q-016's exit: a linked worktree's `.git` names `<main>/.git/worktrees/<id>`, outside the one
/// mirrored project tree — and a declared mount whose `source` and `target` are the main
/// repository's own path is what makes it resolvable from inside the guest.
fn workflow_17_worktree() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("wf17-main").map_err(io_failed)?;
    let main_repo = tp.project().to_path_buf();
    let git = |args: &[&str], cwd: &Path| -> Result<(), Failed> {
        let mut command = std::process::Command::new("git");
        command
            .args(args)
            .current_dir(cwd)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null");
        // Git exports its own repository into every hook it runs, so a suite
        // invoked from the pre-push gate inherits `GIT_DIR` and its siblings.
        // Without clearing them the fixture's `git` reads this repository
        // instead of the temporary one, runs this repository's installed
        // hooks with the fixture as the working directory, and fails on a
        // `.pre-commit-config.yaml` that is not there. Isolating the config
        // alone is not enough: the location variables outrank `current_dir`.
        for name in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_COMMON_DIR",
            "GIT_INDEX_FILE",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_PREFIX",
            "GIT_NAMESPACE",
            "GIT_CEILING_DIRECTORIES",
        ] {
            command.env_remove(name);
        }
        let status = command.status().map_err(io_failed)?;
        if !status.success() {
            return fail(format!("git {args:?} failed in {}", cwd.display()));
        }
        Ok(())
    };
    git(&["init", "-q"], &main_repo)?;
    git(
        &[
            "-c",
            "user.name=wf17",
            "-c",
            "user.email=wf17@example.invalid",
            "commit",
            "--allow-empty",
            "-q",
            "-m",
            "wf17",
        ],
        &main_repo,
    )?;
    let worktree = tp.root().join("wf17-tree");
    git(
        &["worktree", "add", "-q", worktree.to_str().unwrap_or("")],
        &main_repo,
    )?;

    // Both trees at their own host paths, which is the whole point — the worktree's `.git` file
    // names the main repository absolutely, from either side.
    //
    // Two rows, not three. Before ADR-0110 this manifest also carried a `[[mounts]]` row
    // mirroring the main repository, because that was the surface slice 019 built for reaching a
    // tree outside the workspace set. It is now the same mechanism: a workspace compiles to a
    // mount whose target is its source, so declaring both put two rows on one target and is
    // refused at `65`. That refusal is correct and the redundancy was always there — it was
    // invisible while two units bound the same path and the later one silently won.
    let main_spelling = main_repo.to_string_lossy().into_owned();
    let worktree_spelling = worktree.to_string_lossy().into_owned();
    arrange_manifest(
        &tp,
        "wf17-worktree",
        &format!(
            "\n[[workspaces]]\nsource = '{main_spelling}'\n\
            \n[[workspaces]]\nsource = '{worktree_spelling}'\n"
        ),
        "",
    )?;
    // From inside the guest: resolve the worktree's own `.git` pointer and read HEAD through
    // it. No git in the guest image, and none needed — reachability of the named git directory
    // is exactly what Q-016 asks for. The worktree path is an argument rather than shell
    // source, for the space-safety reason workflow_09 records.
    let reach = viv_at(
        &tp,
        &worktree,
        &[
            "exec",
            "--",
            "sh",
            "-lc",
            concat!(
                "gitdir=$(sed -n 's/^gitdir: //p' \"$1\"/.git) && ",
                "test -d \"$gitdir\" && cat \"$gitdir\"/HEAD"
            ),
            "sh",
            worktree.to_str().unwrap_or(""),
        ],
    )?;
    check(expect_code(&reach, 0))?;
    check(expect_stdout_mentions(&reach, "ref:"))?;
    check(expect_code(&viv_at(&tp, &worktree, &["stop"])?, 0))
}

/// Slice 020's acceptance heart: two equal workspace declarations share one VM, round-trip
/// independently, and map each invocation's exact directory into the guest session.
///
/// Neither declaration is privileged. The second `start` exercises the reuse predicate phase 2
/// repaired against the complete recorded workspace set.
#[allow(clippy::too_many_lines)] // One sandbox story: the fleet-row assertions share its VM.
fn workflow_20_many_workspaces() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("wf20-anchor").map_err(io_failed)?;
    let first = tp.root().join("owned-first");
    let second = tp.root().join("owned-second");
    let second_subdir = second.join("nested");
    fs::create_dir_all(&first).map_err(io_failed)?;
    fs::create_dir_all(&second_subdir).map_err(io_failed)?;
    arrange_manifest(
        &tp,
        "anchor-outside",
        &format!(
            "\n[[workspaces]]\nsource = '{}'\n\n[[workspaces]]\nsource = '{}'\n",
            first.to_string_lossy(),
            second.to_string_lossy()
        ),
        "",
    )?;
    write_file(&first.join("from-host.txt"), "first host\n").map_err(io_failed)?;
    write_file(&second.join("from-host.txt"), "second host\n").map_err(io_failed)?;

    check(expect_code(&viv_at(&tp, &first, &["start"])?, 0))?;
    // The second call reaches `reusable()` from the other tree and must be N15's no-op against
    // the same boot, without another preflight, evaluation, build, or VM.
    check(expect_code(&viv_at(&tp, &second, &["start"])?, 0))?;
    let status = viv_at(&tp, &second, &["status", "--json"])?;
    check(expect_code(&status, 0))?;
    check(expect_json_string(&status, "state", "running"))?;

    for (tree, marker) in [(&first, "first"), (&second, "second")] {
        let tree_arg = tree.to_string_lossy().into_owned();
        let pwd = viv_at(&tp, tree, &["exec", "--", "sh", "-lc", "pwd"])?;
        check(expect_code(&pwd, 0))?;
        if String::from_utf8_lossy(&pwd.stdout).trim() != tree_arg {
            return fail(format!(
                "a session invoked from {} started at `{}`",
                tree.display(),
                String::from_utf8_lossy(&pwd.stdout).trim()
            ));
        }
        let read = viv_at(
            &tp,
            tree,
            &[
                "exec",
                "--",
                "sh",
                "-lc",
                "cat \"$1\"/from-host.txt",
                "sh",
                &tree_arg,
            ],
        )?;
        check(expect_code(&read, 0))?;
        check(expect_stdout_mentions(&read, &format!("{marker} host")))?;
        check(expect_code(
            &viv_at(
                &tp,
                tree,
                &[
                    "exec",
                    "--",
                    "sh",
                    "-lc",
                    "printf '%s guest\\n' \"$2\" > \"$1\"/from-guest.txt",
                    "sh",
                    &tree_arg,
                    marker,
                ],
            )?,
            0,
        ))?;
        let landed = fs::read_to_string(tree.join("from-guest.txt")).map_err(io_failed)?;
        if landed.trim() != format!("{marker} guest") {
            return fail(format!(
                "the guest write in {} landed as `{}`",
                tree.display(),
                landed.trim()
            ));
        }
        shell_pwd_at(&tp, tree, tree)?;
    }

    let nested_pwd = viv_at(&tp, &second_subdir, &["exec", "--", "sh", "-lc", "pwd"])?;
    check(expect_code(&nested_pwd, 0))?;
    if String::from_utf8_lossy(&nested_pwd.stdout).trim() != second_subdir.to_string_lossy() {
        return fail(format!(
            "a subdirectory invocation started at `{}` instead of `{}`",
            String::from_utf8_lossy(&nested_pwd.stdout).trim(),
            second_subdir.display()
        ));
    }
    // Slice 025: the fleet view names this multi-workspace sandbox as one row carrying the
    // whole declared set, never as one row per workspace (ADR-0108).
    let fleet = viv_at(&tp, &second, &["status", "-g", "--json"])?;
    check(expect_code(&fleet, 0))?;
    let projects = project_rows(&fleet)?;
    let rows: Vec<&serde_json::Value> = projects
        .iter()
        .filter(|row| row["manifest"] == "anchor-outside")
        .collect();
    if rows.len() != 1 {
        return fail(format!(
            "the sandbox appeared {} times in the fleet",
            rows.len()
        ));
    }
    let listed: Vec<&str> = rows[0]["workspaces"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect()
        })
        .unwrap_or_default();
    for tree in [&first, &second] {
        let owned = tree.canonicalize().map_err(io_failed)?;
        if !listed.contains(&owned.to_string_lossy().as_ref()) {
            return fail(format!(
                "the row's workspace set {listed:?} lost {}",
                owned.display()
            ));
        }
    }

    check(expect_code(&viv_at(&tp, &second, &["stop"])?, 0))
}

/// Open a real interactive shell at `cwd` and prove its initial guest directory exactly.
fn shell_pwd_at(tp: &TempProject, cwd: &Path, expected: &Path) -> Result<(), Failed> {
    use std::io::Write as _;

    let (mut pty, pts) = pty_process::blocking::open().map_err(|error| pty_failed(&error))?;
    rustix::io::ioctl_fionbio(&pty, true).map_err(errno_failed)?;
    pty.resize(pty_process::Size::new(40, 120))
        .map_err(|error| pty_failed(&error))?;
    let mut child = pty_process::blocking::Command::new(preflight::viv())
        .arg("shell")
        .current_dir(cwd)
        .env_clear()
        .envs(support::viv_environment(tp))
        .spawn(pts)
        .map_err(|error| pty_failed(&error))?;
    settle(&mut pty)?;
    let observed = ask(&mut pty, "pwd")?;
    let expected = expected.to_string_lossy();
    if !observed.lines().any(|line| line.trim() == expected) {
        let _ = child.kill();
        return fail(format!(
            "interactive shell from {expected} did not report that cwd: {observed:?}"
        ));
    }
    write!(pty, "exit\r").map_err(io_failed)?;
    pty.flush().map_err(io_failed)?;
    child.wait().map_err(io_failed)?;
    Ok(())
}

/// A declared file mount reaches the guest, and nothing beside it does (ADR-0105, spec/06).
///
/// The demonstration is made at the export rather than by attacking from inside the guest, and
/// deliberately: the product hands out no root in the guest — `viv exec` runs as the session user,
/// the agent unit runs as `vivarium`, the image carries no `sudo` and no getty, and a piece can
/// only set `vivarium.*` options — so a guest-root remount is a modelled adversary and not
/// something a trial can perform. What bounds that adversary is the daemon's own root directory:
/// whatever a guest does with the tag, root included, it reaches what this asserts on. So the
/// trial reads the live daemon's root from the host and requires exactly the declared file, with a
/// sibling planted next to the source on purpose and required absent.
#[allow(clippy::too_many_lines)]
fn workflow_22_file_mount_confinement() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("wf22-project").map_err(io_failed)?;
    // Two file mounts from one populated directory: a read-only one and a read-write one, so the
    // trial covers both halves of the declaration through the same staged export.
    arrange_manifest(
        &tp,
        "wf22-demo",
        concat!(
            "[[mounts]]\n",
            "source = \"${HOME}/wf22-secrets/wf22 identity.toml\"\n",
            "target = \"/workspaces/wf22-identity.toml\"\nreadonly = true\n\n",
            "[[mounts]]\n",
            "source = \"${HOME}/wf22-secrets/wf22-notes.txt\"\n",
            "target = \"~/wf22-notes.txt\"\nreadonly = false\n"
        ),
        "",
    )?;

    let secrets = tp.home().join("wf22-secrets");
    write_file(&secrets.join("wf22 identity.toml"), "id = \"wf22-value\"\n").map_err(io_failed)?;
    write_file(&secrets.join("wf22-notes.txt"), "host notes\n").map_err(io_failed)?;
    // The sibling that must never cross. Its name is what the assertions below look for, and it
    // shares the parent directory with both declared files.
    write_file(&secrets.join("wf22-sibling-secret"), "SIBLING\n").map_err(io_failed)?;

    // Both declared files reach their targets, so the confinement is not the trivial kind that
    // serves nothing at all.
    let read_ro = viv(
        &tp,
        &[
            "exec",
            "--",
            "sh",
            "-lc",
            "cat /workspaces/wf22-identity.toml",
        ],
    )?;
    check(expect_code(&read_ro, 0))?;
    check(expect_stdout_mentions(&read_ro, "wf22-value"))?;
    let notes_read = viv(
        &tp,
        &["exec", "--", "sh", "-lc", "cat \"$HOME\"/wf22-notes.txt"],
    )?;
    check(expect_code(&notes_read, 0))?;
    check(expect_stdout_mentions(&notes_read, "host notes"))?;

    // The export root is read-only while the declared file is not: a read-write file mount stays
    // writable through its own bind, and the write lands on the host inode.
    check(expect_code(
        &viv(
            &tp,
            &[
                "exec",
                "--",
                "sh",
                "-lc",
                "printf 'guest note\\n' >> \"$HOME\"/wf22-notes.txt",
            ],
        )?,
        0,
    ))?;
    let notes = fs::read_to_string(secrets.join("wf22-notes.txt")).map_err(io_failed)?;
    if !notes.contains("guest note") {
        return fail(format!(
            "the guest's append never reached the host: {notes:?}"
        ));
    }

    // The share as the guest sees it: exactly the declared file at the internal mount point, which
    // is now readable rather than shadowed, because there is nothing left to hide.
    for (tag, name) in [("mnt0", "wf22 identity.toml"), ("mnt1", "wf22-notes.txt")] {
        let listing = viv(
            &tp,
            &[
                "exec",
                "--",
                "sh",
                "-lc",
                "ls -A /run/vivarium-mounts/\"$1\"",
                "sh",
                tag,
            ],
        )?;
        check(expect_code(&listing, 0))?;
        let served = String::from_utf8_lossy(&listing.stdout);
        let entries = served.lines().filter(|line| !line.is_empty()).count();
        if entries != 1 || !served.contains(name) {
            return fail(format!(
                "the guest sees {entries} entries in share {tag}, not the one declared file: \
                {served:?}"
            ));
        }
        if served.contains("wf22-sibling-secret") {
            return fail(format!(
                "a sibling of the declared file reached share {tag}"
            ));
        }
    }

    // And the same property where it is actually enforced: the daemon's own root. This is what
    // bounds a guest that has root — it cannot address what its daemon cannot see.
    for tag in ["mnt0", "mnt1"] {
        let socket = path_named(tp.runtime(), &format!("{tag}.sock")).ok_or_else(|| {
            Failed::from(format!("no socket for share {tag} under the runtime root"))
        })?;
        let pid = daemon_pid_serving(&socket)
            .ok_or_else(|| Failed::from(format!("no live daemon serving {}", socket.display())))?;
        let root = PathBuf::from(format!("/proc/{pid}/root"));
        let mut served = fs::read_dir(&root)
            .map_err(io_failed)?
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        served.sort();
        if served.len() != 1 {
            return fail(format!(
                "share {tag}'s daemon serves {served:?}, not one declared file"
            ));
        }
        if served.iter().any(|name| name == "wf22-sibling-secret") {
            return fail(format!(
                "share {tag}'s daemon can reach the planted sibling"
            ));
        }
        // The parent directory itself must be unreachable, not merely unlisted: a daemon whose
        // root were the parent would answer this.
        if root.join("wf22-sibling-secret").exists() {
            return fail(format!(
                "share {tag}'s daemon resolves a sibling path under its own root"
            ));
        }
    }

    // `readonly = true` still refuses the guest, and nothing beside the declared files moves.
    // Snapshotted after the read-write append, so the only change this could catch is one the
    // declaration never asked for — a defaced identity file, or a touched sibling.
    let before = snapshot_tree(&secrets).map_err(io_failed)?;
    check(expect_nonzero(&viv(
        &tp,
        &[
            "exec",
            "--",
            "sh",
            "-lc",
            "printf 'guest defaced this' > /workspaces/wf22-identity.toml",
        ],
    )?))?;
    check(expect_tree_unchanged(&secrets, &before))?;
    check(expect_code(&viv(&tp, &["stop"])?, 0))
}

/// Kills the wrapped child on drop, so a failing leg cannot leak a host agent process.
struct KillOnDrop(std::process::Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A live host `ssh-agent` holding one throwaway key, dead when this is dropped.
struct HostAgent {
    _process: KillOnDrop,
    socket: PathBuf,
    fingerprint: String,
}

/// Arrange the host half the relay forwards: a real agent holding a generated key. The socket
/// sits under the runtime root for the 108-byte `SUN_LEN` budget, the same reason the product's
/// own sockets live there.
fn arrange_host_agent(tp: &TempProject) -> Result<HostAgent, Failed> {
    let key = tp.root().join("wf23-key");
    let generated = std::process::Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-C", "wf23", "-f"])
        .arg(&key)
        .status()
        .map_err(io_failed)?;
    if !generated.success() {
        return fail("ssh-keygen could not create the fixture key".to_owned());
    }
    let socket = tp.runtime().join("wf23-agent.sock");
    let process = KillOnDrop(
        std::process::Command::new("ssh-agent")
            .arg("-D")
            .arg("-a")
            .arg(&socket)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(io_failed)?,
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while !socket.exists() {
        if Instant::now() > deadline {
            return fail("the fixture ssh-agent never bound its socket".to_owned());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let added = std::process::Command::new("ssh-add")
        .arg(&key)
        .env("SSH_AUTH_SOCK", &socket)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(io_failed)?;
    if !added.success() {
        return fail("ssh-add could not hand the fixture key to the agent".to_owned());
    }
    let listed = std::process::Command::new("ssh-keygen")
        .arg("-lf")
        .arg(key.with_extension("pub"))
        .output()
        .map_err(io_failed)?;
    let fingerprint = String::from_utf8_lossy(&listed.stdout)
        .split_whitespace()
        .nth(1)
        .map(str::to_owned)
        .ok_or_else(|| Failed::from("ssh-keygen printed no fingerprint for the fixture key"))?;
    Ok(HostAgent {
        _process: process,
        socket,
        fingerprint,
    })
}

/// Slice 031's acceptance, end to end as the user: a declared channel refuses by name before
/// build and boot when the host agent is absent, relays a real agent when it is present, and
/// leaves no private key material in the guest — asserted by inspection, because an operation
/// succeeding cannot tell a relay from a key that was copied in.
fn workflow_23_relay() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("agent-channel").map_err(io_failed)?;
    write_piece(
        &tp,
        "credentials-ssh",
        "{ ... }: { vivarium.credentials.agents = [ \"ssh\" ]; }\n",
    )?;
    // `ssh-add` inside the guest is the operation only a working relay completes; the shipped
    // guest carries Nix and direnv alone (spec/06), so the image adds the client tools.
    arrange_manifest_with_image(
        &tp,
        "agent-channel",
        "pieces = [ \"credentials-ssh\" ]\n",
        "",
        "{ pkgs, ... }: { environment.systemPackages = [ pkgs.openssh ]; }\n",
    )?;

    // Refused before build and boot: the harness clears the child's environment, so
    // `SSH_AUTH_SOCK` is deterministically unset — spec/07's first fault — and no build record
    // may exist afterwards, because a refusal delivered after minutes of `nix build` would
    // satisfy a weaker assertion than the acceptance makes.
    let refused = viv(&tp, &["start"])?;
    check(expect_code(&refused, EX_CONFIG))?;
    check(expect_stderr_mentions(&refused, "agent-source-unset"))?;
    check(expect_stderr_mentions(&refused, "SSH_AUTH_SOCK"))?;
    if path_named_exists(tp.state(), "generations") {
        return fail(
            "the refusal came after a build; a declared channel refuses before one".to_owned(),
        );
    }

    let agent = arrange_host_agent(&tp)?;

    // Cold start through the ordinary ensure-running path, with the agent now resolvable. The
    // guest listing the host agent's key is an answer only a working relay can produce: the key
    // was never written anywhere the guest can read.
    let socket_value = agent.socket.to_string_lossy().into_owned();
    let with_agent: &[(&str, &str)] = &[("SSH_AUTH_SOCK", &socket_value)];
    let guest_list = viv_with_env(&tp, &["exec", "--", "ssh-add", "-l"], with_agent)?;
    check(expect_code(&guest_list, 0))?;
    check(expect_stdout_mentions(&guest_list, &agent.fingerprint))?;

    // The guest's variable is the tool-generated fixed path, never the host's own (N17).
    let guest_variable = viv_with_env(
        &tp,
        &["exec", "--", "sh", "-lc", "printf %s \"$SSH_AUTH_SOCK\""],
        with_agent,
    )?;
    check(expect_code(&guest_variable, 0))?;
    let reported = String::from_utf8_lossy(&guest_variable.stdout);
    if reported.trim() != "/run/vivarium/ssh-agent.sock" {
        return fail(format!(
            "the guest's SSH_AUTH_SOCK is `{}`, not the fixed relay path",
            reported.trim()
        ));
    }

    // No private key material in the guest, by inspection. The scan must be able to see before
    // its emptiness means anything (the harness method note), so a decoy proves the instrument
    // and is removed before the real pass. `/nix` is outside the scan on purpose: the read-only
    // store is shared, key material there would be an N10 violation no launch-time relay could
    // produce, and scanning it costs minutes for a claim this trial does not make.
    let scan = "find /home /root /run /tmp /var /etc -xdev -type f \
        -exec grep -l \"PRIVATE KEY\" {} + 2>/dev/null; true";
    let control = viv_with_env(
        &tp,
        &[
            "exec",
            "--",
            "sh",
            "-lc",
            &format!("printf %s \"FAKE PRIVATE KEY\" > /tmp/wf23-decoy && {scan}"),
        ],
        with_agent,
    )?;
    check(expect_code(&control, 0))?;
    check(expect_stdout_mentions(&control, "/tmp/wf23-decoy"))?;
    let swept = viv_with_env(
        &tp,
        &[
            "exec",
            "--",
            "sh",
            "-lc",
            &format!("rm /tmp/wf23-decoy && {scan}"),
        ],
        with_agent,
    )?;
    check(expect_code(&swept, 0))?;
    let findings = String::from_utf8_lossy(&swept.stdout);
    if !findings.trim().is_empty() {
        return fail(format!(
            "the guest holds files matching private key material:\n{findings}"
        ));
    }
    // And the conventional landing place is empty, listed rather than inferred.
    let ssh_dir = viv_with_env(
        &tp,
        &[
            "exec",
            "--",
            "sh",
            "-lc",
            "ls -A \"$HOME/.ssh\" 2>/dev/null; true",
        ],
        with_agent,
    )?;
    check(expect_code(&ssh_dir, 0))?;
    let entries = String::from_utf8_lossy(&ssh_dir.stdout);
    if !entries.trim().is_empty() {
        return fail(format!("the guest's ~/.ssh is not empty:\n{entries}"));
    }

    check(expect_code(&viv(&tp, &["stop"])?, 0))
}

/// The pid of the live `virtiofsd` serving this socket, read from the host's own process table.
///
/// By command line rather than by a pid file: what this trial needs to be true is that the process
/// actually serving the guest is confined, and the process serving the guest is the one whose
/// arguments name this socket.
fn daemon_pid_serving(socket: &Path) -> Option<u32> {
    let needle = socket.to_string_lossy().into_owned();
    for entry in fs::read_dir("/proc").ok()?.flatten() {
        // `/proc` holds more than processes, so a name that is not a pid is skipped rather than
        // ending the search — the difference between a scan and a scan that stops at `acpi`.
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        let Ok(cmdline) = fs::read(entry.path().join("cmdline")) else {
            continue;
        };
        let cmdline = String::from_utf8_lossy(&cmdline).replace('\0', " ");
        if cmdline.contains("virtiofsd") && cmdline.contains(&needle) {
            return Some(pid);
        }
    }
    None
}

/// Rewrites the one manifest this trial derives, one defective declaration a leg.
fn bind_after_arrange(tp: &TempProject, mounts: &str) -> Result<(), Failed> {
    arrange_manifest(tp, "mounts-refusals", mounts, "")
}

/// The resting assertion every refusal leg shares: the refusal left a build and no VM.
fn expect_resting(tp: &TempProject) -> Result<(), String> {
    let status = run_viv(preflight::viv(), tp, tp.project(), &["status", "--json"])
        .map_err(|error| error.to_string())?;
    expect_code(&status, 0)?;
    expect_json_string(&status, "state", "built")
}

/// Slice 032, the retention core: a build appends a rooted generation, an unchanged build
/// appends nothing, the profile lists, switches, and prunes, a named generation boots, and the
/// running VM's own generation refuses to unlink at `75` — decided from the boot record rather
/// than `current`, which the trial arranges to disagree.
#[allow(clippy::too_many_lines)] // one retention story told in order, not logic to split
fn workflow_24_retention() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("generations-project").map_err(io_failed)?;
    arrange_manifest(&tp, "generations-demo", "", "")?;

    // First build: one generation, current, all seven published keys, rooted in the store.
    check(expect_code(&viv(&tp, &["start"])?, 0))?;
    check(expect_code(&viv(&tp, &["stop"])?, 0))?;
    let list = viv(&tp, &["generations", "list", "--json"])?;
    check(expect_code(&list, 0))?;
    check(expect_json_array_items(
        &list,
        "generations",
        &[
            "number",
            "current",
            "store_path",
            "manifest",
            "lock_digest",
            "backend",
            "built_at",
        ],
    ))?;
    let rows = generations_rows(&tp)?;
    let [first] = rows.as_slice() else {
        return fail(format!(
            "expected one generation after one build, got {rows:?}"
        ));
    };
    if first["current"] != serde_json::json!(true) {
        return fail(format!("the only generation is not current: {first}"));
    }
    let digest = first["lock_digest"].as_str().unwrap_or_default();
    if !digest.starts_with("sha256:") {
        return fail(format!(
            "lock_digest is not a sha256 content digest: {digest:?}"
        ));
    }
    let number_1 = first["number"].as_u64().unwrap_or_default();
    let path_1 = first["store_path"]
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| Failed::from("the first generation names no store path"))?;
    check(expect_generation_rooted(
        &tp,
        "generations-demo",
        number_1,
        &path_1,
        true,
    ))?;

    // An unchanged start appends nothing: the freshness key is the store path (N4).
    check(expect_code(&viv(&tp, &["start"])?, 0))?;
    check(expect_code(&viv(&tp, &["stop"])?, 0))?;
    if generations_rows(&tp)?.len() != 1 {
        return fail("an unchanged build appended a duplicate generation".to_owned());
    }

    // A changed image appends the next generation and moves `current` — the change must reach
    // the built output, which an `[env]` launch-channel value would not.
    arrange_manifest_with_image(
        &tp,
        "generations-demo",
        "",
        "",
        "{ ... }: { environment.etc.\"generation-mark\".text = \"two\"; }\n",
    )?;
    check(expect_code(&viv(&tp, &["start"])?, 0))?;
    check(expect_code(&viv(&tp, &["stop"])?, 0))?;
    let rows = generations_rows(&tp)?;
    let [old, new] = rows.as_slice() else {
        return fail(format!("expected two generations, got {rows:?}"));
    };
    let number_2 = new["number"].as_u64().unwrap_or_default();
    if number_2 <= number_1
        || old["current"] != serde_json::json!(false)
        || new["current"] != serde_json::json!(true)
    {
        return fail(format!(
            "the second build did not append monotonically: {rows:?}"
        ));
    }

    // `rollback` steps `current` back; `activate` moves it by name.
    check(expect_code(&viv(&tp, &["generations", "rollback"])?, 0))?;
    if generations_rows(&tp)?[0]["current"] != serde_json::json!(true) {
        return fail("rollback did not move `current` to the previous generation".to_owned());
    }
    let activate = number_2.to_string();
    check(expect_code(
        &viv(&tp, &["generations", "activate", &activate])?,
        0,
    ))?;
    if generations_rows(&tp)?[1]["current"] != serde_json::json!(true) {
        return fail("activate did not move `current` back".to_owned());
    }

    // A named generation boots without evaluating. `current` stays where activate put it, so
    // the prune guard below can only pass by reading the boot record.
    let boot_old = number_1.to_string();
    check(expect_code(
        &viv(&tp, &["start", "--generation", &boot_old])?,
        0,
    ))?;
    let refused = viv(&tp, &["generations", "prune", "--keep", "1"])?;
    check(expect_code(&refused, EX_TEMPFAIL))?;
    check(expect_stderr_mentions(&refused, "generation-in-use"))?;
    check(expect_code(&viv(&tp, &["stop"])?, 0))?;

    // Stopped, the same prune unlinks exactly what its retention argument selects.
    check(expect_code(
        &viv(&tp, &["generations", "prune", "--keep", "1"])?,
        0,
    ))?;
    let rows = generations_rows(&tp)?;
    if rows.len() != 1 || rows[0]["number"].as_u64() != Some(number_2) {
        return fail(format!(
            "prune --keep 1 did not keep exactly the newest: {rows:?}"
        ));
    }
    let metadata = tp
        .state()
        .join("vivarium")
        .join("projects")
        .join("generations-demo")
        .join("default")
        .join("metadata");
    if metadata.join(format!("{number_1}.json")).exists()
        || metadata.join(format!("{number_1}.lock")).exists()
    {
        return fail("a pruned generation left its metadata or lock snapshot behind".to_owned());
    }
    check(expect_generation_rooted(
        &tp,
        "generations-demo",
        number_1,
        &path_1,
        false,
    ))?;
    // A pruned generation is a legible error to boot, never a silent rebuild (spec/11).
    check(expect_code(
        &viv(&tp, &["start", "--generation", &boot_old])?,
        EX_DATAERR,
    ))?;

    // Numbers are never reused: the build after a prune steps past the gap.
    arrange_manifest_with_image(
        &tp,
        "generations-demo",
        "",
        "",
        "{ ... }: { environment.etc.\"generation-mark\".text = \"three\"; }\n",
    )?;
    check(expect_code(&viv(&tp, &["start"])?, 0))?;
    check(expect_code(&viv(&tp, &["stop"])?, 0))?;
    let numbers: Vec<u64> = generations_rows(&tp)?
        .iter()
        .filter_map(|row| row["number"].as_u64())
        .collect();
    if numbers != vec![number_2, number_2 + 1] {
        return fail(format!("numbering reused or skipped wrongly: {numbers:?}"));
    }
    Ok(())
}

/// Slice 025, sessions counted not tracked: a fresh boot reports zero, an attached session
/// raises the count, and the detach returns it to zero (spec/12, spec/17).
fn workflow_25_sessions_count() -> Result<(), Failed> {
    let tp = TempProject::new().map_err(io_failed)?;
    arrange_manifest(
        &tp,
        "wf25-sess",
        "\n[resources]\nmem_mib = 2048\nvcpu = 2\n",
        "",
    )?;
    check(expect_code(&viv(&tp, &["start"])?, 0))?;
    poll_sessions(&tp, 0)?;

    // One held session, spawned concurrently and then killed: the kill closes the client's
    // connection, which is exactly what a detach is (spec/12's per-connection sessions).
    let mut held = spawn_viv_session(&tp)?;
    let raised = poll_sessions(&tp, 1);
    let _ = held.kill();
    let _ = held.wait();
    raised?;
    poll_sessions(&tp, 0)?;
    check(expect_code(&viv(&tp, &["stop"])?, 0))
}

/// Slice 025's acceptance: two sandboxes from different manifests running at once are both
/// named exactly once, and a running row's measured use is a reading that differs from its
/// declared ceiling — two numbers, not one printed twice (spec/17). The one trial that holds
/// two guests at the same time, and therefore this slice's flake-risk trial under host memory
/// pressure.
fn workflow_25_fleet_two_sandboxes() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("wf25-two").map_err(io_failed)?;
    let first = tp.root().join("two-first");
    let second = tp.root().join("two-second");
    fs::create_dir_all(&first).map_err(io_failed)?;
    fs::create_dir_all(&second).map_err(io_failed)?;
    for (name, tree) in [("two-a", &first), ("two-b", &second)] {
        arrange_manifest(
            &tp,
            name,
            "\n[resources]\nmem_mib = 2048\nvcpu = 2\n",
            &format!("\n[[workspaces]]\nsource = '{}'\n", tree.display()),
        )?;
    }
    check(expect_code(&viv_at(&tp, &first, &["start"])?, 0))?;
    check(expect_code(&viv_at(&tp, &second, &["start"])?, 0))?;

    let fleet = viv_at(&tp, &first, &["status", "-g", "--json"])?;
    check(expect_code(&fleet, 0))?;
    let projects = project_rows(&fleet)?;
    let mut names: Vec<&str> = projects
        .iter()
        .filter_map(|row| row["manifest"].as_str())
        .collect();
    names.sort_unstable();
    if names != ["two-a", "two-b"] {
        return fail(format!("the running fleet enumerated {names:?}"));
    }
    for row in &projects {
        if row["state"] != "running" {
            return fail(format!(
                "{} reported {} while its VM runs",
                row["manifest"], row["state"]
            ));
        }
        let Some(used) = row["runtime"]["mem_used_bytes"].as_u64() else {
            return fail(format!(
                "{} reported no measured memory on a delegated host",
                row["manifest"]
            ));
        };
        // The two-readings acceptance: a measurement, not the declaration echoed back.
        if used == 0 || used == 2048 * 1024 * 1024 {
            return fail(format!("{} measured `{used}`", row["manifest"]));
        }
        if row["runtime"]["sessions"].as_u64() != Some(0) {
            return fail(format!(
                "{} counted sessions {}",
                row["manifest"], row["runtime"]["sessions"]
            ));
        }
    }

    let human = viv_at(&tp, &first, &["status", "-g"])?;
    check(expect_code(&human, 0))?;
    check(expect_stdout_mentions(&human, "two-a"))?;
    check(expect_stdout_mentions(&human, "two-b"))?;
    check(expect_stdout_mentions(&human, "host:"))?;

    check(expect_code(&viv_at(&tp, &first, &["stop"])?, 0))?;
    check(expect_code(&viv_at(&tp, &second, &["stop"])?, 0))
}

/// The degraded ladder's floor: with the agent out of the picture the supervisor waits its
/// power-button window (6 s) and its destroy window (2 s) before the VM is gone, so a clean
/// stop under this figure cannot have been the power path's. The agent rung measures ~2-3 s,
/// so the margin absorbs host load without admitting the fallback.
const DEGRADED_LADDER_FLOOR: Duration = Duration::from_secs(8);

/// Slice 027's first-rung acceptance: an agent-reachable stop is initiated through the agent,
/// distinguished from the backend's power signal by evidence rather than assumption. The
/// backend's API socket is deleted first, so the power path cannot act at all and its degraded
/// ladder cannot finish under [`DEGRADED_LADDER_FLOOR`] — a clean stop under that floor whose
/// record reads `agent` can only have gone through the guest. The unsynced write proves N18
/// holds through the new rung, and the second leg deletes the control socket instead, forcing
/// the fall-through the ladder assigns to an unreachable agent.
fn workflow_27_agent_rung() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("wf27-rung").map_err(io_failed)?;
    arrange_manifest(&tp, "wf27-rung", "", "")?;
    check(expect_code(&viv(&tp, &["start"])?, 0))?;
    check(expect_code(
        &viv(
            &tp,
            &["exec", "--", "sh", "-lc", "printf u > \"$HOME/unsynced\""],
        )?,
        0,
    ))?;

    // Deleted, never renamed: the supervisor's teardown sweep refuses any runtime-directory
    // entry it was not told about — and refuses the whole sweep, not the one file.
    let runtime_dir = tp
        .runtime()
        .join("vivarium")
        .join("wf27-rung")
        .join("default");
    fs::remove_file(runtime_dir.join("api.sock")).map_err(io_failed)?;
    let started = Instant::now();
    let stopped = viv(&tp, &["stop", "--json"])?;
    let elapsed = started.elapsed();
    check(expect_code(&stopped, 0))?;
    check(expect_json_string(&stopped, "rung", "agent"))?;
    check(expect_json_string(&stopped, "state", "built"))?;
    if elapsed >= DEGRADED_LADDER_FLOOR {
        return fail(format!(
            "the stop took {elapsed:?}, past the floor the power path cannot get under"
        ));
    }

    check(expect_code(&viv(&tp, &["start"])?, 0))?;
    check(expect_code(
        &viv(
            &tp,
            &["exec", "--", "sh", "-lc", "test -f \"$HOME/unsynced\""],
        )?,
        0,
    ))?;

    fs::remove_file(runtime_dir.join("control.sock")).map_err(io_failed)?;
    let fallback = viv(&tp, &["stop", "--json"])?;
    check(expect_code(&fallback, 0))?;
    check(expect_json_string(&fallback, "rung", "power-signal"))
}

/// A guest whose every orderly shutdown deliberately takes ~20 s: a root oneshot's `ExecStop`
/// sleeps through the default grace, so only a longer `--timeout` lets the transaction finish.
const SLOW_STOP_IMAGE: &str = concat!(
    "{ pkgs, ... }:\n",
    "{\n",
    "  systemd.services.slow-stop = {\n",
    "    description = \"Deliberately slow shutdown\";\n",
    "    wantedBy = [ \"multi-user.target\" ];\n",
    "    serviceConfig = {\n",
    "      Type = \"oneshot\";\n",
    "      RemainAfterExit = true;\n",
    "      ExecStart = \"${pkgs.coreutils}/bin/true\";\n",
    "      ExecStop = \"${pkgs.coreutils}/bin/sleep 20\";\n",
    "    };\n",
    "  };\n",
    "}\n"
);

/// Slice 027's grace acceptance, `Q-018`'s exit made falsifiable. A deliberately slow guest
/// under `--timeout 40` is allowed its whole shutdown — the stop ends through the agent rung
/// after more time than the default grace would have permitted — while the same guest under
/// `--timeout 3` is observably escalated: the acknowledged-then-wedged shutdown is handed to
/// the power path's compiled window and the VM is down well before its own slow transaction
/// would have ended, with the record naming the rung that ended it.
fn workflow_27_slow_guest() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("wf27-slow").map_err(io_failed)?;
    arrange_manifest_with_image(&tp, "wf27-slow", "", "", SLOW_STOP_IMAGE)?;
    check(expect_code(&viv(&tp, &["start"])?, 0))?;

    let started = Instant::now();
    let patient = viv(&tp, &["stop", "--timeout", "40", "--json"])?;
    let patient_elapsed = started.elapsed();
    check(expect_code(&patient, 0))?;
    check(expect_json_string(&patient, "rung", "agent"))?;
    if patient_elapsed <= Duration::from_secs(10) {
        return fail(format!(
            "the patient stop finished in {patient_elapsed:?}; the guest was not slow"
        ));
    }

    check(expect_code(&viv(&tp, &["start"])?, 0))?;
    let started = Instant::now();
    let curt = viv(&tp, &["stop", "--timeout", "3", "--json"])?;
    let curt_elapsed = started.elapsed();
    check(expect_code(&curt, 0))?;
    check(expect_json_string(&curt, "rung", "power-signal"))?;
    if curt_elapsed >= Duration::from_secs(18) {
        return fail(format!(
            "the curt stop took {curt_elapsed:?}; the escalation never cut the shutdown short"
        ));
    }
    Ok(())
}

/// Slice 027's sweep acceptance: with two sandboxes running, `viv stop --all` from a directory
/// no manifest declares stops both and exits `0`; a second invocation is a `0` no-op with an
/// empty record; and an unsynced write survives the sweep, so N18 holds through it. The other
/// two-guest trial, sharing `workflow_25_fleet_two_sandboxes`'s flake-risk note under host
/// memory pressure.
fn workflow_27_sweep() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("wf27-all").map_err(io_failed)?;
    let first = tp.root().join("all-first");
    let second = tp.root().join("all-second");
    fs::create_dir_all(&first).map_err(io_failed)?;
    fs::create_dir_all(&second).map_err(io_failed)?;
    for (name, tree) in [("all-a", &first), ("all-b", &second)] {
        arrange_manifest(
            &tp,
            name,
            "\n[resources]\nmem_mib = 2048\nvcpu = 2\n",
            &format!("\n[[workspaces]]\nsource = '{}'\n", tree.display()),
        )?;
    }
    check(expect_code(&viv_at(&tp, &first, &["start"])?, 0))?;
    check(expect_code(&viv_at(&tp, &second, &["start"])?, 0))?;
    check(expect_code(
        &viv_at(
            &tp,
            &first,
            &["exec", "--", "sh", "-lc", "printf u > \"$HOME/unsynced\""],
        )?,
        0,
    ))?;

    // From the root the trees hang under, which no manifest declares: the sweep is the one
    // path that needs no selected manifest (spec/01, spec/10).
    let swept = viv_at(&tp, tp.root(), &["stop", "--all", "--json"])?;
    check(expect_code(&swept, 0))?;
    let rows = project_rows(&swept)?;
    let mut names: Vec<&str> = rows
        .iter()
        .filter_map(|row| row["manifest"].as_str())
        .collect();
    names.sort_unstable();
    if names != ["all-a", "all-b"] {
        return fail(format!("the sweep acted on {names:?}"));
    }
    for row in &rows {
        if row["state"] != "built" {
            return fail(format!("{} landed in {}", row["manifest"], row["state"]));
        }
        if row["rung"].as_str().is_none() {
            return fail(format!("{} reported no rung", row["manifest"]));
        }
    }

    let again = viv_at(&tp, tp.root(), &["stop", "--all", "--json"])?;
    check(expect_code(&again, 0))?;
    if !project_rows(&again)?.is_empty() {
        return fail("a second sweep re-reported sandboxes it had already stopped");
    }

    check(expect_code(&viv_at(&tp, &first, &["start"])?, 0))?;
    check(expect_code(
        &viv_at(
            &tp,
            &first,
            &["exec", "--", "sh", "-lc", "test -f \"$HOME/unsynced\""],
        )?,
        0,
    ))?;
    check(expect_code(&viv_at(&tp, tp.root(), &["stop", "--all"])?, 0))
}

/// Slice 028's memory rung against a live guest (spec/17, ADR-0113): dirtied guest page cache
/// comes back, the fall shows on the same path `viv status` reads, the workload survives with
/// its headroom restored, a second trim is a `0` fact, and the fan-out's record nests both
/// subtrees with no grand total. The cache is dirtied by reading through the guest — a volume
/// write would fill host page cache and measure the host (the ADR-0082 rabbit hole).
fn workflow_28_memory_reclaims() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("reclaim-project").map_err(io_failed)?;
    arrange_manifest(&tp, "reclaim", "", "")?;
    check(expect_code(&viv(&tp, &["start"])?, 0))?;

    // A survivor the trim must not disturb, detached from its session so its lifetime is the
    // guest's rather than the connection's.
    check(expect_code(
        &viv(
            &tp,
            &[
                "exec",
                "--",
                "sh",
                "-lc",
                "setsid sh -c 'sleep 300' < /dev/null > /dev/null 2>&1 \
                & echo $! > \"$HOME/worker.pid\"",
            ],
        )?,
        0,
    ))?;

    // Fill guest page cache: read the store through the guest, bounded by file count.
    check(expect_code(
        &viv(
            &tp,
            &[
                "exec",
                "--",
                "sh",
                "-lc",
                "find /nix/store -type f 2>/dev/null | head -n 4000 \
                | xargs cat > /dev/null 2>&1; true",
            ],
        )?,
        0,
    ))?;

    let before_status = mem_used_reading(&tp)?;

    let trimmed = viv(&tp, &["memory", "trim", "--json"])?;
    check(expect_code(&trimmed, 0))?;
    check(expect_json_keys(
        &trimmed,
        &[
            "manifest",
            "target_mib",
            "mem_used_before_bytes",
            "mem_used_after_bytes",
            "reclaimed_bytes",
        ],
    ))?;
    let record = json_record(&trimmed)?;
    if record["target_mib"].as_u64().is_none_or(|mib| mib == 0) {
        return fail(format!(
            "a completed run reported no target: {}",
            record["target_mib"]
        ));
    }
    let reclaimed = record["reclaimed_bytes"].as_u64().unwrap_or(0);
    if reclaimed == 0 {
        return fail("a trim over a cache-heavy guest reclaimed nothing");
    }

    // The fall shows on the same path `status` reads, so the two commands agree (spec/01).
    let after_status = mem_used_reading(&tp)?;
    if after_status >= before_status {
        return fail(format!(
            "status reported no fall: {before_status} then {after_status}"
        ));
    }

    // The workload is still running, and can take memory back: the guest allocates again.
    check(expect_code(
        &viv(
            &tp,
            &[
                "exec",
                "--",
                "sh",
                "-lc",
                "kill -0 \"$(cat \"$HOME/worker.pid\")\"",
            ],
        )?,
        0,
    ))?;

    // Reclaiming nothing is a fact about the guest, not a failure (spec/01).
    let again = viv(&tp, &["memory", "trim", "--json"])?;
    check(expect_code(&again, 0))?;

    // The fan-out in the same boot: one subtree per resource, the sandbox key hoisted, and
    // deliberately no top-level total (ADR-0113).
    let fanned = viv(&tp, &["trim", "--json"])?;
    check(expect_code(&fanned, 0))?;
    check(expect_json_keys(&fanned, &["manifest", "memory", "disk"]))?;
    let record = json_record(&fanned)?;
    if !record["reclaimed_bytes"].is_null() {
        return fail("the fan-out minted a top-level total");
    }
    for (subtree, key) in [("memory", "mem_used_before_bytes"), ("disk", "volumes")] {
        if record[subtree][key].is_null() {
            return fail(format!(
                "the `{subtree}` subtree is not its command's record"
            ));
        }
    }

    check(expect_code(&viv(&tp, &["stop"])?, 0))
}

/// Slice 028's disk rung against a live guest (spec/17, ADR-0037): freed bytes inside the
/// default volume return to the sparse host image, measured on the image's own allocated
/// blocks — never `fstrim`'s report — and joined to what `viv volume list` reads.
fn workflow_28_volume_trim() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("trim-volume-project").map_err(io_failed)?;
    arrange_manifest(&tp, "trimvol", "", "")?;
    check(expect_code(&viv(&tp, &["start"])?, 0))?;

    // Write, delete, and settle half a GiB inside the home volume.
    check(expect_code(
        &viv(
            &tp,
            &[
                "exec",
                "--",
                "sh",
                "-lc",
                "dd if=/dev/zero of=\"$HOME/blob\" bs=1M count=512 conv=fsync status=none \
                && rm \"$HOME/blob\" && sync",
            ],
        )?,
        0,
    ))?;

    let listed = viv(&tp, &["volume", "list", "--json"])?;
    check(expect_code(&listed, 0))?;
    let before_rows = json_record(&listed)?;
    let listed_before = volume_row_field(&before_rows, "default", "allocated_bytes")?;

    let trimmed = viv(&tp, &["volume", "trim", "--json"])?;
    check(expect_code(&trimmed, 0))?;
    let record = json_record(&trimmed)?;
    let rows = record["volumes"]
        .as_array()
        .cloned()
        .ok_or_else(|| Failed::from("the trim record published no `volumes` array"))?;
    let total: u64 = rows
        .iter()
        .filter_map(|row| row["reclaimed_bytes"].as_u64())
        .sum();
    if record["reclaimed_bytes"].as_u64() != Some(total) {
        return fail(format!(
            "the top-level total {} is not the sum of the rows {total}",
            record["reclaimed_bytes"]
        ));
    }
    let default_row = rows
        .iter()
        .find(|row| row["name"] == "default")
        .ok_or_else(|| Failed::from("the default volume reported no row"))?;
    let before = default_row["allocated_before_bytes"].as_u64().unwrap_or(0);
    let after = default_row["allocated_after_bytes"]
        .as_u64()
        .unwrap_or(u64::MAX);
    if after >= before {
        return fail(format!(
            "the default volume's image did not shrink: {before} then {after}"
        ));
    }
    // The written 512 MiB came back at least in large part; a token fall would pass a broken
    // discard chain (the harness-method lesson: measure the outcome, not the motion).
    if before.saturating_sub(after) < 256 * 1024 * 1024 {
        return fail(format!(
            "the trim returned only {} bytes of the 512 MiB written",
            before.saturating_sub(after)
        ));
    }
    if listed_before != before {
        return fail(format!(
            "the trim's `before` ({before}) is not `volume list`'s reading ({listed_before})"
        ));
    }

    // Joined after as well: one measurement, two readers (spec/01).
    let relisted = viv(&tp, &["volume", "list", "--json"])?;
    check(expect_code(&relisted, 0))?;
    let after_rows = json_record(&relisted)?;
    let listed_after = volume_row_field(&after_rows, "default", "allocated_bytes")?;
    if listed_after > before {
        return fail(format!(
            "volume list re-read {listed_after} after a trim that ended at {after}"
        ));
    }

    check(expect_code(&viv(&tp, &["stop"])?, 0))
}

/// One named volume row's field out of a `volume list`/`volume trim` record.
fn volume_row_field(record: &serde_json::Value, name: &str, field: &str) -> Result<u64, Failed> {
    record["volumes"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["name"] == name))
        .and_then(|row| row[field].as_u64())
        .ok_or_else(|| Failed::from(format!("no `{field}` for volume `{name}` in {record}")))
}

/// Reads `runtime.mem_used_bytes` out of the local report, failing when it is unavailable —
/// the trials that call this already require the same delegated controller the product does.
fn mem_used_reading(tp: &TempProject) -> Result<u64, Failed> {
    let status = viv(tp, &["status", "--json"])?;
    check(expect_code(&status, 0))?;
    let record = json_record(&status)?;
    record["runtime"]["mem_used_bytes"]
        .as_u64()
        .ok_or_else(|| Failed::from("status reported no mem_used_bytes on a delegated host"))
}

/// Slice 026's admission refusal (spec/17 row 1, N23): below the reserve, `viv start` exits
/// `69` under `host.memory-reserve` with nothing built, nothing locked, and no unit — the
/// absence demonstrated rather than assumed. The low reading is bind-mounted over
/// `/proc/meminfo` inside a private user+mount namespace, so the product path reads it through
/// its ordinary reader and no test seam exists. On this host the fleet term is unmeasurable
/// (`host-cgroup2-delegation` trips), so the same run demonstrates the degraded tier: the
/// decision was reached from host-level readings, never skipped.
fn workflow_26_admission_refusal() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("wf26-adm").map_err(io_failed)?;
    arrange_manifest(&tp, "wf26-adm", "", "")?;

    // 512 MiB available: unambiguously below the 1 GiB reserve, beside a plausible total.
    let crafted = tp.root().join("meminfo");
    fs::write(
        &crafted,
        "MemTotal:       33554432 kB\nMemFree:          409600 kB\nMemAvailable:     524288 kB\n",
    )
    .map_err(io_failed)?;

    // Capability precheck, failing with a named reason rather than skipping silently: on a
    // kernel that refuses the bind the trial can prove nothing, and a quiet skip would read as
    // green (the harness-method lesson).
    let precheck = std::process::Command::new("unshare")
        .args([
            "--map-root-user",
            "--mount",
            "sh",
            "-c",
            "mount --bind \"$WF26_MEMINFO\" /proc/meminfo \
                && grep -q 'MemAvailable:     524288' /proc/meminfo",
        ])
        .env("WF26_MEMINFO", &crafted)
        .output()
        .map_err(io_failed)?;
    if !precheck.status.success() {
        return fail(format!(
            "this kernel refused the user-namespace bind over /proc/meminfo, so the refusal \
            trial cannot arrange a low reading: {}",
            String::from_utf8_lossy(&precheck.stderr).trim()
        ));
    }

    // Prewarm the derived index: the enumeration surface republishes it as recomputable cache,
    // and a snapshot taken before it exists would blame admission for that write.
    check(expect_code(&viv(&tp, &["status", "-g"])?, 0))?;
    let state_before = snapshot_tree(tp.state()).map_err(io_failed)?;
    let cache_before = snapshot_tree(tp.cache()).map_err(io_failed)?;

    let mut command = std::process::Command::new("unshare");
    command
        .args([
            "--map-root-user",
            "--mount",
            "sh",
            "-c",
            "mount --bind \"$WF26_MEMINFO\" /proc/meminfo && exec \"$WF26_VIV\" start",
        ])
        .current_dir(tp.project())
        .env_clear();
    command.envs(viv_environment(&tp));
    command.env("WF26_MEMINFO", &crafted);
    command.env("WF26_VIV", preflight::viv());
    let refused = command.output().map_err(io_failed)?;

    if refused.status.code() != Some(EX_UNAVAILABLE) {
        return fail(format!(
            "a start below the reserve answered {:?} instead of 69; stderr: {}",
            refused.status.code(),
            String::from_utf8_lossy(&refused.stderr).trim()
        ));
    }
    let stderr = String::from_utf8_lossy(&refused.stderr);
    if !stderr.contains("memory-reserve") {
        return fail(format!(
            "the refusal did not name `host.memory-reserve`: {stderr}"
        ));
    }

    // The absences the acceptance demands, each named: no build output or generation reached
    // the state root, no cache drift, no runtime target directory, and no unit.
    check(expect_tree_unchanged(tp.state(), &state_before))?;
    check(expect_tree_unchanged(tp.cache(), &cache_before))?;
    let runtime_dir = tp
        .runtime()
        .join("vivarium")
        .join("wf26-adm")
        .join("default");
    if runtime_dir.exists() {
        return fail("a refused start created its runtime target directory");
    }
    let unit = std::process::Command::new("systemctl")
        .args([
            "--user",
            "show",
            "-p",
            "ActiveState",
            "--value",
            "vivarium-wf26-adm-default.service",
        ])
        .output()
        .map_err(io_failed)?;
    if unit.status.success() && String::from_utf8_lossy(&unit.stdout).trim() == "active" {
        return fail("a refused start left an active unit behind");
    }
    Ok(())
}

/// Slice 026's attach stream (spec/10, acceptance row 5): `viv start --attach` boots, streams
/// the guest console to stdout, `SIGINT` detaches at `0`, and a `viv status` taken after the
/// detach shows the sandbox still running. The silent tier rides along: an ordinary start's
/// stderr carries no admission warning. Between the legs, an attach against the
/// already-running sandbox says so in words and streams nothing. The whole leg runs twice,
/// because one clean run is evidence of possibility rather than reliability.
fn workflow_26_attach_stream() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("wf26-att").map_err(io_failed)?;
    arrange_manifest(&tp, "wf26-att", "", "")?;

    for leg in 1..=2_u32 {
        attach_boot_and_detach(&tp, leg)?;

        if leg == 1 {
            // The recorded out-of-scope, demonstrated rather than left to reading: attaching to
            // a VM this command did not boot is answered in words, with nothing streamed.
            let again = viv(&tp, &["start", "--attach"])?;
            check(expect_code(&again, 0))?;
            check(expect_stderr_mentions(
                &again,
                "streams only a boot this command performs",
            ))?;
            if !again.stdout.is_empty() {
                return fail("an attach against a running sandbox streamed console bytes");
            }

            // The non-SIGINT contract at process level: a SIGTERM landing while the attached
            // start is still in its blocking phase is consumed by the eager watchers and
            // answered `128+15` on the already-running outcome, never swallowed into a `0`.
            let mut command = std::process::Command::new(preflight::viv());
            command
                .args(["start", "--attach"])
                .current_dir(tp.project())
                .env_clear()
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            command.envs(viv_environment(&tp));
            let mut termed = KillOnDrop(command.spawn().map_err(io_failed)?);
            // Past the watcher installation at entry, still well inside the evaluation the
            // running sandbox's pipeline performs before its short-circuit.
            std::thread::sleep(Duration::from_millis(300));
            let pid = rustix::process::Pid::from_raw(
                i32::try_from(termed.0.id()).map_err(|error| Failed::from(error.to_string()))?,
            )
            .ok_or_else(|| Failed::from("the attach child has no pid"))?;
            rustix::process::kill_process(pid, rustix::process::Signal::TERM)
                .map_err(errno_failed)?;
            let status = termed.0.wait().map_err(io_failed)?;
            if status.code() != Some(128 + 15) {
                return fail(format!(
                    "a SIGTERM during the blocking phase answered {:?} instead of 143",
                    status.code()
                ));
            }
        }

        check(expect_code(&viv(&tp, &["stop"])?, 0))?;
    }
    Ok(())
}

/// One boot-stream-detach-status leg of the attach trial.
fn attach_boot_and_detach(tp: &TempProject, leg: u32) -> Result<(), Failed> {
    use std::io::Read as _;

    let mut command = std::process::Command::new(preflight::viv());
    command
        .args(["start", "--attach"])
        .current_dir(tp.project())
        .env_clear()
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    command.envs(viv_environment(tp));
    let mut spawned = command.spawn().map_err(io_failed)?;
    let stdout = spawned
        .stdout
        .take()
        .ok_or_else(|| Failed::from("the attach child has no stdout pipe"))?;
    let stderr_pipe = spawned
        .stderr
        .take()
        .ok_or_else(|| Failed::from("the attach child has no stderr pipe"))?;
    let mut child = KillOnDrop(spawned);

    let collected: std::sync::Arc<std::sync::Mutex<Vec<u8>>> = std::sync::Arc::default();
    let sink = std::sync::Arc::clone(&collected);
    let stdout_reader = std::thread::spawn(move || {
        let mut stdout = stdout;
        let mut buffer = [0_u8; 8192];
        loop {
            match stdout.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    if let Ok(mut bytes) = sink.lock() {
                        bytes.extend_from_slice(&buffer[..count]);
                    }
                }
            }
        }
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut stderr_pipe = stderr_pipe;
        let mut bytes = Vec::new();
        let _ = stderr_pipe.read_to_end(&mut bytes);
        bytes
    });

    // The stream phase begins once the boot concluded, and `status` is its observable. A
    // SIGINT sent earlier would land on the build, which is the detached form's own Ctrl-C
    // story rather than a detach.
    let running_deadline = Instant::now() + Duration::from_mins(15);
    loop {
        if Instant::now() > running_deadline {
            return fail(format!("leg {leg}: the sandbox never reported running"));
        }
        let status = viv(tp, &["status", "--json"])?;
        if expect_json_string(&status, "state", "running").is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_secs(2));
    }

    // The distinctive marker: the kernel banner opens every guest console, so its arrival
    // proves the console reached stdout rather than accepting any bytes at all.
    let marker_deadline = Instant::now() + Duration::from_mins(1);
    loop {
        if collected
            .lock()
            .ok()
            .is_some_and(|bytes| String::from_utf8_lossy(&bytes).contains("Linux version"))
        {
            break;
        }
        if Instant::now() > marker_deadline {
            let streamed = collected.lock().map_or(0, |bytes| bytes.len());
            return fail(format!(
                "leg {leg}: no kernel banner reached stdout ({streamed} bytes streamed)"
            ));
        }
        std::thread::sleep(Duration::from_millis(200));
    }

    let pid = rustix::process::Pid::from_raw(
        i32::try_from(child.0.id()).map_err(|error| Failed::from(error.to_string()))?,
    )
    .ok_or_else(|| Failed::from("the attach child has no pid"))?;
    rustix::process::kill_process(pid, rustix::process::Signal::INT).map_err(errno_failed)?;
    let status = child.0.wait().map_err(io_failed)?;
    let _ = stdout_reader.join();
    let stderr_bytes = stderr_reader.join().unwrap_or_default();

    // SIGINT is the designed detach gesture, and the attached form's success (spec/10).
    if status.code() != Some(0) {
        return fail(format!(
            "leg {leg}: the detach answered {:?}; stderr: {}",
            status.code(),
            String::from_utf8_lossy(&stderr_bytes).trim()
        ));
    }
    // The silent tier (spec/17 row 3): nothing beyond what an ordinary start emits reached
    // stderr — in particular, no admission warning.
    let stderr_text = String::from_utf8_lossy(&stderr_bytes);
    if stderr_text.contains("warning:") {
        return fail(format!(
            "leg {leg}: an ordinary start warned: {stderr_text}"
        ));
    }

    // The acceptance's demanded demonstration: the sandbox survived the detach.
    check(expect_json_string(
        &viv(tp, &["status", "--json"])?,
        "state",
        "running",
    ))
}

/// Reads `runtime.sessions` out of the local report, `None` when the agent could not answer.
fn session_reading(tp: &TempProject) -> Result<Option<u64>, Failed> {
    let status = viv(tp, &["status", "--json"])?;
    check(expect_code(&status, 0))?;
    let record: serde_json::Value = serde_json::from_slice(&status.stdout)
        .map_err(|error| Failed::from(format!("status --json was not JSON: {error}")))?;
    Ok(record["runtime"]["sessions"].as_u64())
}

/// Polls the count toward `expected`: attach and detach are asynchronous on both ends, so a
/// single reading would race the transition it asserts.
fn poll_sessions(tp: &TempProject, expected: u64) -> Result<(), Failed> {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut last = None;
    while Instant::now() < deadline {
        last = session_reading(tp)?;
        if last == Some(expected) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    fail(format!(
        "the session count never reached {expected}; last reading {last:?}"
    ))
}

/// A concurrently held `viv exec` session: spawned, not awaited, so the trial can observe the
/// count while the session lives.
fn spawn_viv_session(tp: &TempProject) -> Result<std::process::Child, Failed> {
    let mut command = std::process::Command::new(preflight::viv());
    command
        .args(["exec", "--", "sleep", "600"])
        .current_dir(tp.project())
        .env_clear()
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    command.envs(support::viv_environment(tp));
    command.spawn().map_err(io_failed)
}

/// The parsed rows of `generations list --json`, oldest first as published.
fn generations_rows(tp: &TempProject) -> Result<Vec<serde_json::Value>, Failed> {
    let list = viv(tp, &["generations", "list", "--json"])?;
    check(expect_code(&list, 0))?;
    let record: serde_json::Value = serde_json::from_slice(&list.stdout)
        .map_err(|error| Failed::from(format!("generations list --json was not JSON: {error}")))?;
    record["generations"]
        .as_array()
        .cloned()
        .ok_or_else(|| Failed::from("generations list --json published no `generations` array"))
}

/// Asserts against the store — `nix-store --query --roots` — whether this generation's link is
/// among the roots of its output. The store rather than the filesystem, because a symlink that
/// is not registered is the exact defect slice 032 exists to remove. Only stdout answers:
/// `--roots` prints stale-root housekeeping to stderr.
fn expect_generation_rooted(
    tp: &TempProject,
    manifest: &str,
    number: u64,
    store_path: &str,
    expected: bool,
) -> Result<(), String> {
    let link = tp
        .state()
        .join("vivarium")
        .join("projects")
        .join(manifest)
        .join("default")
        .join("generations")
        .join(number.to_string());
    let output = std::process::Command::new("nix-store")
        .args(["--query", "--roots"])
        .arg(store_path)
        .output()
        .map_err(|error| format!("could not run nix-store --query --roots: {error}"))?;
    let named = String::from_utf8_lossy(&output.stdout);
    let rooted = named
        .lines()
        .any(|line| line.contains(&link.display().to_string()));
    if rooted == expected {
        Ok(())
    } else if expected {
        Err(format!(
            "no root names {}; roots of {store_path}: {named}",
            link.display()
        ))
    } else {
        Err(format!(
            "{} still roots {store_path} after its unlink: {named}",
            link.display()
        ))
    }
}

/// Points the profile's `current` at `build` as generation `number`, standing in for a retained
/// build the trial did not really produce — the reader is a symlink chain plus an existence
/// check, so no store is needed.
fn fabricate_generation(
    tp: &TempProject,
    manifest: &str,
    number: u64,
    build: &Path,
) -> Result<(), Failed> {
    let root = tp
        .state()
        .join("vivarium")
        .join("projects")
        .join(manifest)
        .join("default");
    let generations = root.join("generations");
    fs::create_dir_all(&generations).map_err(io_failed)?;
    let link = generations.join(number.to_string());
    let _ = fs::remove_file(&link);
    std::os::unix::fs::symlink(build, &link).map_err(io_failed)?;
    let current = root.join("current");
    let _ = fs::remove_file(&current);
    std::os::unix::fs::symlink(
        PathBuf::from("generations").join(number.to_string()),
        &current,
    )
    .map_err(io_failed)?;
    Ok(())
}

/// Whether a file with this exact name exists anywhere under `root`.
fn path_named_exists(root: &Path, name: &str) -> bool {
    path_named(root, name).is_some()
}

/// The first path with this exact name anywhere under `root`.
///
/// The runtime root a trial holds is the session's, while a guest's own artefacts live a few
/// levels down under project and target names the trial does not spell — so the search is by name
/// rather than by constructed path.
fn path_named(root: &Path, name: &str) -> Option<PathBuf> {
    let entries = fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.file_name().is_some_and(|found| found == name) {
            return Some(path);
        }
        if path.is_dir()
            && let Some(found) = path_named(&path, name)
        {
            return Some(found);
        }
    }
    None
}
