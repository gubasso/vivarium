mod support;

use std::fs;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use libtest_mimic::{Arguments, Failed, Trial};
use support::{
    EX_CONFIG, EX_DATAERR, EX_IOERR, EX_TEMPFAIL, EX_UNAVAILABLE, EX_USAGE, GateLevel, TempProject,
    VivOutput, expect_code, expect_derived_manifest_visible, expect_json_array_items,
    expect_json_array_nonempty, expect_json_fields_at, expect_json_keys, expect_json_map_entries,
    expect_json_string, expect_no_volume_images, expect_nonzero, expect_stderr_mentions,
    expect_stdout_lacks, expect_stdout_mentions, expect_tree_unchanged, expect_volume_image, gate,
    json, run_viv, snapshot_tree, viv_environment, volume_image, write_file,
};

type WorkflowRunner = fn() -> Result<(), Failed>;
type WorkflowSpec = (&'static str, GateLevel, WorkflowRunner);

/// One trial per workflow *and gate level*, not per workflow. A trial carries a single
/// ignore flag, so folding a workflow's CLI-only assertions in with its boot-dependent
/// ones would hide the cheap half behind `/dev/kvm` — exactly what the three-level gate
/// exists to avoid. Every trial keeps its `workflow_NN_` prefix so the guide pairing
/// survives the split.
const WORKFLOWS: [WorkflowSpec; 44] = [
    (
        "workflow_01_manifest_workspace_resolution_usage",
        GateLevel::Cli,
        workflow_01_usage,
    ),
    (
        "workflow_01_manifest_workspace_resolution_boot",
        GateLevel::Virtualization,
        workflow_01_boot,
    ),
    (
        "workflow_02_derived_workspace_index",
        GateLevel::Cli,
        workflow_02_index,
    ),
    (
        "workflow_03_team_shared_and_personal_override",
        GateLevel::ConfigEval,
        workflow_03,
    ),
    (
        "workflow_04_inspect_before_run_usage",
        GateLevel::Cli,
        workflow_04_usage,
    ),
    (
        "workflow_04_inspect_before_run",
        GateLevel::ConfigEval,
        workflow_04_eval,
    ),
    (
        "workflow_05_restrict_egress_config_surface",
        GateLevel::ConfigEval,
        workflow_05_config,
    ),
    (
        "workflow_05_restrict_egress_allowlist",
        GateLevel::Virtualization,
        workflow_05_enforcement,
    ),
    (
        "workflow_06_exec_usage_surface",
        GateLevel::Cli,
        workflow_06_usage,
    ),
    (
        "workflow_06_exec_exit_code_propagation",
        GateLevel::Virtualization,
        workflow_06_propagation,
    ),
    (
        "workflow_06_shell_interactive_session",
        GateLevel::Virtualization,
        workflow_06_shell,
    ),
    (
        "workflow_07_volume_list_requires_manifest",
        GateLevel::Cli,
        workflow_07_usage,
    ),
    (
        "workflow_07_stop_restart_preserving_volumes",
        GateLevel::Virtualization,
        workflow_07_warmth,
    ),
    (
        "workflow_07_stop_completes_within_budget",
        GateLevel::Virtualization,
        workflow_07_shutdown_bounded,
    ),
    (
        "workflow_08_destroy_usage_surface",
        GateLevel::Cli,
        workflow_08_usage,
    ),
    (
        "workflow_08_destroy_cold_rebuild",
        GateLevel::Virtualization,
        workflow_08_rebuild,
    ),
    (
        "workflow_09_workspace_round_trip",
        GateLevel::Virtualization,
        workflow_09_round_trip,
    ),
    (
        "workflow_16_doctor_report",
        GateLevel::Cli,
        workflow_16_doctor,
    ),
    (
        "workflow_17_declared_mounts_eval",
        GateLevel::ConfigEval,
        workflow_17_eval,
    ),
    (
        "workflow_17_declared_mounts_refusals",
        GateLevel::Virtualization,
        workflow_17_refusals,
    ),
    (
        "workflow_17_declared_mounts_round_trip",
        GateLevel::Virtualization,
        workflow_17_round_trip,
    ),
    (
        "workflow_17_linked_worktree_reaches_main",
        GateLevel::Virtualization,
        workflow_17_worktree,
    ),
    (
        "workflow_20_many_workspaces_one_sandbox",
        GateLevel::Virtualization,
        workflow_20_many_workspaces,
    ),
    (
        "workflow_22_file_mount_serves_only_its_file",
        GateLevel::Virtualization,
        workflow_22_file_mount_confinement,
    ),
    (
        "workflow_15_contract_skew_refusal",
        GateLevel::Virtualization,
        workflow_15_contract_skew,
    ),
    (
        "workflow_15_contract_skew_live",
        GateLevel::Virtualization,
        workflow_15_contract_skew_live,
    ),
    (
        "workflow_23_agent_channel_config_surface",
        GateLevel::ConfigEval,
        workflow_23_config,
    ),
    (
        "workflow_23_agent_channel_relay",
        GateLevel::Virtualization,
        workflow_23_relay,
    ),
    (
        "workflow_18_update_usage_surface",
        GateLevel::Cli,
        workflow_18_update_usage,
    ),
    (
        "workflow_18_update_moves_the_pin",
        GateLevel::ConfigEval,
        workflow_18_update_pin,
    ),
    (
        "workflow_24_generations_usage_surface",
        GateLevel::Cli,
        workflow_24_usage,
    ),
    (
        "workflow_24_generations_retention",
        GateLevel::Virtualization,
        workflow_24_retention,
    ),
    (
        "workflow_25_fleet_usage_surface",
        GateLevel::Cli,
        workflow_25_fleet_usage,
    ),
    (
        "workflow_25_sessions_counted",
        GateLevel::Virtualization,
        workflow_25_sessions_count,
    ),
    (
        "workflow_25_fleet_two_sandboxes",
        GateLevel::Virtualization,
        workflow_25_fleet_two_sandboxes,
    ),
    (
        "workflow_26_admission_refusal",
        GateLevel::Virtualization,
        workflow_26_admission_refusal,
    ),
    (
        "workflow_26_start_attach_stream",
        GateLevel::Virtualization,
        workflow_26_attach_stream,
    ),
    (
        "workflow_27_stop_destroy_records",
        GateLevel::Cli,
        workflow_27_records,
    ),
    (
        "workflow_27_stop_agent_rung_evidence",
        GateLevel::Virtualization,
        workflow_27_agent_rung,
    ),
    (
        "workflow_27_stop_slow_guest_grace",
        GateLevel::Virtualization,
        workflow_27_slow_guest,
    ),
    (
        "workflow_27_stop_all_sweep",
        GateLevel::Virtualization,
        workflow_27_sweep,
    ),
    (
        "workflow_28_trim_usage_surface",
        GateLevel::Cli,
        workflow_28_usage,
    ),
    (
        "workflow_28_memory_trim_reclaims",
        GateLevel::Virtualization,
        workflow_28_memory_reclaims,
    ),
    (
        "workflow_28_volume_trim_returns_blocks",
        GateLevel::Virtualization,
        workflow_28_volume_trim,
    ),
];

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

fn main() -> std::process::ExitCode {
    // The egress fixture re-executes this binary inside the VM's namespaces; a
    // fixture-mode invocation never reaches the trial harness.
    if let Some(code) = support::egress::run_mode() {
        return code;
    }
    let args = Arguments::from_args();
    let mut trials = vec![Trial::test("harness_self_check", harness_self_check)];
    for (name, level, runner) in WORKFLOWS {
        trials.push(gated_trial(name, level, runner));
    }
    libtest_mimic::run(&args, trials).exit_code()
}

use support::harness::gate_required;

fn gated_trial(name: &'static str, level: GateLevel, runner: fn() -> Result<(), Failed>) -> Trial {
    let require = gate_required();
    let decision = gate().result(level);
    let ignored = decision.is_err() && !require;
    if ignored && let Err(reason) = decision {
        eprintln!("gated: {name} — {reason}");
    }
    Trial::test(name, move || {
        require_gate(level)?;
        runner()
    })
    .with_ignored_flag(ignored)
}

fn harness_self_check() -> Result<(), Failed> {
    let tp = TempProject::new().map_err(io_failed)?;
    for path in [
        tp.project(),
        tp.home(),
        tp.config(),
        tp.state(),
        tp.data(),
        tp.cache(),
        tp.runtime(),
    ] {
        if !path.is_dir() {
            return fail(format!(
                "isolated directory was not created: {}",
                path.display()
            ));
        }
    }
    // Five of the six roots are durable and live inside the temp root; the runtime root is the
    // exception, and deliberately so. `TempProject` places it under the session's own
    // `/run/user/<uid>` because a control-socket path assembled beneath `TMPDIR` overruns the
    // 108-byte Unix-socket limit on a host whose `TMPDIR` is long — the case a disk-heavy lane
    // creates. So the isolation this asserts is "no durable root escapes the temp root", not "every
    // variable points inside it".
    let environment = tp.env();
    if environment.len() != 6 {
        return fail("isolated environment does not name all six roots");
    }
    for (name, value) in &environment {
        let inside = Path::new(value).starts_with(tp.root());
        let is_runtime = name == "XDG_RUNTIME_DIR";
        if inside == is_runtime {
            return fail(format!(
                "isolated root `{}` is in the wrong place: {}",
                name.to_string_lossy(),
                Path::new(value).display()
            ));
        }
    }
    // ADR-0055 makes the runtime base's privacy a precondition of launching at all, so a harness
    // that left it group- or world-accessible would fail every launch trial before the product
    // path was reached. Asserted here rather than learned again from a `77`.
    let mode = fs::metadata(tp.runtime())
        .map_err(io_failed)?
        .permissions()
        .mode();
    if mode & 0o077 != 0 {
        return fail(format!(
            "the isolated runtime root is not private: mode {:o}",
            mode & 0o777
        ));
    }

    let out = run_viv(
        Path::new("sh"),
        &tp,
        tp.project(),
        &["-c", "printf harness >&2; exit 64"],
    )
    .map_err(io_failed)?;
    check(expect_code(&out, EX_USAGE))?;
    check(expect_stderr_mentions(&out, "harness"))?;
    if expect_code(&out, 0).is_ok() {
        return fail("expect_code accepted a deliberately mismatched status");
    }

    // The isolation hole that would silently invalidate every fail-closed `78`
    // assertion: an ambient VIVARIUM_MANIFEST reaching the child. The check is that
    // every `VIVARIUM_` variable the child sees is one the harness put there on
    // purpose — a bare prefix scan would have to be relaxed the moment the harness
    // needed to inject anything, and relaxing it is how the hole reopens.
    let leak = run_viv(Path::new("sh"), &tp, tp.project(), &["-c", "env"]).map_err(io_failed)?;
    check(expect_injected_vivarium_variables_only(&leak))?;

    // Fixture paths and tokens remain distinct for callers that create two trees in one trial.
    let sibling = TempProject::with_project_name("project").map_err(io_failed)?;
    if tp.project() == sibling.project() || tp.token() == sibling.token() {
        return fail("two fixtures did not receive distinct paths and tokens");
    }

    if !gate().is_well_formed() {
        return fail("gate probe returned a malformed decision");
    }

    check(boot_group_membership_self_check())?;
    check(harness_json_self_check())
}

/// The name of the nextest group that bounds how many trials may boot a guest at once.
const BOOT_GROUP: &str = "boot";

/// The filterset the boot group must carry, derived from the gate table rather than restated.
///
/// Exact names joined by union, because a name pattern fails open: a virtualization trial the
/// pattern happens not to match runs outside the group, unbounded, and nothing reports it.
fn expected_boot_filter() -> String {
    WORKFLOWS
        .iter()
        .filter(|(_, level, _)| matches!(level, GateLevel::Virtualization))
        .map(|(name, _, _)| format!("test(={name})"))
        .collect::<Vec<_>>()
        .join(" + ")
}

/// Assert that `.config/nextest.toml` bounds exactly the trials that boot a guest.
///
/// The bound exists because a booting trial costs a guest kernel, a VMM, one virtiofsd per share,
/// and often a closure realisation, while the product deliberately imposes no fleet count of its
/// own — above the minimum reserve `viv start` warns and lets the user decide (N23, spec/17), and
/// an unattended run has no user. That makes the group nextest's only bound, and a stale
/// membership list would remove it silently, so the list is compared against this harness's own
/// gate table on every run. Checked here rather than in a boot trial because a wrong membership
/// list is precisely what a host with `/dev/kvm` would discover the expensive way.
fn boot_group_membership_self_check() -> Result<(), String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".config")
        .join("nextest.toml");
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "nextest configuration is unreadable at {}: {err}",
            path.display()
        )
    })?;
    let config: toml::Table = toml::from_str(&text)
        .map_err(|err| format!("nextest configuration does not parse: {err}"))?;

    if config
        .get("test-groups")
        .and_then(|groups| groups.get(BOOT_GROUP))
        .and_then(|group| group.get("max-threads"))
        .and_then(toml::Value::as_integer)
        .is_none_or(|max| max < 1)
    {
        return Err(format!(
            "`[test-groups.{BOOT_GROUP}]` does not declare a positive `max-threads`"
        ));
    }

    // Every profile that assigns the group is checked, not just the first: an override added to
    // one profile with a different list would bound that lane differently for no stated reason.
    let mut assignments = 0usize;
    let expected = expected_boot_filter();
    let profiles = config
        .get("profile")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| "nextest configuration declares no profiles".to_owned())?;
    for (profile, settings) in profiles {
        let overrides = settings
            .get("overrides")
            .and_then(toml::Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        for entry in overrides {
            if entry.get("test-group").and_then(toml::Value::as_str) != Some(BOOT_GROUP) {
                continue;
            }
            assignments += 1;
            let filter = entry
                .get("filter")
                .and_then(toml::Value::as_str)
                .unwrap_or_default();
            if filter != expected {
                return Err(format!(
                    "profile `{profile}` bounds the wrong trials. \
                    Expected:\n  {expected}\ngot:\n  {filter}"
                ));
            }
        }
    }
    if assignments == 0 {
        return Err(format!(
            "no profile assigns the `{BOOT_GROUP}` group, so nothing bounds concurrent boots"
        ));
    }
    Ok(())
}

/// The JSON assertions are the harness's only *structural* checks, and every trial that
/// uses them is gated off until a real `viv` exists — so without this they would ship
/// unexecuted. Each case below is one the substring scan they replaced got wrong.
fn harness_json_self_check() -> Result<(), String> {
    let envelope = br#"{"volumes":[{"name":"default","mount":"~"}],"state":"running"}"#;

    if !json::missing_keys(envelope, &["volumes", "state"])?.is_empty() {
        return Err("missing_keys did not find keys that are present".to_owned());
    }
    if json::missing_keys(envelope, &["conflicts"])? != vec!["conflicts".to_owned()] {
        return Err("missing_keys did not report an absent key".to_owned());
    }
    // `name` is nested inside the array, not top-level. The substring scan could not
    // tell the two apart, so an envelope assertion passed on the wrong shape.
    if json::missing_keys(envelope, &["name"])?.is_empty() {
        return Err("missing_keys accepted a nested key as top-level".to_owned());
    }
    if !json::array_items_missing(envelope, "volumes", &["name", "mount"])?.is_empty() {
        return Err("array_items_missing did not find fields that are present".to_owned());
    }
    if json::array_items_missing(envelope, "volumes", &["orphan"])?
        != vec!["volumes[0].orphan".to_owned()]
    {
        return Err("array_items_missing did not report an absent field".to_owned());
    }
    if json::array_items_missing(envelope, "state", &["name"]).is_ok() {
        return Err("array_items_missing accepted a non-array under the key".to_owned());
    }

    harness_nested_json_self_check()
}

/// The specified envelopes nest — `config eval` puts the merged config under `config`,
/// `config sources` keys `values` by option path — so the path- and map-aware assertions
/// need the same self-check as the top-level one. Without them a trial would have to
/// demand nested field names at the root, which a conforming implementation never emits.
fn harness_nested_json_self_check() -> Result<(), String> {
    let nested = br#"{
        "config": { "sandbox": { "egress": { "mode": "allowlist", "allow": [] } } },
        "values": {
            "resources.mem_mib": { "effective": null, "winner": null, "contributors": [] }
        },
        "conflicts": [ { "kind": "tie", "key": "resources.mem_mib", "layers": ["a", "b"] } ]
    }"#;

    if !json::missing_at_path(nested, "config.sandbox.egress", &["mode", "allow"])?.is_empty() {
        return Err("missing_at_path did not find nested fields that are present".to_owned());
    }
    if json::missing_at_path(nested, "config.sandbox.egress", &["proxy"])?
        != vec!["config.sandbox.egress.proxy".to_owned()]
    {
        return Err("missing_at_path did not report an absent nested field".to_owned());
    }
    if json::missing_at_path(nested, "config.absent", &["mode"]).is_ok() {
        return Err("missing_at_path accepted a path that does not exist".to_owned());
    }
    // The regression this whole helper exists to prevent: nested names are not root names.
    if json::missing_keys(nested, &["mode"])?.is_empty() {
        return Err("missing_keys accepted a deeply nested key as top-level".to_owned());
    }

    if !json::map_entries_missing(nested, "values", &["effective", "winner", "contributors"])?
        .is_empty()
    {
        return Err("map_entries_missing did not find member fields that are present".to_owned());
    }
    if json::map_entries_missing(nested, "values", &["priority"])?
        != vec!["values[\"resources.mem_mib\"].priority".to_owned()]
    {
        return Err("map_entries_missing did not report an absent member field".to_owned());
    }
    if json::map_entries_missing(nested, "conflicts", &["kind"]).is_ok() {
        return Err("map_entries_missing accepted an array under the key".to_owned());
    }

    if json::array_len(nested, "values").is_ok() {
        return Err("array_len accepted a non-array under the key".to_owned());
    }
    // `"conflicts": []` is the shape a non-conforming defect report would emit, so the
    // zero case has to be distinguishable from "the key is there".
    if json::array_len(br#"{"conflicts":[]}"#, "conflicts")? != 0 {
        return Err("array_len did not report an empty array as empty".to_owned());
    }
    Ok(())
}

// Guide: docs/guides/first-time-workspace-boot.md
fn workflow_01_usage() -> Result<(), Failed> {
    let tp = TempProject::new().map_err(io_failed)?;
    arrange_manifest(&tp, "rust-web", "", "")?;
    let before = snapshot_tree(tp.project()).map_err(io_failed)?;

    let selection = viv(&tp, &["config", "--json"])?;
    check(expect_derived_manifest_visible(&selection, "rust-web"))?;
    check(expect_tree_unchanged(tp.project(), &before))?;
    check(expect_code(
        &viv(&tp, &["start", "--rebuild", "--no-rebuild"])?,
        EX_USAGE,
    ))?;

    let unbound = TempProject::new().map_err(io_failed)?;
    check(expect_code(&viv(&unbound, &["shell"])?, EX_CONFIG))?;
    check(expect_code(&viv(&tp, &["shell", "--unknown"])?, EX_USAGE))
}

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

// Guide: docs/guides/clean-repo-derived-resolution.md
fn workflow_02_index() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("API").map_err(io_failed)?;
    arrange_manifest(&tp, "clean-registry", "", "")?;
    let before = snapshot_tree(tp.project()).map_err(io_failed)?;

    // A leftover authored registry is inert: the derived index has a new cache-root name and
    // neither reads, rewrites, nor deletes the old state-root file.
    let old_registry = tp.state().join("vivarium").join("registry.toml");
    write_file(&old_registry, "this is deliberately not registry TOML\n").map_err(io_failed)?;
    let old_bytes = fs::read(&old_registry).map_err(io_failed)?;

    let first = viv(&tp, &["config", "--json"])?;
    check(expect_derived_manifest_visible(&first, "clean-registry"))?;
    check(expect_tree_unchanged(tp.project(), &before))?;
    if fs::read(&old_registry).map_err(io_failed)? != old_bytes {
        return fail("the derived resolver touched the leftover authored registry");
    }

    let index = tp.cache().join("vivarium").join("workspace-index.json");
    if !index.is_file() {
        return fail("the first derived resolution did not publish its cache");
    }
    fs::remove_file(&index).map_err(io_failed)?;
    let rebuilt = viv(&tp, &["config", "--json"])?;
    check(expect_derived_manifest_visible(&rebuilt, "clean-registry"))?;
    if !index.is_file() || first.stdout != rebuilt.stdout {
        return fail("deleting the derived index changed the selected sandbox");
    }
    let prior_index = fs::read(&index).map_err(io_failed)?;
    arrange_manifest(
        &tp,
        "clean-registry",
        "\n[env]\nCACHE_GENERATION = \"two\"\n",
        "",
    )?;
    check(expect_derived_manifest_visible(
        &viv(&tp, &["config", "--json"])?,
        "clean-registry",
    ))?;
    if fs::read(&index).map_err(io_failed)? == prior_index {
        return fail("changing a manifest did not invalidate the derived index");
    }

    // A mount is never an ownership candidate.
    let mounted = tp.root().join("mount-only");
    let elsewhere = tp.root().join("elsewhere");
    fs::create_dir_all(&mounted).map_err(io_failed)?;
    fs::create_dir_all(&elsewhere).map_err(io_failed)?;
    arrange_manifest(
        &tp,
        "mount-only",
        &format!(
            "\n[[workspaces]]\nsource = '{}'\n\n[[mounts]]\nsource = '{}'\ntarget = '/mnt/only'\n",
            elsewhere.display(),
            mounted.display()
        ),
        "",
    )?;
    check(expect_code(
        &viv_at(&tp, &mounted, &["config", "--json"])?,
        EX_CONFIG,
    ))?;

    // Two explicit owners fail closed and name both claimants.
    for name in ["owner-one", "owner-two"] {
        arrange_manifest(
            &tp,
            name,
            &format!("\n[[workspaces]]\nsource = '{}'\n", mounted.display()),
            "",
        )?;
    }
    let ambiguous = viv_at(&tp, &mounted, &["config", "--json"])?;
    check(expect_code(&ambiguous, EX_CONFIG))?;
    check(expect_stderr_mentions(&ambiguous, "owner-one"))?;
    check(expect_stderr_mentions(&ambiguous, "owner-two"))
}

/// Guide: docs/guides/team-shared-personal-overrides.md
///
/// The workflow is "a team shares config, each person adjusts it" — so the fixture has
/// to be **one shared piece and two different personal manifests**. Both manifests name
/// the same piece and neither edits it, which is the whole point: the manifest is the
/// personal layer (spec/07, ADR-0040), so a per-user value never touches the shared
/// artifact. A single manifest listing a "team" and a "personal" piece would verify only
/// that priority works, not that anything stays shareable.
fn workflow_03() -> Result<(), Failed> {
    let tp = TempProject::new().map_err(io_failed)?;
    // The shared piece proposes with `mkDefault` so each manifest's normal-priority
    // leaf outranks it — the convention that keeps independently-authored pieces
    // adoptable together (spec/04).
    write_piece(
        &tp,
        "team",
        r#"{ lib, ... }: {
    vivarium.env.WF3_TOOLCHAIN = lib.mkDefault "team-proposed";
    vivarium.mounts = [
        {
            source = "\${HOME}/.config/team";
            target = "~/.config/team";
            readonly = true;
        }
    ];
}
"#,
    )?;
    let shared_piece = tp.config().join("vivarium").join("pieces").join("team.nix");
    let shared_before = fs::read(&shared_piece).map_err(io_failed)?;

    // Two people, same piece, different manifests. Every value appears nowhere else in
    // the fixture so no assertion can be satisfied by an incidental substring.
    arrange_manifest(
        &tp,
        "ana-api",
        "pieces = [ \"team\" ]\n\n[env]\nWF3_TOOLCHAIN = \"ana-decides\"\n",
        "\n[resources]\nmem_mib = 4096\n",
    )?;
    arrange_manifest(
        &tp,
        "bruno-api",
        "pieces = [ \"team\" ]\n\n[env]\nWF3_TOOLCHAIN = \"bruno-decides\"\n",
        "\n[resources]\nmem_mib = 8192\n",
    )?;

    // Both manifests are defined, and both adopt the identical shared piece.
    let library = viv(&tp, &["manifest", "list", "--json"])?;
    check(expect_code(&library, 0))?;
    check(expect_json_array_items(
        &library,
        "manifests",
        &["name", "path", "image", "pieces"],
    ))?;
    check(expect_stdout_mentions(&library, "ana-api"))?;
    check(expect_stdout_mentions(&library, "bruno-api"))?;

    let ana = viv_with_env(
        &tp,
        &["config", "eval", "--json"],
        &[("VIVARIUM_MANIFEST", "ana-api")],
    )?;
    check(expect_code(&ana, 0))?;
    check(expect_json_keys(
        &ana,
        &["manifest", "image", "pieces", "config"],
    ))?;
    // The merged view carries this user's decision and neither the piece's proposal
    // nor the other user's value.
    check(expect_stdout_mentions(&ana, "ana-decides"))?;
    check(expect_stdout_lacks(&ana, "team-proposed"))?;
    check(expect_stdout_lacks(&ana, "bruno-decides"))?;
    // Host-side variables in the shared piece's mount sources stay unexpanded through
    // evaluation — which is what keeps that piece portable between the two of them.
    check(expect_stdout_mentions(&ana, "${HOME}"))?;

    let sources = viv_with_env(
        &tp,
        &["config", "sources", "--json"],
        &[("VIVARIUM_MANIFEST", "ana-api")],
    )?;
    check(expect_code(&sources, 0))?;
    check(expect_json_keys(
        &sources,
        &["manifest", "image", "pieces", "values", "conflicts"],
    ))?;
    // `effective`, `winner`, and `contributors` belong to each *entry* of `values`, not
    // to the envelope root (spec/01). Asserting them top-level would fail against a
    // conforming implementation for the very reason it is conforming.
    check(expect_json_map_entries(
        &sources,
        "values",
        &["effective", "winner", "contributors"],
    ))?;
    // Provenance is the mirror image: it must carry the shadowed contributor that
    // `config eval` dropped.
    check(expect_stdout_mentions(&sources, "ana-decides"))?;
    check(expect_stdout_mentions(&sources, "team-proposed"))?;

    // Selecting the other person's manifest for this invocation changes the result without any
    // edit to the shared artifact.
    let bruno = viv_with_env(
        &tp,
        &["config", "eval", "--json"],
        &[("VIVARIUM_MANIFEST", "bruno-api")],
    )?;
    check(expect_code(&bruno, 0))?;
    check(expect_stdout_mentions(&bruno, "bruno-decides"))?;
    check(expect_stdout_lacks(&bruno, "ana-decides"))?;
    check(expect_stdout_lacks(&bruno, "team-proposed"))?;
    if fs::read(&shared_piece).map_err(io_failed)? != shared_before {
        return fail("the shared piece changed while evaluating personal manifests");
    }

    workflow_03_literal_path(&tp)?;
    workflow_03_tie(&tp)
}

/// A literal personal path in a **shared** layer is an N11 violation. Under ADR-0042 the
/// two config verbs diverge deliberately, and asserting only the failing half would miss
/// the point: `config eval` refuses with `65`, while `config sources` — the command a
/// user reaches for *because* eval refused — still renders the defect and exits `0`.
fn workflow_03_literal_path(tp: &TempProject) -> Result<(), Failed> {
    write_piece(
        tp,
        "team",
        r#"{ ... }: {
    vivarium.mounts = [
        {
            source = "/home/ana/.config/team";
            target = "~/.config/team";
            readonly = true;
        }
    ];
}
"#,
    )?;
    let invalid = viv_with_env(
        tp,
        &["config", "eval", "--json"],
        &[("VIVARIUM_MANIFEST", "ana-api")],
    )?;
    check(expect_code(&invalid, EX_DATAERR))?;
    check(expect_stderr_mentions(&invalid, "/home/ana"))?;

    let still_readable = viv_with_env(
        tp,
        &["config", "sources", "--json"],
        &[("VIVARIUM_MANIFEST", "ana-api")],
    )?;
    check(expect_code(&still_readable, 0))?;
    // The defect must be *encoded*, not merely alluded to: `conflicts` carries a record
    // naming the class, the key, and the declaring layers (spec/01). Checking only that
    // the key exists would be satisfied by `"conflicts": []` — an implementation that
    // renders the path in prose and reports no machine-readable defect at all.
    check(expect_json_array_nonempty(
        &still_readable,
        "conflicts",
        &["kind", "key", "layers"],
    ))?;
    check(expect_stdout_mentions(&still_readable, "literal-path"))?;
    check(expect_stdout_mentions(&still_readable, "/home/ana"))?;

    // ADR-0108's ownership half, end to end. What it asserts changed with ADR-0110 and the
    // change is worth pinning rather than deleting.
    //
    // The rule used to be enforced: `vivarium.workspaces` was an option a shared layer could
    // write, so `src/config/merged.rs` refused one at `65` naming the offending layer. There is
    // no such option now — a workspace compiles to a `vivarium.mounts` row that `viv` writes from
    // manifest text — so the guarantee is structural. A piece reaching for the old spelling
    // contributes nothing, and the provenance view says so by showing the manifest's own trees
    // and no others.
    //
    // It also says so quietly, which is the cost: `nix/vivarium-report.nix` evaluates each layer
    // with `_module.check = false` so an ordinary NixOS layer is not fatal here, and an undeclared
    // `vivarium.*` is ignored along with it. Recorded as `Q-034`; this trial pins the behaviour so
    // the day it gains a diagnostic, this is what moves.
    write_piece(
        tp,
        "team",
        r#"{ ... }: {
    vivarium.workspaces = [ { source = "\${HOME}/team-tree"; } ];
}
"#,
    )?;
    let owned_by_a_piece = viv_with_env(
        tp,
        &["config", "eval", "--json"],
        &[("VIVARIUM_MANIFEST", "ana-api")],
    )?;
    check(expect_code(&owned_by_a_piece, 0))?;
    // The manifest's own tree, and nothing the piece asked for.
    check(expect_stdout_mentions(
        &owned_by_a_piece,
        "\"workspaces\":[{",
    ))?;
    check(expect_stdout_lacks(&owned_by_a_piece, "team-tree"))
}

/// The equal-priority tie. Under ADR-0040's convention a shared piece proposes with
/// `mkDefault`, so the case that actually collides is **two pieces at normal priority** —
/// not a piece against the manifest leaf, which the leaf simply wins. Under ADR-0042 the
/// collision is a content defect: `config eval` returns `65`, and `config sources` reports
/// it at exit `0` with the `[tie]` marker on stderr.
fn workflow_03_tie(tp: &TempProject) -> Result<(), Failed> {
    arrange_manifest(tp, "tie-demo", "pieces = [ \"tie-a\", \"tie-b\" ]\n", "")?;
    write_piece(
        tp,
        "tie-a",
        "{ ... }: { vivarium.resources.mem_mib = 4096; }\n",
    )?;
    write_piece(
        tp,
        "tie-b",
        "{ ... }: { vivarium.resources.mem_mib = 8192; }\n",
    )?;

    check(expect_code(
        &viv_with_env(
            tp,
            &["config", "eval", "--json"],
            &[("VIVARIUM_MANIFEST", "tie-demo")],
        )?,
        EX_DATAERR,
    ))?;

    let tie = viv_with_env(
        tp,
        &["config", "sources", "--json"],
        &[("VIVARIUM_MANIFEST", "tie-demo")],
    )?;
    check(expect_code(&tie, 0))?;
    check(expect_json_array_nonempty(
        &tie,
        "conflicts",
        &["kind", "key", "layers"],
    ))?;
    check(expect_stdout_mentions(&tie, "\"tie\""))?;
    check(expect_stderr_mentions(&tie, "[tie]"))?;
    // The marker is stderr-only so `--json 2>/dev/null | jq` stays clean.
    check(expect_stdout_lacks(&tie, "[tie]"))
}

// Guide: docs/guides/inspect-before-run.md
fn workflow_04_usage() -> Result<(), Failed> {
    let tp = TempProject::new().map_err(io_failed)?;
    arrange_manifest(
        &tp,
        "inspect-dev",
        "pieces = [ \"inspect\" ]\n",
        "\n[resources]\nmem_mib = 2048\nvcpu = 2\n",
    )?;
    write_piece(&tp, "inspect", "{ ... }: { }\n")?;

    let show = viv(&tp, &["manifest", "show", "inspect-dev", "--json"])?;
    check(expect_code(&show, 0))?;
    check(expect_json_keys(
        &show,
        &[
            "manifest",
            "path",
            "image",
            "pieces",
            "resources",
            "egress",
            "extends",
        ],
    ))?;
    // The knobs are nested under their own objects (spec/01), so they are addressed by
    // path rather than demanded at the root.
    check(expect_json_fields_at(
        &show,
        "resources",
        &["mem_mib", "vcpu"],
    ))?;
    check(expect_json_fields_at(&show, "egress", &["mode", "allow"]))?;
    check(expect_code(
        &viv(&tp, &["manifest", "show", "missing"])?,
        EX_CONFIG,
    ))?;
    check(expect_code(&viv(&tp, &["manifest", "show"])?, EX_USAGE))?;
    check(expect_code(
        &viv(&tp, &["manifest", "show", "one", "two"])?,
        EX_USAGE,
    ))?;

    let list = viv(&tp, &["manifest", "list", "--json"])?;
    check(expect_code(&list, 0))?;
    check(expect_json_keys(&list, &["manifests"]))?;
    check(expect_json_array_items(
        &list,
        "manifests",
        &["name", "path", "image", "pieces"],
    ))?;
    let empty = TempProject::new().map_err(io_failed)?;
    let empty_list = viv(&empty, &["manifest", "list", "--json"])?;
    check(expect_code(&empty_list, 0))?;
    check(expect_json_keys(&empty_list, &["manifests"]))?;

    // Explicit workspace ownership makes the manifest available without another write.
    check(expect_derived_manifest_visible(
        &viv(&tp, &["config", "--json"])?,
        "inspect-dev",
    ))
}

// Guide: docs/guides/inspect-before-run.md
fn workflow_04_eval() -> Result<(), Failed> {
    let tp = TempProject::new().map_err(io_failed)?;
    arrange_manifest(
        &tp,
        "inspect-dev",
        "pieces = [ \"inspect\" ]\n",
        "\n[resources]\nmem_mib = 2048\nvcpu = 2\n",
    )?;
    write_piece(&tp, "inspect", "{ ... }: { }\n")?;

    let evaluated = viv(&tp, &["config", "eval", "--json"])?;
    check(expect_code(&evaluated, 0))?;
    check(expect_json_keys(
        &evaluated,
        &["manifest", "image", "pieces", "config"],
    ))?;
    let sources = viv(&tp, &["config", "sources", "--json"])?;
    check(expect_code(&sources, 0))?;
    check(expect_json_keys(
        &sources,
        &["manifest", "image", "pieces", "values", "conflicts"],
    ))?;
    // Read-only diagnostics never mutate project or VM state.
    check(expect_no_volume_images(&tp))?;

    // One volume name bound to two mountpoints is spec/14's cited instance of an
    // irreconcilable merge, and the only path to `65`.
    let conflict_tail = concat!(
        "\n[[volumes]]\nname = \"cache\"\nmount = \"/one\"\n",
        "\n[[volumes]]\nname = \"cache\"\nmount = \"/two\"\n"
    );
    arrange_manifest(&tp, "conflict", "", conflict_tail)?;
    check(expect_code(
        &viv_with_env(
            &tp,
            &["config", "eval", "--json"],
            &[("VIVARIUM_MANIFEST", "conflict")],
        )?,
        EX_DATAERR,
    ))
}

// Guide: docs/guides/restrict-egress-allowlist.md
fn workflow_05_config() -> Result<(), Failed> {
    let tp = arrange_egress_fixture()?;

    let evaluated = viv(&tp, &["config", "eval", "--json"])?;
    check(expect_code(&evaluated, 0))?;
    check(expect_json_keys(
        &evaluated,
        &["manifest", "image", "pieces", "config"],
    ))?;
    // `config eval --json` nests the merged config under `config` (spec/01), so the
    // egress knobs are reached by path rather than expected at the envelope root.
    check(expect_json_fields_at(
        &evaluated,
        "config.sandbox.egress",
        &["mode", "allow"],
    ))?;
    // The piece's mkForce beats the image's mkDefault, so the merged view carries the
    // winner and not the shadowed default.
    check(expect_stdout_mentions(&evaluated, "allowlist"))?;
    check(expect_stdout_lacks(&evaluated, "\"open\""))?;
    // Allowlist entries concatenate across layers rather than replacing one another,
    // so both the manifest's and the piece's host must survive the merge.
    check(expect_stdout_mentions(
        &evaluated,
        support::egress::ALLOWED_NAME,
    ))?;
    check(expect_stdout_mentions(
        &evaluated,
        support::egress::GHOST_NAME,
    ))?;

    let sources = viv(&tp, &["config", "sources", "--json"])?;
    check(expect_code(&sources, 0))?;
    check(expect_stdout_mentions(&sources, "egress-restriction"))?;
    // The provenance view must still show the image layer that `config eval` dropped.
    check(expect_stdout_mentions(&sources, "\"open\""))
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

fn arrange_egress_fixture() -> Result<TempProject, Failed> {
    let tp = TempProject::new().map_err(io_failed)?;
    // The image supplies the shadowed default; only the piece forces the allowlist, so
    // the precedence under test is genuinely arranged rather than pre-decided by the
    // manifest. Each layer contributes a distinct host to make concatenation
    // observable — the manifest the reachable endpoint's name, the piece the name
    // whose upstream answer is NXDOMAIN.
    arrange_manifest_with_image(
        &tp,
        "restricted",
        "pieces = [ \"egress-restriction\" ]\n",
        &format!(
            "\n[egress]\nallow = [ \"{}\" ]\n",
            support::egress::ALLOWED_NAME
        ),
        "{ lib, ... }: { sandbox.egress.mode = lib.mkDefault \"open\"; }\n",
    )?;
    write_piece(
        &tp,
        "egress-restriction",
        &format!(
            "{{ lib, ... }}: {{\n    sandbox.egress.mode = lib.mkForce \"allowlist\";\n    \
            sandbox.egress.allow = [ \"{}\" ];\n}}\n",
            support::egress::GHOST_NAME
        ),
    )?;
    Ok(tp)
}

// Guide: docs/guides/run-agent-command.md
fn workflow_06_usage() -> Result<(), Failed> {
    let unbound = TempProject::new().map_err(io_failed)?;
    check(expect_code(
        &viv(&unbound, &["exec", "--", "true"])?,
        EX_CONFIG,
    ))?;
    check(expect_code(&viv(&unbound, &["exec"])?, EX_USAGE))?;
    check(expect_code(&viv(&unbound, &["exec", "--"])?, EX_USAGE))?;
    check(expect_code(
        &viv(&unbound, &["exec", "-t", "--no-tty", "--", "true"])?,
        EX_USAGE,
    ))?;
    // `-t` when host stdin is not a terminal.
    check(expect_code(
        &viv(&unbound, &["exec", "-t", "--", "true"])?,
        EX_USAGE,
    ))
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
    let mut command = std::process::Command::new(gate().viv());
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

    let mut child = pty_process::blocking::Command::new(gate().viv())
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

// Spec: docs/reference/spec/13-doctor-and-health-checks.md
//
// Gate-safe on purpose: the report's exit code reflects this host's health, which the Cli gate
// does not constrain, so the trial asserts the published shapes — the enumerated catalog, the
// envelope, the skip reasons, the stderr note, bracketed word markers — and never the health.
#[allow(clippy::too_many_lines)]
fn workflow_16_doctor() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("doctor-project").map_err(io_failed)?;

    // `--list` probes nothing, so its exit is `0` on any host, and the descriptor fields are the
    // whole row (spec/13).
    let list = viv(&tp, &["doctor", "--list", "--json"])?;
    check(expect_code(&list, 0))?;
    check(expect_json_array_nonempty(
        &list,
        "checks",
        &["id", "category", "scope", "severity", "title"],
    ))?;
    check(expect_stdout_mentions(&list, "kvm-device-present"))?;

    // Unbound: host probes still report, project probes skip with the fixed reason, network
    // probes skip offline, the one stderr note names the way in — and stdout stays one JSON
    // object so `--json 2>/dev/null | jq` is clean.
    let report = viv(&tp, &["doctor", "--json"])?;
    check(expect_json_keys(
        &report,
        &["status", "checks", "summary", "schema_version"],
    ))?;
    check(expect_json_fields_at(
        &report,
        "summary",
        &[
            "total",
            "passed",
            "warned",
            "failed",
            "skipped",
            "hard_failures",
        ],
    ))?;
    check(expect_stdout_mentions(&report, "no-manifest-bound"))?;
    check(expect_stdout_mentions(&report, "offline-mode"))?;
    check(expect_stderr_mentions(
        &report,
        "no manifest declares this workspace",
    ))?;

    // Retained state whose manifest has gone is surfaced by name and path, never reaped.
    let orphan_state_key = tp.state().join("vivarium/projects/removed-manifest");
    let orphan_data_key = tp.data().join("vivarium/projects/removed-manifest");
    let orphan_state = orphan_state_key.join("default");
    let orphan_data = orphan_data_key.join("default");
    fs::create_dir_all(&orphan_state).map_err(io_failed)?;
    fs::create_dir_all(&orphan_data).map_err(io_failed)?;
    let orphaned = viv(&tp, &["doctor", "--json"])?;
    check(expect_stdout_mentions(&orphaned, "state-manifest-orphans"))?;
    check(expect_stdout_mentions(&orphaned, "removed-manifest"))?;
    check(expect_stdout_mentions(
        &orphaned,
        &orphan_state_key.display().to_string(),
    ))?;
    check(expect_stdout_mentions(
        &orphaned,
        &orphan_data_key.display().to_string(),
    ))?;
    if !orphan_state.is_dir() || !orphan_data.is_dir() {
        return fail("viv doctor deleted retained state while reporting it");
    }

    // The human report wears bracketed word markers — never glyphs — and closes with the summary
    // line naming the exit (spec/13).
    let human = viv(&tp, &["doctor"])?;
    check(expect_stdout_mentions(&human, "[skipped]"))?;
    check(expect_stdout_mentions(&human, "-> exit "))?;

    // ADR-0109's finding is shared with refusing verbs, but doctor remains a report: it names the
    // selected manifest, cwd, and exact declaration without turning the condition into exit 78.
    let elsewhere = tp.root().join("declared-elsewhere");
    fs::create_dir_all(&elsewhere).map_err(io_failed)?;
    arrange_manifest(
        &tp,
        "doctor-ownership",
        &format!("\n[[workspaces]]\nsource = '{}'\n", elsewhere.display()),
        "",
    )?;
    let manifest_path = tp
        .config()
        .join("vivarium")
        .join("manifests")
        .join("doctor-ownership.toml");
    let before = fs::read(&manifest_path).map_err(io_failed)?;
    let refused = viv(&tp, &["config", "--manifest", "doctor-ownership", "--json"])?;
    check(expect_code(&refused, EX_CONFIG))?;
    check(expect_stderr_mentions(
        &refused,
        "manifest.workspace-undeclared-directory",
    ))?;
    check(expect_stderr_mentions(
        &refused,
        manifest_path.to_str().unwrap_or(""),
    ))?;
    check(expect_stderr_mentions(
        &refused,
        tp.project().to_str().unwrap_or(""),
    ))?;
    check(expect_stderr_mentions(&refused, "[[workspaces]]"))?;
    if fs::read(&manifest_path).map_err(io_failed)? != before {
        return fail("the undeclared-workspace refusal modified the selected manifest");
    }
    let ownership = viv_with_env(
        &tp,
        &["doctor", "--json"],
        &[("VIVARIUM_MANIFEST", "doctor-ownership")],
    )?;
    if ownership.status.code() == Some(EX_CONFIG) {
        return fail("doctor refused ADR-0109's ownership finding instead of reporting it");
    }
    check(expect_stdout_mentions(
        &ownership,
        "working-directory-declared",
    ))?;
    check(expect_stdout_mentions(&ownership, "doctor-ownership.toml"))?;
    check(expect_stdout_mentions(
        &ownership,
        tp.project().to_str().unwrap_or(""),
    ))?;
    check(expect_stdout_mentions(&ownership, "[[workspaces]]"))?;

    arrange_manifest(
        &tp,
        "doctor-second-owner",
        &format!("\n[[workspaces]]\nsource = '{}'\n", elsewhere.display()),
        "",
    )?;
    let ambiguous = viv_at(&tp, &elsewhere, &["config", "--json"])?;
    check(expect_code(&ambiguous, EX_CONFIG))?;
    check(expect_stderr_mentions(
        &ambiguous,
        "state.workspace-owner-ambiguous",
    ))?;
    check(expect_stderr_mentions(&ambiguous, "doctor-ownership"))?;
    check(expect_stderr_mentions(&ambiguous, "doctor-second-owner"))?;

    // ADR-0109's second consumer, on the same condition. `config` refused above; `doctor` must
    // report the identical finding and NOT refuse, because a doctor that exited `78` on the
    // condition it exists to explain would be self-defeating. Asserted here rather than trusted:
    // the finding reaches doctor through a path where an ordinary `.ok()` would erase it, and an
    // erased finding renders as `no-manifest-bound`, which says the opposite of what happened.
    let diagnosed = viv_at(&tp, &elsewhere, &["doctor"])?;
    check(expect_code(&diagnosed, 0))?;
    for named in ["doctor-ownership", "doctor-second-owner"] {
        check(expect_stdout_mentions(&diagnosed, named))?;
    }
    check(expect_stdout_mentions(
        &diagnosed,
        "working-directory-declared",
    ))?;
    check(expect_stdout_mentions(&diagnosed, "manifest-resolves"))?;
    // The stderr note too, and not as an afterthought: it is the one line that can contradict
    // everything above it. `project` is `None` here for the opposite of the usual reason — two
    // manifests declare this directory, not none — and a note saying none would tell the user
    // the reverse of what the findings just said.
    check(expect_stderr_mentions(&diagnosed, "more than one manifest"))?;
    Ok(())
}

// Guide: docs/guides/stop-restart-preserving-volumes.md
fn workflow_07_usage() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("volume-project").map_err(io_failed)?;
    arrange_manifest(&tp, "volumes", "", VOLUME_TAIL)?;
    check(expect_code(&viv(&tp, &["volume", "list", "--json"])?, 0))?;
    // `--force` conflicts with a nonzero `--timeout`; `--force --timeout 0` is fine.
    check(expect_code(
        &viv(&tp, &["stop", "--force", "--timeout", "5"])?,
        EX_USAGE,
    ))
}

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

const VOLUME_TAIL: &str = "\n[[volumes]]\nname = \"cache\"\nmount = \"/var/cache/project\"\n";

// Guide: docs/guides/destroy-cold-rebuild.md
fn workflow_08_usage() -> Result<(), Failed> {
    // The manifest name is deliberately unlike the project name so the state key is observable.
    let tp = TempProject::with_project_name("destroy-project").map_err(io_failed)?;
    arrange_manifest(&tp, "teardown-demo", "", "")?;
    check(expect_derived_manifest_visible(
        &viv(&tp, &["config", "--json"])?,
        "teardown-demo",
    ))?;
    check(expect_code(&viv(&tp, &["destroy"])?, EX_USAGE))?;

    // `gc` is a global whole-store sweep: it never requires a selected manifest, so it cannot
    // answer `78` even from an undeclared directory. The sweep itself must not run here — this
    // suite runs from the hooks, and a real collection takes the store's GC lock under every
    // concurrently building trial — so the collector is made unreachable instead: with an empty
    // `PATH` the verb gets exactly as far as spawning `nix-store` and answers `69`, which proves
    // no manifest gate stood before it. The real sweep is `tests/host/generations-check`'s.
    let global = TempProject::new().map_err(io_failed)?;
    let no_tools = global.root().join("no-tools");
    fs::create_dir_all(&no_tools).map_err(io_failed)?;
    let gc = viv_with_env(
        &global,
        &["gc"],
        &[("PATH", no_tools.to_str().unwrap_or_default())],
    )?;
    check(expect_code(&gc, EX_UNAVAILABLE))?;
    check(expect_stderr_mentions(&gc, "gc-unavailable"))
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

/// Slice 019's evaluation tier: the declared-mount defects the merged view can decide refuse
/// `config eval` with `65` and render through `config sources` at `0` (ADR-0042), each named by
/// its `conflicts` kind (spec/01).
fn workflow_17_eval() -> Result<(), Failed> {
    let tp = TempProject::new().map_err(io_failed)?;

    // N24's decidable half: a literal session-directory source, refused in the PERSONAL layer —
    // which is what separates it from N11, where a literal path in one's own manifest is
    // ordinary authorship.
    arrange_manifest(
        &tp,
        "mounts-eval",
        "\n[[mounts]]\nsource = \"/tmp/wf17-shared-tools\"\ntarget = \"/workspaces/tools\"\n",
        "",
    )?;
    let session = viv(&tp, &["config", "eval", "--json"])?;
    check(expect_code(&session, EX_DATAERR))?;
    check(expect_stderr_mentions(&session, "session-path"))?;
    check(expect_stderr_mentions(&session, "/tmp/wf17-shared-tools"))?;
    let sources = viv(&tp, &["config", "sources", "--json"])?;
    check(expect_code(&sources, 0))?;
    check(expect_json_array_nonempty(
        &sources,
        "conflicts",
        &["kind", "key", "layers"],
    ))?;
    check(expect_stdout_mentions(&sources, "session-path"))?;

    // spec/07's sharing rule: a shared piece may reference the host only through the portable
    // set, and the same private variable in the personal manifest is legal.
    write_piece(
        &tp,
        "wf17tool",
        r#"{ ... }: {
    vivarium.mounts = [
        {
            source = "\${WF17_PRIVATE_DIR}/tool";
            target = "/workspaces/tool";
            readonly = false;
        }
    ];
}
"#,
    )?;
    arrange_manifest(&tp, "mounts-eval", "pieces = [ \"wf17tool\" ]\n", "")?;
    let nonportable = viv(&tp, &["config", "eval", "--json"])?;
    check(expect_code(&nonportable, EX_DATAERR))?;
    check(expect_stderr_mentions(
        &nonportable,
        "non-portable-variable",
    ))?;
    arrange_manifest(
        &tp,
        "mounts-eval",
        "\n[[mounts]]\nsource = \"${WF17_PRIVATE_DIR}/tool\"\ntarget = \"/workspaces/tool\"\n",
        "",
    )?;
    check(expect_code(&viv(&tp, &["config", "eval", "--json"])?, 0))?;

    // ADR-0020: declarations concatenate across layers, and duplicate targets fail evaluation.
    write_piece(
        &tp,
        "wf17dup",
        r#"{ ... }: {
    vivarium.mounts = [
        {
            source = "\${HOME}/.config/wf17";
            target = "~/.config/wf17";
            readonly = false;
        }
    ];
}
"#,
    )?;
    arrange_manifest(
        &tp,
        "mounts-eval",
        concat!(
            "pieces = [ \"wf17dup\" ]\n\n[[mounts]]\n",
            "source = \"${HOME}/wf17-other\"\ntarget = \"~/.config/wf17\"\n"
        ),
        "",
    )?;
    let duplicate = viv(&tp, &["config", "eval", "--json"])?;
    check(expect_code(&duplicate, EX_DATAERR))?;
    check(expect_stderr_mentions(&duplicate, "~/.config/wf17"))
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
        gate().viv(),
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
        gate().viv(),
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
    let mut child = pty_process::blocking::Command::new(gate().viv())
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

/// The declared channel is visible on both config surfaces: `config eval` renders the effective
/// list and `config sources` names the layer that opted the user in — which is what makes a
/// piece-declared channel legible rather than a surprise (spec/07, slice 031).
fn workflow_23_config() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("agent-channel-config").map_err(io_failed)?;
    write_piece(
        &tp,
        "credentials-ssh",
        "{ ... }: { vivarium.credentials.agents = [ \"ssh\" ]; }\n",
    )?;
    arrange_manifest(
        &tp,
        "agent-channel-config",
        "pieces = [ \"credentials-ssh\" ]\n",
        "",
    )?;

    let eval = viv(&tp, &["config", "eval", "--json"])?;
    check(expect_code(&eval, 0))?;
    check(expect_json_fields_at(
        &eval,
        "config.credentials",
        &["agents"],
    ))?;
    check(expect_stdout_mentions(&eval, "\"ssh\""))?;

    let human = viv(&tp, &["config", "eval"])?;
    check(expect_code(&human, 0))?;
    check(expect_stdout_mentions(&human, "[credentials]"))?;
    check(expect_stdout_mentions(&human, "agents"))?;

    let sources = viv(&tp, &["config", "sources", "--json"])?;
    check(expect_code(&sources, 0))?;
    check(expect_json_map_entries(
        &sources,
        "values",
        &["effective", "winner", "contributors"],
    ))?;
    check(expect_stdout_mentions(&sources, "credentials.agents"))?;
    check(expect_stdout_mentions(&sources, "credentials-ssh"))?;

    // The acceptance's doctor clause: with a channel declared and the harness environment
    // holding no agent, `agent-source-usable` is a real finding naming the fault — never the
    // `not-applicable` skip — and a soft warn still exits `0`.
    let doctor = viv(&tp, &["doctor", "--json"])?;
    check(expect_code(&doctor, 0))?;
    check(expect_stdout_mentions(&doctor, "agent-source-usable"))?;
    check(expect_stdout_mentions(&doctor, "$SSH_AUTH_SOCK"))?;
    check(expect_stdout_lacks(
        &doctor,
        "no composed layer declares an agent channel",
    ))?;

    // The same clause through the manifest's own bounded module: the declaration moves into a
    // directory-form manifest's `extends` and the piece adoption is dropped, so only the extends
    // text can put the channel in view. Extends is a composed layer evaluation honors, and a
    // probe reading pieces alone would answer `not-applicable` here.
    let manifests = tp.config().join("vivarium").join("manifests");
    fs::remove_file(manifests.join("agent-channel-config.toml")).map_err(io_failed)?;
    let directory = manifests.join("agent-channel-config");
    write_file(
        &directory.join("default.toml"),
        &format!(
            "image = \"minimal\"\nextends = \"agents.nix\"\n\n[[workspaces]]\nsource = '{}'\n",
            tp.project().display()
        ),
    )
    .map_err(io_failed)?;
    write_file(
        &directory.join("agents.nix"),
        "{ ... }: { vivarium.credentials.agents = [ \"ssh\" ]; }\n",
    )
    .map_err(io_failed)?;
    let extends_doctor = viv(&tp, &["doctor", "--json"])?;
    check(expect_code(&extends_doctor, 0))?;
    check(expect_stdout_mentions(
        &extends_doctor,
        "agent-source-usable",
    ))?;
    check(expect_stdout_mentions(&extends_doctor, "$SSH_AUTH_SOCK"))?;
    check(expect_stdout_lacks(
        &extends_doctor,
        "no composed layer declares an agent channel",
    ))
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
    let status = run_viv(gate().viv(), tp, tp.project(), &["status", "--json"])
        .map_err(|error| error.to_string())?;
    expect_code(&status, 0)?;
    expect_json_string(&status, "state", "built")
}
/// Slice 018, the update surface that needs no Nix: an unbound project refuses at `78`; an
/// unknown flag and an unknown input name are `64`, the name decided by vivarium because Nix
/// answers it with a warning and a successful no-op; a team override lock refuses at `78`
/// naming both files, emits no JSON, and writes nothing; and the verb is published.
fn workflow_18_update_usage() -> Result<(), Failed> {
    let unbound = TempProject::new().map_err(io_failed)?;
    check(expect_code(&viv(&unbound, &["update"])?, EX_CONFIG))?;

    let tp = TempProject::with_project_name("update-usage").map_err(io_failed)?;
    arrange_manifest(&tp, "update-usage", "", "")?;
    check(expect_code(&viv(&tp, &["update", "--bogus"])?, EX_USAGE))?;
    let unknown = viv(&tp, &["update", "no-such-input"])?;
    check(expect_code(&unknown, EX_USAGE))?;
    check(expect_stderr_mentions(&unknown, "no-such-input"))?;

    // The override refusal: a read-only team pin beside a directory-form manifest wins, and
    // the update refuses whole — both files named, no JSON, and the shadowed per-target
    // lock not written (ADR-0062, spec/02).
    let ov = TempProject::with_project_name("update-override").map_err(io_failed)?;
    let manifest_dir = ov
        .config()
        .join("vivarium")
        .join("manifests")
        .join("update-override");
    write_file(
        &manifest_dir.join("default.toml"),
        &format!(
            "image = \"minimal\"\n\n[[workspaces]]\nsource = '{}'\n",
            ov.project().display()
        ),
    )
    .map_err(io_failed)?;
    write_file(
        &ov.config()
            .join("vivarium")
            .join("images")
            .join("minimal.nix"),
        "{ ... }: { }\n",
    )
    .map_err(io_failed)?;
    write_file(
        &manifest_dir.join("flake.lock"),
        "{\"nodes\":{\"root\":{}},\"root\":\"root\",\"version\":7}\n",
    )
    .map_err(io_failed)?;
    let refused = viv(&ov, &["update", "--json"])?;
    check(expect_code(&refused, EX_CONFIG))?;
    check(expect_stderr_mentions(&refused, "override-in-force"))?;
    check(expect_stderr_mentions(&refused, "flake.lock"))?;
    check(expect_stderr_mentions(&refused, "projects"))?;
    if !refused.stdout.is_empty() {
        return Err(Failed::from(
            "a refused update must emit no JSON at all (spec/01)",
        ));
    }
    let owned = ov
        .data()
        .join("vivarium")
        .join("projects")
        .join("update-override")
        .join("default")
        .join("flake.lock");
    if owned.exists() {
        return Err(Failed::from(
            "a refused update wrote the shadowed per-target lock",
        ));
    }

    // The pre-lock precedence, through the command boundary: with the per-target lock held
    // by another process, an unknown name still answers `64` — the usage refusal precedes
    // the lock and every write — while a well-formed invocation meets the contention at
    // `75`. The lock file is the same flock `TargetLock` takes.
    let lock_dir = tp
        .runtime()
        .join("vivarium")
        .join("update-usage")
        .join("default");
    std::fs::create_dir_all(&lock_dir).map_err(io_failed)?;
    std::fs::set_permissions(&lock_dir, std::fs::Permissions::from_mode(0o700))
        .map_err(io_failed)?;
    let holder = std::fs::File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(lock_dir.join("lock"))
        .map_err(io_failed)?;
    holder.lock().map_err(io_failed)?;
    let contended_typo = viv(&tp, &["update", "no-such-input"])?;
    check(expect_code(&contended_typo, EX_USAGE))?;
    let contended_valid = viv(&tp, &["update"])?;
    check(expect_code(&contended_valid, EX_TEMPFAIL))?;
    drop(holder);
    let owned = tp
        .data()
        .join("vivarium")
        .join("projects")
        .join("update-usage")
        .join("default")
        .join("flake.lock");
    if owned.exists() {
        return Err(Failed::from(
            "a refused or contended update wrote the owned lock",
        ));
    }

    let help = viv(&tp, &["--help"])?;
    check(expect_code(&help, 0))?;
    check(expect_stdout_mentions(&help, "update"))
}

/// Slice 018, the pin-moving core under the `ConfigEval` gate: a first `viv update` creates
/// the lock and reports `before: null`; the base input renders and its hashless row reports
/// no pin; a repeat run moves nothing and still reports the requested row; rows never carry
/// a `changed: true` the fixtures did not arrange; and no staged residue survives beside the
/// owned lock.
fn workflow_18_update_pin() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("update-pin").map_err(io_failed)?;
    let images = tp.config().join("vivarium").join("images").join("my-base");
    write_file(&images.join("default.nix"), "{ ... }: { }\n").map_err(io_failed)?;
    // The base declares `nixpkgs` unconditionally, pointing at a local leaf flake, so the
    // second update below always exercises the follows translation: `viv update nixpkgs`
    // is run against the base that owns the node, and the reported row must still carry
    // the name the user passed (spec/01). A leaf suffices — `viv update` locks references
    // and never evaluates their outputs — and a local `path:` needs no network.
    let leaf = tp.root().join("fake-nixpkgs");
    write_file(&leaf.join("flake.nix"), "{ outputs = { self }: { }; }\n").map_err(io_failed)?;
    write_file(
        &images.join("flake.nix"),
        &format!(
            "{{ inputs.nixpkgs.url = \"path:{}\";\n  \
            outputs = {{ self, nixpkgs }}: {{ }}; }}\n",
            leaf.display()
        ),
    )
    .map_err(io_failed)?;
    write_file(
        &tp.config()
            .join("vivarium")
            .join("manifests")
            .join("update-pin.toml"),
        &format!(
            "image = \"my-base\"\n\n[[workspaces]]\nsource = '{}'\n",
            tp.project().display()
        ),
    )
    .map_err(io_failed)?;

    let first = viv(&tp, &["update", "--json"])?;
    check(expect_code(&first, 0))?;
    check(expect_json_keys(&first, &["manifest", "lock", "inputs"]))?;
    check(expect_stdout_mentions(&first, "my-base"))?;
    check(expect_stdout_mentions(&first, "nixpkgs"))?;
    check(expect_stdout_mentions(&first, "\"before\":null"))?;
    check(expect_stderr_mentions(&first, "created the pin"))?;
    let owned = tp
        .data()
        .join("vivarium")
        .join("projects")
        .join("update-pin")
        .join("default")
        .join("flake.lock");
    if !owned.is_file() {
        return Err(Failed::from(
            "the first update did not create the owned lock",
        ));
    }

    // The repeat: nothing upstream moved — the baselines are pinned `path:` references —
    // so every row is `changed: false`, the requested row included, and the run is `0`.
    let second = viv(&tp, &["update", "nixpkgs", "--json"])?;
    check(expect_code(&second, 0))?;
    check(expect_stdout_mentions(&second, "\"changed\":false"))?;
    check(expect_stdout_lacks(&second, "\"changed\":true"))?;
    // The requested row carries the user's own string, translation or not (spec/01).
    check(expect_stdout_mentions(&second, "\"name\":\"nixpkgs\""))?;

    // No staged residue beside the owned lock: the update's temp file is consumed by the
    // rename or removed on failure, never left for the next reader to misread.
    let residue: Vec<_> = std::fs::read_dir(owned.parent().ok_or("owned lock parent")?)
        .map_err(io_failed)?
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name != "flake.lock")
        .collect();
    if !residue.is_empty() {
        return Err(Failed::from(format!(
            "staged residue beside the owned lock: {residue:?}"
        )));
    }
    Ok(())
}

/// Slice 032, the grammar surface: malformed retention arguments answer `64`, an unbound
/// project `78`, and a never-built project lists the empty envelope at `0` (spec/01, spec/14).
fn workflow_24_usage() -> Result<(), Failed> {
    let unbound = TempProject::new().map_err(io_failed)?;
    check(expect_code(
        &viv(&unbound, &["generations", "list"])?,
        EX_CONFIG,
    ))?;

    let tp = TempProject::with_project_name("generations-usage").map_err(io_failed)?;
    arrange_manifest(&tp, "generations-usage", "", "")?;
    for rejected in [
        vec!["generations"],
        vec!["generations", "prune"],
        vec!["generations", "prune", "--keep", "1", "--older-than", "1d"],
        vec!["generations", "prune", "--older-than", "7"],
        vec!["generations", "activate"],
        vec!["start", "--generation", "2", "--rebuild"],
        vec!["start", "--generation", "2", "--no-rebuild"],
    ] {
        check(expect_code(&viv(&tp, &rejected)?, EX_USAGE))?;
    }
    // Never built: the empty envelope at `0`, never an error (spec/01).
    let list = viv(&tp, &["generations", "list", "--json"])?;
    check(expect_code(&list, 0))?;
    check(expect_json_keys(&list, &["generations"]))?;
    // With nothing retained, a switch names nothing and is a malformed request (spec/14).
    check(expect_code(
        &viv(&tp, &["generations", "activate", "7"])?,
        EX_USAGE,
    ))?;
    check(expect_code(
        &viv(&tp, &["generations", "rollback"])?,
        EX_USAGE,
    ))?;
    // The family is published: help names it.
    let help = viv(&tp, &["--help"])?;
    check(expect_code(&help, 0))?;
    check(expect_stdout_mentions(&help, "generations"))
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

/// Slice 025, the enumeration surface without a VM: an empty fleet is `{"projects":[],"host":…}`
/// at `0`, a never-started manifest is configuration rather than a row, fabricated retained
/// state makes a manifest a row exactly once with its declared ceiling and honest nulls, a
/// vanished workspace directory is named and never removed, and the local report's `runtime`
/// object holds shape on a project that has never built (spec/01, spec/17).
#[allow(clippy::too_many_lines)] // One enumeration story: the legs share fixtures and ordering.
fn workflow_25_fleet_usage() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("wf25-anchor").map_err(io_failed)?;

    // An empty library enumerates an empty fleet beside a populated host object.
    let empty = viv(&tp, &["status", "-g", "--json"])?;
    check(expect_code(&empty, 0))?;
    check(expect_json_keys(&empty, &["projects", "host"]))?;
    check(expect_json_fields_at(
        &empty,
        "host",
        &[
            "mem_available_bytes",
            "mem_total_bytes",
            "pressure_some_avg60",
        ],
    ))?;
    if json::array_len(&empty.stdout, "projects").map_err(Failed::from)? != 0 {
        return fail("an empty library enumerated a sandbox");
    }
    let host: serde_json::Value = serde_json::from_slice(&empty.stdout)
        .map_err(|error| Failed::from(format!("status -g --json was not JSON: {error}")))?;
    if host["host"]["mem_available_bytes"].as_u64().is_none()
        || host["host"]["mem_total_bytes"].as_u64().is_none()
    {
        return fail("the host object reported no memory readings on a Linux host");
    }

    // Two manifests, each owning its own tree.
    let first = tp.root().join("fleet-first");
    let second = tp.root().join("fleet-second");
    fs::create_dir_all(&first).map_err(io_failed)?;
    fs::create_dir_all(&second).map_err(io_failed)?;
    arrange_manifest(
        &tp,
        "fleet-a",
        "\n[resources]\nmem_mib = 2048\nvcpu = 2\n",
        &format!("\n[[workspaces]]\nsource = '{}'\n", first.display()),
    )?;
    arrange_manifest(
        &tp,
        "fleet-b",
        "",
        &format!("\n[[workspaces]]\nsource = '{}'\n", second.display()),
    )?;

    // A manifest never started is configuration, not a row (spec/01's enumeration domain).
    let configured = viv(&tp, &["status", "-g", "--json"])?;
    check(expect_code(&configured, 0))?;
    if json::array_len(&configured.stdout, "projects").map_err(Failed::from)? != 0 {
        return fail("a never-started manifest was enumerated as a sandbox");
    }

    // Retained state is what makes a sandbox: fabricate what a start would leave behind.
    for name in ["fleet-a", "fleet-b"] {
        fs::create_dir_all(
            tp.state()
                .join("vivarium")
                .join("projects")
                .join(name)
                .join("default"),
        )
        .map_err(io_failed)?;
    }
    let config_before = snapshot_tree(tp.config()).map_err(io_failed)?;
    let state_before = snapshot_tree(tp.state()).map_err(io_failed)?;

    let fleet = viv(&tp, &["status", "-g", "--json"])?;
    check(expect_code(&fleet, 0))?;
    check(expect_json_array_items(
        &fleet,
        "projects",
        &[
            "manifest",
            "workspaces",
            "path_missing",
            "state",
            "stale",
            "generation",
            "uptime_seconds",
            "resources",
            "runtime",
        ],
    ))?;
    let projects = project_rows(&fleet)?;
    let mut names: Vec<&str> = projects
        .iter()
        .filter_map(|row| row["manifest"].as_str())
        .collect();
    names.sort_unstable();
    if names != ["fleet-a", "fleet-b"] {
        return fail(format!("the fleet enumerated {names:?}"));
    }
    let row_a = projects
        .iter()
        .find(|row| row["manifest"] == "fleet-a")
        .ok_or("fleet-a lost its row")?;
    if row_a["state"] != "absent" {
        return fail(format!("a never-built sandbox reported {}", row_a["state"]));
    }
    // The resting ceiling is the declaration in force at the next launch; the undeclared
    // sibling resolves from the host, so only the declared one is pinned here.
    if row_a["resources"]["mem_mib"].as_u64() != Some(2048)
        || row_a["resources"]["vcpu"].as_u64() != Some(2)
    {
        return fail(format!(
            "the declared ceiling did not survive to the row: {}",
            row_a["resources"]
        ));
    }
    if !row_a["runtime"]["mem_used_bytes"].is_null() || !row_a["runtime"]["sessions"].is_null() {
        return fail("a resting row fabricated a liveness reading");
    }
    if row_a["runtime"]["disk_allocated_bytes"].as_u64() != Some(0) {
        return fail("a sandbox with no images reported occupied disk");
    }
    let owned = first.canonicalize().map_err(io_failed)?;
    let workspaces_a: Vec<&str> = row_a["workspaces"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect()
        })
        .unwrap_or_default();
    if workspaces_a != [owned.to_string_lossy().as_ref()] {
        return fail(format!("fleet-a's workspace set was {workspaces_a:?}"));
    }
    check(expect_tree_unchanged(tp.config(), &config_before))?;
    check(expect_tree_unchanged(tp.state(), &state_before))?;

    // An unreadable derived index answers `74` (spec/14's `status -g` row); a malformed one is a
    // cache miss that rebuilds and is exercised by `workflow_02_derived_workspace_index`.
    let index = tp.cache().join("vivarium").join("workspace-index.json");
    let readable = fs::metadata(&index).map_err(io_failed)?.permissions();
    fs::set_permissions(&index, fs::Permissions::from_mode(0o000)).map_err(io_failed)?;
    let unreadable = viv(&tp, &["status", "-g", "--json"])?;
    fs::set_permissions(&index, readable).map_err(io_failed)?;
    check(expect_code(&unreadable, EX_IOERR))?;
    check(expect_stderr_mentions(&unreadable, "index"))?;

    // A vanished declared directory: the row survives, names the path, and nothing is removed.
    fs::remove_dir_all(&second).map_err(io_failed)?;
    let missing = viv(&tp, &["status", "-g", "--json"])?;
    check(expect_code(&missing, 0))?;
    let projects = project_rows(&missing)?;
    let row_b = projects
        .iter()
        .find(|row| row["manifest"] == "fleet-b")
        .ok_or("a sandbox with a missing workspace was dropped from the enumeration")?;
    let absent: Vec<&str> = row_b["path_missing"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect()
        })
        .unwrap_or_default();
    if absent != [second.to_string_lossy().as_ref()] {
        return fail(format!("path_missing named {absent:?}"));
    }
    check(expect_stderr_mentions(&missing, "fleet-b"))?;
    check(expect_stderr_mentions(&missing, "unmounted filesystem"))?;
    check(expect_stdout_lacks(&missing, "warning"))?;

    // The local report holds the same runtime shape on a project that has never built.
    let local = viv_at(&tp, &first, &["status", "--json"])?;
    check(expect_code(&local, 0))?;
    check(expect_json_fields_at(
        &local,
        "runtime",
        &[
            "mem_used_bytes",
            "disk_allocated_bytes",
            "disk_virtual_bytes",
            "sessions",
            "pressure_some_avg60",
        ],
    ))?;
    let record: serde_json::Value = serde_json::from_slice(&local.stdout)
        .map_err(|error| Failed::from(format!("status --json was not JSON: {error}")))?;
    if !record["runtime"]["mem_used_bytes"].is_null()
        || record["runtime"]["disk_allocated_bytes"].as_u64() != Some(0)
        || !record["resources"].is_null()
    {
        return fail(format!(
            "a never-built local report fabricated a reading: {}",
            record["runtime"]
        ));
    }

    // Volume state that cannot be read degrades to `null` disk fields rather than an exit:
    // spec/14's status rows admit no code for a failed reading, and `status` keeps answering.
    let volumes = tp
        .state()
        .join("vivarium")
        .join("projects")
        .join("fleet-a")
        .join("default")
        .join("volumes");
    fs::create_dir_all(&volumes).map_err(io_failed)?;
    fs::set_permissions(&volumes, fs::Permissions::from_mode(0o000)).map_err(io_failed)?;
    let degraded = viv_at(&tp, &first, &["status", "--json"]);
    fs::set_permissions(&volumes, fs::Permissions::from_mode(0o755)).map_err(io_failed)?;
    let degraded = degraded?;
    check(expect_code(&degraded, 0))?;
    let degraded: serde_json::Value = serde_json::from_slice(&degraded.stdout)
        .map_err(|error| Failed::from(format!("status --json was not JSON: {error}")))?;
    if !degraded["runtime"]["disk_allocated_bytes"].is_null()
        || !degraded["runtime"]["disk_virtual_bytes"].is_null()
    {
        return fail(format!(
            "unreadable volume state did not degrade to null: {}",
            degraded["runtime"]
        ));
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

/// Slice 028's reclaim surface without a VM: the grammar (ADR-0113's flag placement), the
/// resting refusals, the never-materialized empty record, and the fan-out's partial-failure
/// face — every assertion spec/14's `memory trim`, `volume trim`, and `trim` rows make that
/// needs no guest.
fn workflow_28_usage() -> Result<(), Failed> {
    let tp = TempProject::new().map_err(io_failed)?;

    // Grammar outranks binding: every malformed spelling is `64` before a manifest matters.
    check(expect_code(&viv(&tp, &["memory"])?, EX_USAGE))?;
    check(expect_code(&viv(&tp, &["memory", "grow"])?, EX_USAGE))?;
    check(expect_code(
        &viv(&tp, &["memory", "trim", "--to"])?,
        EX_USAGE,
    ))?;
    check(expect_code(
        &viv(&tp, &["memory", "trim", "--to", "abc"])?,
        EX_USAGE,
    ))?;
    check(expect_code(
        &viv(&tp, &["memory", "trim", "--to", "0"])?,
        EX_USAGE,
    ))?;
    check(expect_code(
        &viv(&tp, &["memory", "trim", "extra"])?,
        EX_USAGE,
    ))?;
    // The fan-out carries neither resource command's flags (ADR-0113).
    check(expect_code(&viv(&tp, &["trim", "--to", "1024"])?, EX_USAGE))?;
    check(expect_code(&viv(&tp, &["trim", "cache"])?, EX_USAGE))?;
    check(expect_code(
        &viv(&tp, &["volume", "trim", "cache", "extra"])?,
        EX_USAGE,
    ))?;

    // Well-formed but unbound: `78`, the same refusal every project-local verb answers.
    check(expect_code(&viv(&tp, &["memory", "trim"])?, EX_CONFIG))?;
    check(expect_code(&viv(&tp, &["volume", "trim"])?, EX_CONFIG))?;

    arrange_manifest(&tp, "reclaim", "", "")?;

    // Bound but resting: a reclaim asks a running guest, so `75` with the remedy named.
    let resting = viv(&tp, &["memory", "trim"])?;
    check(expect_code(&resting, EX_TEMPFAIL))?;
    check(expect_stderr_mentions(&resting, "viv start"))?;

    // Never-materialized volumes are the empty record and exit `0`, before the running check
    // (spec/01) — and an unknown name is a malformed question whatever the state.
    let empty = viv(&tp, &["volume", "trim", "--json"])?;
    check(expect_code(&empty, 0))?;
    let record = json_record(&empty)?;
    if record["volumes"]
        .as_array()
        .is_none_or(|rows| !rows.is_empty())
        || record["reclaimed_bytes"] != 0
    {
        return fail(format!("a never-materialized project reported {record}"));
    }
    check(expect_code(
        &viv(&tp, &["volume", "trim", "nosuch"])?,
        EX_CONFIG,
    ))?;

    // Materialize what a start would leave behind, so the disk rung reaches the running check.
    let image = volume_image(&tp, "reclaim", "default");
    let Some(volume_directory) = image.parent() else {
        return fail("the volume image path has no parent directory");
    };
    fs::create_dir_all(volume_directory).map_err(io_failed)?;
    fs::write(&image, vec![0u8; 4096]).map_err(io_failed)?;
    check(expect_code(&viv(&tp, &["volume", "trim"])?, EX_TEMPFAIL))?;

    // The fan-out inherits `viv stop --all`'s partial-failure rule whole (ADR-0113): a failing
    // rung never skips the other, each is named on stderr, the first failure's category is the
    // exit, and stdout carries no record.
    let fanned = viv(&tp, &["trim", "--json"])?;
    check(expect_code(&fanned, EX_TEMPFAIL))?;
    if !fanned.stdout.is_empty() {
        return fail("a failed fan-out emitted a record on stdout");
    }
    check(expect_stderr_mentions(&fanned, "could not trim memory"))?;
    check(expect_stderr_mentions(&fanned, "could not trim volumes"))?;
    Ok(())
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
    command.env("WF26_VIV", gate().viv());
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
            let mut command = std::process::Command::new(gate().viv());
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

    let mut command = std::process::Command::new(gate().viv());
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

/// Slice 027's record surface (`Q-021`'s exit) at the CLI gate: the stop record's shape for
/// the idempotent no-op, the destroy record's four keys, and the sweep's empty record from a
/// directory no manifest declares — none of which needs a VM.
fn workflow_27_records() -> Result<(), Failed> {
    let tp = TempProject::new().map_err(io_failed)?;
    arrange_manifest(&tp, "records", "", "")?;

    let stopped = viv(&tp, &["stop", "--json"])?;
    check(expect_code(&stopped, 0))?;
    check(expect_json_keys(&stopped, &["manifest", "state", "rung"]))?;
    let record = json_record(&stopped)?;
    if record["state"] != "absent" || !record["rung"].is_null() {
        return fail(format!("the no-op stop reported {record}"));
    }

    // The human face of a side-effect verb says nothing on stdout (spec/01).
    let human = viv(&tp, &["stop"])?;
    check(expect_code(&human, 0))?;
    if !human.stdout.is_empty() {
        return fail("a human stop printed to stdout");
    }

    let destroyed = viv(&tp, &["destroy", "--yes", "--json"])?;
    check(expect_code(&destroyed, 0))?;
    check(expect_json_keys(
        &destroyed,
        &["manifest", "removed", "spared", "volumes_kept"],
    ))?;
    let record = json_record(&destroyed)?;
    if record["volumes_kept"] != false {
        return fail(format!(
            "a plain destroy reported {}",
            record["volumes_kept"]
        ));
    }
    if record["spared"].as_array().is_none_or(Vec::is_empty) {
        return fail("the destroy record spared nothing");
    }

    let swept = viv_at(&tp, tp.root(), &["stop", "--all", "--json"])?;
    check(expect_code(&swept, 0))?;
    if !project_rows(&swept)?.is_empty() {
        return fail("a sweep over nothing running reported rows");
    }
    Ok(())
}

/// One `--json` record parsed whole, for the value assertions the key helpers cannot make.
fn json_record(out: &VivOutput) -> Result<serde_json::Value, Failed> {
    serde_json::from_slice(&out.stdout)
        .map_err(|error| Failed::from(format!("stdout was not one JSON record: {error}")))
}

/// The parsed rows under a record's `projects` key, in published order — `status -g` and the
/// `stop --all` sweep share the wrapper (spec/01).
fn project_rows(out: &VivOutput) -> Result<Vec<serde_json::Value>, Failed> {
    json_record(out)?["projects"]
        .as_array()
        .cloned()
        .ok_or_else(|| Failed::from("the record published no `projects` array"))
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
    let mut command = std::process::Command::new(gate().viv());
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

fn viv_with_env(
    tp: &TempProject,
    args: &[&str],
    extra: &[(&str, &str)],
) -> Result<VivOutput, Failed> {
    support::run_viv_with_env(gate().viv(), tp, tp.project(), args, extra).map_err(io_failed)
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

fn arrange_manifest(
    tp: &TempProject,
    name: &str,
    manifest_body: &str,
    manifest_tail: &str,
) -> Result<(), Failed> {
    arrange_manifest_with_image(tp, name, manifest_body, manifest_tail, "{ ... }: { }\n")
}

fn arrange_manifest_with_image(
    tp: &TempProject,
    name: &str,
    manifest_body: &str,
    manifest_tail: &str,
    image_body: &str,
) -> Result<(), Failed> {
    let workspace =
        if manifest_body.contains("[[workspaces]]") || manifest_tail.contains("[[workspaces]]") {
            String::new()
        } else {
            format!("\n[[workspaces]]\nsource = '{}'\n", tp.project().display())
        };
    write_file(
        &tp.config()
            .join("vivarium")
            .join("images")
            .join("minimal.nix"),
        image_body,
    )
    .map_err(io_failed)?;
    write_file(
        &tp.config()
            .join("vivarium")
            .join("manifests")
            .join(format!("{name}.toml")),
        &format!("image = \"minimal\"\n{manifest_body}{manifest_tail}{workspace}"),
    )
    .map_err(io_failed)
}

fn write_piece(tp: &TempProject, name: &str, contents: &str) -> Result<(), Failed> {
    write_file(
        &tp.config()
            .join("vivarium")
            .join("pieces")
            .join(format!("{name}.nix")),
        contents,
    )
    .map_err(io_failed)
}

fn viv(tp: &TempProject, args: &[&str]) -> Result<VivOutput, Failed> {
    run_viv(gate().viv(), tp, tp.project(), args).map_err(io_failed)
}

fn viv_at(tp: &TempProject, cwd: &Path, args: &[&str]) -> Result<VivOutput, Failed> {
    run_viv(gate().viv(), tp, cwd, args).map_err(io_failed)
}

/// The `VIVARIUM_` variables a child is allowed to see, and why each one is there.
///
/// `VIVARIUM_BASELINE_*` pin the generated flake's two baseline inputs — `nixpkgs` and `microvm`
/// — to local store paths, which is what keeps this suite off the GitHub API. Neither
/// participates in manifest resolution, so neither can turn an expected `78` into a success.
const INJECTED_VARIABLES: [&str; 2] = ["VIVARIUM_BASELINE_NIXPKGS", "VIVARIUM_BASELINE_MICROVM"];

fn expect_injected_vivarium_variables_only(out: &VivOutput) -> Result<(), String> {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let leaked: Vec<&str> = stdout
        .lines()
        .filter(|line| line.starts_with("VIVARIUM_"))
        .filter(|line| {
            !INJECTED_VARIABLES
                .iter()
                .any(|name| line.starts_with(&format!("{name}=")))
        })
        .collect();
    if leaked.is_empty() {
        return Ok(());
    }
    Err(format!(
        "the child saw vivarium variables the harness did not inject: {}",
        leaked.join(", ")
    ))
}

fn require_gate(level: GateLevel) -> Result<(), Failed> {
    gate()
        .result(level)
        .as_ref()
        .map_err(|reason| {
            // Reached either because the operator set VIVARIUM_TEST_REQUIRE=1, or because
            // the trial was run despite its ignored flag (`--run-ignored`). Naming the
            // wrong one sends the reader looking for a variable they never set.
            let cause = if gate_required() {
                "gate unmet but VIVARIUM_TEST_REQUIRE=1"
            } else {
                "gate unmet and the ignored flag was overridden"
            };
            Failed::from(format!("{cause}: {reason}"))
        })
        .copied()
}

fn check(result: Result<(), String>) -> Result<(), Failed> {
    result.map_err(Failed::from)
}

// Consumes the error rather than stringifying a borrow, which is what keeps
// `clippy::needless_pass_by_value` satisfied without an explicit `drop`.
fn io_failed(error: std::io::Error) -> Failed {
    Failed::from(error)
}

fn fail<T>(message: impl Into<String>) -> Result<T, Failed> {
    Err(Failed::from(message.into()))
}
