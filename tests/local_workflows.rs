//! The `local` lane: the workflow trials that need only the built `viv` and a
//! filesystem — usage surfaces, fail-closed refusals, and the `--json` records a
//! command renders without evaluating or booting anything.
//!
//! This binary runs everywhere, so it declares no preflight. `viv` is resolved at
//! compile time (`support::preflight::viv`), which makes a missing binary a build
//! error rather than a trial that silently probes a path nobody is building.
//!
//! Its siblings are `eval_workflows` (needs Nix) and `boot_workflows` (needs a
//! guest). The lane register is `docs/reference/testing-lanes.md`.

mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;

use libtest_mimic::{Arguments, Failed, Trial};
use support::{
    EX_CONFIG, EX_IOERR, EX_TEMPFAIL, EX_UNAVAILABLE, EX_USAGE, TempProject, VOLUME_TAIL,
    VivOutput, arrange_manifest, check, doctor_check, expect_code, expect_derived_manifest_visible,
    expect_json_array_items, expect_json_array_nonempty, expect_json_fields_at, expect_json_keys,
    expect_stderr_mentions, expect_stdout_lacks, expect_stdout_mentions, expect_tree_unchanged,
    fail, io_failed, json, json_record, project_rows, run_viv, snapshot_tree, viv, viv_at,
    viv_with_env, volume_image, write_file, write_piece,
};

fn main() -> std::process::ExitCode {
    let args = Arguments::from_args();
    let trials = vec![
        Trial::test("harness_self_check", harness_self_check),
        Trial::test(
            "workflow_01_manifest_workspace_resolution_usage",
            workflow_01_usage,
        ),
        Trial::test("workflow_02_derived_workspace_index", workflow_02_index),
        Trial::test("workflow_04_inspect_before_run_usage", workflow_04_usage),
        Trial::test("workflow_06_exec_usage_surface", workflow_06_usage),
        Trial::test(
            "workflow_07_volume_list_requires_manifest",
            workflow_07_usage,
        ),
        Trial::test("workflow_08_destroy_usage_surface", workflow_08_usage),
        Trial::test("workflow_16_doctor_report", workflow_16_doctor),
        Trial::test("workflow_18_update_usage_surface", workflow_18_update_usage),
        Trial::test("workflow_24_generations_usage_surface", workflow_24_usage),
        Trial::test("workflow_25_fleet_usage_surface", workflow_25_fleet_usage),
        Trial::test("workflow_27_stop_destroy_records", workflow_27_records),
        Trial::test("workflow_28_trim_usage_surface", workflow_28_usage),
    ];
    libtest_mimic::run(&args, trials).exit_code()
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
    // Five of the six roots are durable and must live inside the temp root. The runtime root is
    // the exception, and deliberately so. `TempProject` prefers the session's own
    // `/run/user/<uid>` because a control-socket path assembled beneath `TMPDIR` overruns the
    // 108-byte Unix-socket limit on a host whose `TMPDIR` is long — the case a disk-heavy lane
    // creates — and falls back inside the temp root on a host with no session runtime directory,
    // which a fixture that never binds a socket is correct under. So what this asserts is the
    // rule: no durable root escapes the temp root, and the runtime root is wherever the fixture
    // put it.
    let environment = tp.env();
    if environment.len() != 6 {
        return fail("isolated environment does not name all six roots");
    }
    for (name, value) in &environment {
        if name == "XDG_RUNTIME_DIR" {
            continue;
        }
        if !Path::new(value).starts_with(tp.root()) {
            return fail(format!(
                "durable root `{}` escaped the temp root: {}",
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
    // Read from the finding rather than from the exit code, which is this host's
    // health and which the header above says this trial never asserts. `viv
    // doctor` exits `78` on any failing hard probe, so on a machine without
    // virtualization the old reading failed here and named the ownership finding
    // for a refusal that had nothing to do with it. What makes the finding a
    // report rather than a refusal is its own severity: only a hard probe carries
    // an exit code (spec/13), so a soft one cannot turn this condition into `78`
    // however the rest of the host reads.
    let finding = doctor_check(&ownership, "working-directory-declared")?;
    if finding["severity"] == serde_json::json!("hard") {
        return fail(
            "ADR-0109's ownership finding is hard, so doctor refuses instead of reporting",
        );
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
    // Read from the findings, not from the exit code — the same reason as above.
    // What the comment claims is that the two ownership findings are reported;
    // whether this machine can also run a VM is a different question doctor
    // answers in the same breath and this trial has no business asserting.
    let diagnosed = viv_at(&tp, &elsewhere, &["doctor"])?;
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
