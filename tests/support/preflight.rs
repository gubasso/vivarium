//! What each lane needs, asked once, by the binary that needs it.
//!
//! This module replaces the runtime gate the acceptance harness used to carry. That
//! gate probed the host and turned an unmet capability into a skipped trial, so a
//! machine without `/dev/kvm` reported a green run over acceptance trials that never
//! executed, and `VIVARIUM_TEST_REQUIRE=1` was the hidden knob that inverted it. The
//! rule now is the one `docs/reference/testing-lanes.md` states: a lane runs where a
//! place names it, and nowhere else.
//!
//! So the shape here is different in one way that matters. Nothing below decides
//! whether a trial runs. Each function answers whether this host can honour a
//! promise the binary already made by being invoked, and its `Err` is the message
//! the whole binary refuses with. A `boot` binary started on a laptop fails loudly
//! and says which of `/dev/kvm`, the user manager or `XDG_RUNTIME_DIR` is missing.
//!
//! Every function is called from a binary's `main`, after `Arguments::from_args()`
//! and only when the run is not a listing. nextest builds its test list by running
//! each binary with `--list`, so a refusal before that point would abort the entire
//! run rather than this one lane.

use std::path::Path;
use std::process::Command;

/// The `viv` under test, resolved at compile time.
///
/// Cargo sets `CARGO_BIN_EXE_viv` for every integration-test target, `harness = false`
/// included, and nextest preserves it. A search of `PATH` or of `target/debug` is what
/// the predecessor did, and it could resolve a binary nobody was building — a stale
/// artefact from before the dev shell moved `CARGO_TARGET_DIR`, reported as a product
/// failure. A missing binary is now a build error, which is the earlier and louder
/// place for it.
pub fn viv() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_viv"))
}

/// That Nix is on `PATH` and runs. The `eval` and `boot` lanes both need it.
pub fn nix() -> Result<(), String> {
    match Command::new("nix").arg("--version").output() {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => Err(format!("`nix --version` exited {:?}", output.status.code())),
        Err(error) => Err(format!("`nix` is unavailable: {error}")),
    }
}

/// That this host can boot a guest: the KVM device, a systemd user manager, a runtime
/// directory, and Nix to build the guest with.
///
/// Ordered cheapest first, so the earliest failure is the most actionable — the same
/// ordering rule `spec/10-vm-lifecycle.md` fixes for the product's own hard preflight.
pub fn boot() -> Result<(), String> {
    kvm()?;
    runtime_dir()?;
    user_manager()?;
    nix()
}

/// That `/dev/kvm` exists and opens read-write.
///
/// Existence alone is not the question. A GitHub runner carries the device node with
/// `0666` and no group membership needed; a distribution that ships `0660 root:kvm`
/// gives a user outside that group a device that stats fine and refuses to open.
pub fn kvm() -> Result<(), String> {
    let path = Path::new("/dev/kvm");
    if !path.exists() {
        return Err("/dev/kvm is absent".to_owned());
    }
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map(drop)
        .map_err(|error| format!("/dev/kvm is not read-write openable: {error}"))
}

/// That `XDG_RUNTIME_DIR` names a directory.
///
/// ADR-0055 makes the runtime root's existence and privacy a precondition of launching
/// at all, and the control socket lives under it against a 108-byte path limit.
pub fn runtime_dir() -> Result<(), String> {
    match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(value) if Path::new(&value).is_dir() => Ok(()),
        Some(value) => Err(format!(
            "XDG_RUNTIME_DIR is not a directory: {}",
            Path::new(&value).display()
        )),
        None => Err("XDG_RUNTIME_DIR is unset".to_owned()),
    }
}

/// That a systemd user manager answers.
///
/// ADR-0097 gives a VM's lifetime to a transient user unit, so a host with no user
/// manager cannot start one however good its KVM support is. This is the premise a
/// continuous-integration runner is least likely to hold, and the one whose absence
/// previously read as a product defect: the launcher reported a child exit and said
/// nothing about the manager that was never there.
pub fn user_manager() -> Result<(), String> {
    match Command::new("systemctl")
        .args(["--user", "show", "-p", "Version", "--value"])
        .output()
    {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => Err(format!(
            "`systemctl --user` exited {:?}; no systemd user manager: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        )),
        Err(error) => Err(format!("`systemctl` is unavailable: {error}")),
    }
}

/// The four namespace tools, and an unprivileged user+net pair actually creatable.
///
/// The second half is the one that discriminates. A container, a host with
/// `kernel.unprivileged_userns_clone=0`, and Ubuntu's `AppArmor` restriction all carry
/// the tools and refuse the pair, so a check that only resolved `PATH` would pass and
/// leave every trial to fail on the refusal instead.
pub fn namespaces() -> Result<(), String> {
    for tool in ["unshare", "nsenter", "ip", "nft"] {
        super::harness::tool_on_path(tool)?;
    }
    let unshare = super::harness::tool_on_path("unshare")?;
    let probe = Command::new(&unshare)
        .args(vivarium::net::netns::create_pair_args(
            &vivarium::net::netns::holder_program("true"),
        ))
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .output()
        .map_err(|error| format!("probing `unshare` failed: {error}"))?;
    if !probe.status.success() {
        return Err(format!(
            "unprivileged user+net namespaces are unavailable: {}",
            String::from_utf8_lossy(&probe.stderr).trim()
        ));
    }
    Ok(())
}

/// The Nix-built guest runner the agent trial drives, built here when nothing named one.
///
/// `VIVARIUM_AGENT_RUNNER` stays honoured, because `tests/host/guest-agent-check` builds
/// this image for its own reasons and there is no sense paying for it twice. What is new
/// is the fallback: with the variable unset the lane builds the image itself rather than
/// refusing, so `boot_guest_agent` is a lane a person can run rather than one that needs
/// a wrapper script to arrange its inputs first. The build is a store hit whenever the
/// script has already run.
pub fn agent_runner() -> Result<std::path::PathBuf, String> {
    if let Some(value) = std::env::var_os("VIVARIUM_AGENT_RUNNER") {
        let path = std::path::PathBuf::from(value);
        if !path.is_file() {
            return Err(format!(
                "VIVARIUM_AGENT_RUNNER is not a file: {}",
                path.display()
            ));
        }
        return Ok(path);
    }
    let flake = format!(
        "path:{}?dir=nix#base-image-agent-check",
        env!("CARGO_MANIFEST_DIR")
    );
    let built = Command::new("nix")
        .args([
            "build",
            "--no-link",
            "--print-out-paths",
            "--extra-experimental-features",
            "nix-command flakes",
            &flake,
        ])
        .output()
        .map_err(|error| format!("`nix build` is unavailable: {error}"))?;
    if !built.status.success() {
        return Err(format!(
            "building {flake} failed: {}",
            String::from_utf8_lossy(&built.stderr).trim()
        ));
    }
    let out = String::from_utf8_lossy(&built.stdout).trim().to_owned();
    if out.is_empty() {
        return Err(format!("building {flake} printed no store path"));
    }
    let runner = std::path::PathBuf::from(out).join("bin/vivarium-base-image");
    if !runner.is_file() {
        return Err(format!(
            "the built agent image carries no runner at {}",
            runner.display()
        ));
    }
    Ok(runner)
}
