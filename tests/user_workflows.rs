mod support;

use std::fs;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use libtest_mimic::{Arguments, Failed, Trial};
use support::{
    EX_CONFIG, EX_DATAERR, EX_IOERR, EX_USAGE, GateLevel, TempProject, VivOutput, expect_code,
    expect_json_array_items, expect_json_array_nonempty, expect_json_fields_at, expect_json_keys,
    expect_json_map_entries, expect_json_string, expect_marker_absent, expect_marker_id,
    expect_no_project_binding_files, expect_no_volume_images, expect_nonzero,
    expect_registry_binding_visible, expect_stderr_mentions, expect_stdout_lacks,
    expect_stdout_mentions, expect_tree_unchanged, expect_volume_image, gate, json, run_viv,
    snapshot_tree, volume_image, write_file,
};

type WorkflowRunner = fn() -> Result<(), Failed>;
type WorkflowSpec = (&'static str, GateLevel, WorkflowRunner);

/// One trial per workflow *and gate level*, not per workflow. A trial carries a single
/// ignore flag, so folding a workflow's CLI-only assertions in with its boot-dependent
/// ones would hide the cheap half behind `/dev/kvm` — exactly what the three-level gate
/// exists to avoid. Every trial keeps its `workflow_NN_` prefix so the guide pairing
/// survives the split.
const WORKFLOWS: [WorkflowSpec; 26] = [
    (
        "workflow_01_first_time_bind_usage",
        GateLevel::Cli,
        workflow_01_usage,
    ),
    (
        "workflow_01_first_time_bind_boot",
        GateLevel::Virtualization,
        workflow_01_boot,
    ),
    (
        "workflow_02_clean_repo_global_registry_only",
        GateLevel::Cli,
        workflow_02_registry,
    ),
    (
        "workflow_02_identity_collision_suffix",
        GateLevel::Virtualization,
        workflow_02_identity,
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
        "workflow_07_volume_list_requires_binding",
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

    // The one isolation the temporary roots cannot provide, asserted here because it is the only
    // vantage point that does not need `/dev/kvm`. A project id reaches the
    // `vivarium-<project-id>-<target>.service` unit name, which lives in the session's systemd user
    // manager and is shared by every trial in the run — so two fixtures asking for the same project
    // name must still resolve different ids, or a parallel run fails with "already loaded" for a
    // reason that has nothing to do with the product. Checked through `project_id` rather than by
    // booting, so the property is verified on a host that cannot boot anything.
    let sibling = TempProject::with_project_name("project").map_err(io_failed)?;
    if tp.project_id() == sibling.project_id() {
        return fail(format!(
            "two fixtures resolved the same project id `{}`, so their units would collide",
            tp.project_id()
        ));
    }
    if !tp.project_id().ends_with(tp.token()) || tp.token() == sibling.token() {
        return fail("the fixture token did not reach the project id uniquely");
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

// Guide: docs/guides/first-time-bind-boot.md
fn workflow_01_usage() -> Result<(), Failed> {
    let tp = TempProject::new().map_err(io_failed)?;
    arrange_manifest(&tp, "rust-web", "", "")?;
    let before = snapshot_tree(tp.project()).map_err(io_failed)?;

    let preview = viv(&tp, &["init", "--manifest", "rust-web", "--no-input"])?;
    check(expect_code(&preview, 0))?;
    check(expect_tree_unchanged(tp.project(), &before))?;
    check(expect_no_project_binding_files(tp.project()))?;
    check(expect_code(&viv(&tp, &["config", "--json"])?, EX_CONFIG))?;

    let write = viv(&tp, &["init", "--manifest", "rust-web", "--write", "--yes"])?;
    check(expect_code(&write, 0))?;
    let binding = viv(&tp, &["config", "--json"])?;
    check(expect_registry_binding_visible(&binding, "rust-web"))?;
    check(expect_tree_unchanged(tp.project(), &before))?;

    check(expect_code(
        &viv(&tp, &["start", "--rebuild", "--no-rebuild"])?,
        EX_USAGE,
    ))?;

    let unbound = TempProject::new().map_err(io_failed)?;
    check(expect_code(&viv(&unbound, &["shell"])?, EX_CONFIG))?;
    check(expect_code(&viv(&tp, &["shell", "--unknown"])?, EX_USAGE))
}

// Guide: docs/guides/first-time-bind-boot.md
fn workflow_01_boot() -> Result<(), Failed> {
    let tp = TempProject::new().map_err(io_failed)?;
    arrange_manifest(&tp, "rust-web", "", "")?;
    bind(&tp, "rust-web")?;

    check(expect_code(&viv(&tp, &["start"])?, 0))?;
    // Marker assertions stay behind the virtualization gate permanently: `start` is
    // the cheapest verb that mints one, because every read-only command resolves the
    // identity in memory and persists nothing (spec/15, ADR-0043). There is no
    // CLI-only vantage point from which to check this.
    check(expect_marker_id(
        tp.project(),
        &format!("project-{}", tp.token()),
    ))?;
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

// Guide: docs/guides/clean-repo-global-registry.md
fn workflow_02_registry() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("API").map_err(io_failed)?;
    arrange_manifest(&tp, "clean-registry", "", "")?;
    let before = snapshot_tree(tp.project()).map_err(io_failed)?;
    check(expect_code(
        &viv(&tp, &["init", "--manifest", "clean-registry", "--no-input"])?,
        0,
    ))?;
    check(expect_tree_unchanged(tp.project(), &before))?;
    check(expect_no_project_binding_files(tp.project()))?;
    check(expect_code(
        &viv(
            &tp,
            &["init", "--manifest", "clean-registry", "--write", "--yes"],
        )?,
        0,
    ))?;
    check(expect_registry_binding_visible(
        &viv(&tp, &["config", "--json"])?,
        "clean-registry",
    ))?;
    check(expect_tree_unchanged(tp.project(), &before))
}

// Guide: docs/guides/clean-repo-global-registry.md
fn workflow_02_identity() -> Result<(), Failed> {
    let first = TempProject::with_project_name("API").map_err(io_failed)?;
    arrange_manifest(&first, "clean-registry", "", "")?;
    bind(&first, "clean-registry")?;
    check(expect_code(&viv(&first, &["start"])?, 0))?;
    check(expect_marker_id(
        first.project(),
        &format!("api-{}", first.token()),
    ))?;

    // The colliding project takes the fixture's own basename, not the bare name: the token the
    // fixture appends is what keeps this pair's unit names clear of every other trial's, and two
    // projects only collide if they sanitize to the same base.
    let second_project = first.root().join("collision").join(first.basename());
    fs::create_dir_all(&second_project).map_err(io_failed)?;
    let second_project = second_project.canonicalize().map_err(io_failed)?;
    check(expect_code(
        &viv_at(
            &first,
            &second_project,
            &["init", "--manifest", "clean-registry", "--write", "--yes"],
        )?,
        0,
    ))?;
    check(expect_code(
        &viv_at(&first, &second_project, &["start"])?,
        0,
    ))?;
    // The smallest-free-integer rule: the first holder keeps the bare name, the
    // second gets `-2`. There is deliberately no `api-1`.
    check(expect_marker_id(
        &second_project,
        &format!("api-{}-2", first.token()),
    ))
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

    bind(&tp, "ana-api")?;
    let ana = viv(&tp, &["config", "eval", "--json"])?;
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

    let sources = viv(&tp, &["config", "sources", "--json"])?;
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

    // Rebinding to the other person's manifest changes the result without any edit to
    // the shared artifact.
    bind(&tp, "bruno-api")?;
    let bruno = viv(&tp, &["config", "eval", "--json"])?;
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
    let invalid = viv(tp, &["config", "eval", "--json"])?;
    check(expect_code(&invalid, EX_DATAERR))?;
    check(expect_stderr_mentions(&invalid, "/home/ana"))?;

    let still_readable = viv(tp, &["config", "sources", "--json"])?;
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
    check(expect_stdout_mentions(&still_readable, "/home/ana"))
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
    bind(tp, "tie-demo")?;

    check(expect_code(
        &viv(tp, &["config", "eval", "--json"])?,
        EX_DATAERR,
    ))?;

    let tie = viv(tp, &["config", "sources", "--json"])?;
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

    // `config` fails closed before a binding exists; it never renders an empty record.
    check(expect_code(&viv(&tp, &["config", "--json"])?, EX_CONFIG))?;
    bind(&tp, "inspect-dev")?;
    check(expect_registry_binding_visible(
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
    bind(&tp, "inspect-dev")?;

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
    bind(&tp, "conflict")?;
    check(expect_code(
        &viv(&tp, &["config", "eval", "--json"])?,
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
    bind(&tp, "restricted")?;
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
    bind(&tp, "agent-command")?;
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
    bind(&tp, "interactive")?;
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
    check(expect_stderr_mentions(&report, "no manifest bound"))?;

    // The human report wears bracketed word markers — never glyphs — and closes with the summary
    // line naming the exit (spec/13).
    let human = viv(&tp, &["doctor"])?;
    check(expect_stdout_mentions(&human, "[skipped]"))?;
    check(expect_stdout_mentions(&human, "-> exit "))?;
    Ok(())
}

// Guide: docs/guides/stop-restart-preserving-volumes.md
fn workflow_07_usage() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("volume-project").map_err(io_failed)?;
    arrange_manifest(&tp, "volumes", "", VOLUME_TAIL)?;
    check(expect_code(
        &viv(&tp, &["volume", "list", "--json"])?,
        EX_CONFIG,
    ))?;
    bind(&tp, "volumes")?;
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
    bind(&tp, "volumes")?;
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
    check(expect_volume_image(&tp, "default"))?;
    check(expect_volume_image(&tp, "cache"))?;

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
    bind(&tp, "volumes")?;
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
    // The manifest name is deliberately unlike the project name: a binding assertion
    // that matched the project id would pass on the state path alone.
    let tp = TempProject::with_project_name("destroy-project").map_err(io_failed)?;
    arrange_manifest(&tp, "teardown-demo", "", "")?;
    bind(&tp, "teardown-demo")?;
    check(expect_registry_binding_visible(
        &viv(&tp, &["config", "--json"])?,
        "teardown-demo",
    ))?;
    check(expect_code(&viv(&tp, &["destroy"])?, EX_USAGE))?;

    // `gc` is a global whole-store sweep: it never requires a bound manifest, so it
    // cannot answer `78` even from an unbound directory.
    let global = TempProject::new().map_err(io_failed)?;
    let gc = viv(&global, &["gc"])?;
    if gc.status.code() == Some(EX_CONFIG) {
        return fail("viv gc incorrectly required a bound manifest");
    }
    Ok(())
}

// Guide: docs/guides/destroy-cold-rebuild.md
fn workflow_08_rebuild() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("destroy-project").map_err(io_failed)?;
    arrange_manifest(&tp, "teardown-demo", "", "")?;
    bind(&tp, "teardown-demo")?;
    check(expect_code(&viv(&tp, &["start"])?, 0))?;
    check(expect_code(
        &viv(
            &tp,
            &["exec", "--", "sh", "-lc", "printf old > \"$HOME/old\""],
        )?,
        0,
    ))?;
    check(expect_code(&viv(&tp, &["destroy", "--yes"])?, 0))?;
    // The authoritative model: the marker is removed, the binding survives.
    check(expect_marker_absent(tp.project()))?;
    check(expect_registry_binding_visible(
        &viv(&tp, &["config", "--json"])?,
        "teardown-demo",
    ))?;
    check(expect_code(&viv(&tp, &["destroy", "--yes"])?, 0))?;

    // Variant A — cold: the next start is a clean first run.
    check(expect_code(&viv(&tp, &["start"])?, 0))?;
    check(expect_marker_id(
        tp.project(),
        &format!("destroy-project-{}", tp.token()),
    ))?;
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
    let default_image = volume_image(&tp, "default");
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
/// can seed a plain directory as the "build": `--no-rebuild` selects whatever `last-build`
/// records, the check reads only `share/vivarium/launch-contract-schema`, and the refusal must
/// land before anything in the tree is executed. Behind the virtualization gate because `start`
/// preflights the host before selecting a build, not because anything boots — no case here
/// reaches a launcher.
fn workflow_15_contract_skew() -> Result<(), Failed> {
    let ours = vivarium::launch::LAUNCH_SCHEMA_VERSION;
    let theirs = ours - 1;
    let tp = TempProject::with_project_name("skew-project").map_err(io_failed)?;
    arrange_manifest(&tp, "skew-demo", "", "")?;
    bind(&tp, "skew-demo")?;

    // A stand-in for an old generation: a tree that publishes an older contract number at the
    // stable path. `last-build` is a store path as text, and `--no-rebuild` trusts it.
    let build = tp.root().join("stale-build");
    let schema = build
        .join("share")
        .join("vivarium")
        .join("launch-contract-schema");
    std::fs::create_dir_all(schema.parent().unwrap_or(&build)).map_err(io_failed)?;
    write_file(&schema, &format!("{theirs}\n")).map_err(io_failed)?;
    let record = tp
        .data()
        .join("vivarium")
        .join("projects")
        .join(tp.project_id())
        .join("default")
        .join("last-build");
    std::fs::create_dir_all(record.parent().unwrap_or(&build)).map_err(io_failed)?;
    write_file(&record, &format!("{}\n", build.display())).map_err(io_failed)?;

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
/// is selected — through `last-build` and `--no-rebuild`.
fn workflow_15_contract_skew_live() -> Result<(), Failed> {
    let ours = vivarium::launch::LAUNCH_SCHEMA_VERSION;
    let foreign = ours + 1;
    let tp = TempProject::with_project_name("skew-live").map_err(io_failed)?;
    arrange_manifest(&tp, "skew-live", "", "")?;
    bind(&tp, "skew-live")?;

    // A real build and boot of the current shape, then a clean stop so the refusal below is
    // about the selected build rather than about a running VM.
    check(expect_code(&viv(&tp, &["start"])?, 0))?;
    check(expect_code(&viv(&tp, &["stop"])?, 0))?;

    let record = tp
        .data()
        .join("vivarium")
        .join("projects")
        .join(tp.project_id())
        .join("default")
        .join("last-build");
    let built = std::fs::read_to_string(&record).map_err(io_failed)?;
    let built = built.trim();

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
    write_file(&record, &format!("{}\n", doctored.display())).map_err(io_failed)?;

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
    bind(&tp, "workspace-demo")?;

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
    bind(&tp, "mounts-eval")?;
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
    check(expect_resting(&tp))
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
    bind(&tp, "mounts-demo")?;

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
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .status()
            .map_err(io_failed)?;
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

    // The manifest is the personal layer, where a literal absolute path is ordinary authorship;
    // source and target are the SAME path, which is the whole point — the worktree's `.git`
    // file names it absolutely, from either side.
    let main_spelling = main_repo.to_string_lossy().into_owned();
    let worktree_spelling = worktree.to_string_lossy().into_owned();
    arrange_manifest(
        &tp,
        "wf17-worktree",
        &format!(
            "\n[[workspaces]]\nsource = '{main_spelling}'\n\
            \n[[workspaces]]\nsource = '{worktree_spelling}'\n\
            \n[[mounts]]\nsource = \"{main_spelling}\"\n\
            target = \"{main_spelling}\"\nreadonly = false\n"
        ),
        "",
    )?;
    check(expect_code(
        &viv_at(
            &tp,
            &worktree,
            &["init", "--manifest", "wf17-worktree", "--write", "--yes"],
        )?,
        0,
    ))?;

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
    bind(&tp, "wf22-demo")?;

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

/// Rewrites the one manifest this trial binds and rebinds it, one defective declaration a leg.
fn bind_after_arrange(tp: &TempProject, mounts: &str) -> Result<(), Failed> {
    arrange_manifest(tp, "mounts-refusals", mounts, "")?;
    bind(tp, "mounts-refusals")
}

/// The resting assertion every refusal leg shares: the refusal left a build and no VM.
fn expect_resting(tp: &TempProject) -> Result<(), String> {
    let status = run_viv(gate().viv(), tp, tp.project(), &["status", "--json"])
        .map_err(|error| error.to_string())?;
    expect_code(&status, 0)?;
    expect_json_string(&status, "state", "built")
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

fn bind(tp: &TempProject, manifest: &str) -> Result<(), Failed> {
    check(expect_code(
        &viv(tp, &["init", "--manifest", manifest, "--write", "--yes"])?,
        0,
    ))
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
