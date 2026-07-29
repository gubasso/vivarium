mod support;

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use libtest_mimic::{Arguments, Failed, Trial};
use support::{
    EX_CONFIG, EX_DATAERR, EX_USAGE, GateLevel, TempProject, VivOutput, expect_code,
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
const WORKFLOWS: [WorkflowSpec; 16] = [
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
    let args = Arguments::from_args();
    let mut trials = vec![Trial::test("harness_self_check", harness_self_check)];
    for (name, level, runner) in WORKFLOWS {
        trials.push(gated_trial(name, level, runner));
    }
    libtest_mimic::run(&args, trials).exit_code()
}

/// Whether the operator demanded that an unmet gate fail rather than skip.
fn gate_required() -> bool {
    std::env::var_os("VIVARIUM_TEST_REQUIRE").is_some_and(|value| value == "1")
}

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
    if tp.env().len() != 6
        || tp
            .env()
            .iter()
            .any(|(_, value)| !Path::new(value).starts_with(tp.root()))
    {
        return fail("isolated environment does not contain six roots inside the temp root");
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
    // assertion: an ambient VIVARIUM_MANIFEST reaching the child.
    let leak = run_viv(Path::new("sh"), &tp, tp.project(), &["-c", "env"]).map_err(io_failed)?;
    check(expect_stdout_lacks(&leak, "VIVARIUM_"))?;

    if !gate().is_well_formed() {
        return fail("gate probe returned a malformed decision");
    }

    check(harness_json_self_check())
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
    if json::missing_keys(b"not json", &["state"]).is_ok() {
        return Err("missing_keys accepted output that is not JSON".to_owned());
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

    if json::string_field(envelope, "state")? != Some("running".to_owned()) {
        return Err("string_field did not read a top-level string".to_owned());
    }
    if json::string_field(envelope, "volumes")?.is_some() {
        return Err("string_field returned a value for a non-string field".to_owned());
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

    if json::array_len(nested, "conflicts")? != 1 {
        return Err("array_len miscounted a populated array".to_owned());
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
    check(expect_marker_id(tp.project(), "project"))?;
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
    check(expect_json_string(&status, "state", "running"))
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
    check(expect_marker_id(first.project(), "api"))?;

    let second_project = first.root().join("collision").join("API");
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
    check(expect_marker_id(&second_project, "api-2"))
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
    check(expect_stdout_mentions(&evaluated, "manifest.example"))?;
    check(expect_stdout_mentions(&evaluated, "piece.example"))?;

    let sources = viv(&tp, &["config", "sources", "--json"])?;
    check(expect_code(&sources, 0))?;
    check(expect_stdout_mentions(&sources, "egress-restriction"))?;
    // The provenance view must still show the image layer that `config eval` dropped.
    check(expect_stdout_mentions(&sources, "\"open\""))
}

// Guide: docs/guides/restrict-egress-allowlist.md
fn workflow_05_enforcement() -> Result<(), Failed> {
    let tp = arrange_egress_fixture()?;
    check(expect_code(&viv(&tp, &["start"])?, 0))?;

    // Both arms travel the same reserved documentation space, which RFC 6761 leaves to
    // ordinary resolution, so the only difference between them is allowlist membership.
    // The allowed host must be one the fixture actually permits; a host absent from the
    // allowlist could not exit `0` against a conforming default-deny implementation.
    let allowed = viv(
        &tp,
        &[
            "exec",
            "--",
            "sh",
            "-lc",
            GUEST_FETCH,
            "fetch",
            "https://manifest.example",
        ],
    )?;
    check(expect_code(&allowed, 0))?;

    // Deliberately omitted from both layers' `allow` lists. A `.invalid` host would not
    // work here: RFC 6761 guarantees an immediate negative response for that space, so
    // the denial arm would pass on an ordinary resolution error with no filter present.
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
            "https://blocked.example",
        ],
    )?;
    let elapsed = started.elapsed();
    // No exit code describes a denial: enforcement is host-side but the failure is
    // observed by a guest process, and after guest-process start `exec` returns that
    // process's status verbatim. So the status assertion stays at "non-zero" rather
    // than inventing a vivarium code.
    check(expect_nonzero(&denied))?;
    // What spec/05 *does* fix is reject-not-drop: the attempt must fail promptly, never
    // be silently discarded. Timing is the honest way to check that. Asserting on stderr
    // content instead would test whichever fetch tool the image ships, not vivarium.
    //
    // TODO(spec): the enforcement mechanism is section E's to design
    // (.draft/design-todo/todo.md, "Allowlist enforcement model"), and both arms above
    // are waiting on it. `.example`
    // is reserved documentation space with no delegation in the root zone, so neither
    // host resolves: the allowed arm cannot reach a real endpoint, and the denied arm
    // fails fast on NXDOMAIN whether or not a filter is in force. Section E owes this
    // trial a guest-reachable controlled endpoint with two names differing only in
    // allowlist membership; until then the budget below pins the contract's shape, not
    // yet its enforcement.
    if elapsed >= DENIAL_BUDGET {
        return fail(format!(
            "denied fetch took {elapsed:?}; a rejection must fail fast rather \
            than hang on connect retries (spec/05, ADR-0044)"
        ));
    }
    Ok(())
}

fn arrange_egress_fixture() -> Result<TempProject, Failed> {
    let tp = TempProject::new().map_err(io_failed)?;
    // The image supplies the shadowed default; only the piece forces the allowlist, so
    // the precedence under test is genuinely arranged rather than pre-decided by the
    // manifest. Each layer contributes a distinct host to make concatenation observable.
    arrange_manifest_with_image(
        &tp,
        "restricted",
        "pieces = [ \"egress-restriction\" ]\n",
        "\n[egress]\nallow = [ \"manifest.example\" ]\n",
        "{ lib, ... }: { sandbox.egress.mode = lib.mkDefault \"open\"; }\n",
    )?;
    write_piece(
        &tp,
        "egress-restriction",
        r#"{ lib, ... }: {
    sandbox.egress.mode = lib.mkForce "allowlist";
    sandbox.egress.allow = [ "piece.example" ];
}
"#,
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
    check(expect_code(&viv(&tp, &["start"])?, 0))?;
    check(expect_code(&viv(&tp, &["exec", "--", "true"])?, 0))?;
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

    // Assumes the specified guest toolchain supplies POSIX sh. The exec'd shell
    // signals itself, so this is a genuinely signal-killed guest process — not a
    // shell converting a signal into an exit code.
    check(expect_code(
        &viv(
            &tp,
            &["exec", "--", "sh", "-lc", "exec sh -c 'kill -TERM $$'"],
        )?,
        143,
    ))
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
    check(expect_volume_image(&tp, "volume-project", "default"))?;
    check(expect_volume_image(&tp, "volume-project", "cache"))?;

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
    check(expect_marker_id(tp.project(), "destroy-project"))?;
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
    let default_image = volume_image(&tp, "destroy-project", "default");
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
        &format!("image = \"minimal\"\n{manifest_body}{manifest_tail}"),
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
