mod support;

use std::fs;
use std::path::Path;

use libtest_mimic::{Arguments, Failed, Trial};
use support::{
    EX_CONFIG, EX_DATAERR, EX_USAGE, GateLevel, TempProject, VivOutput, expect_code,
    expect_json_keys, expect_marker_absent, expect_marker_id, expect_no_project_binding_files,
    expect_no_volume_images, expect_nonzero, expect_registry_binding_visible,
    expect_stderr_mentions, expect_stdout_lacks, expect_stdout_mentions, expect_tree_unchanged,
    expect_volume_image, gate, run_viv, snapshot_tree, volume_image, write_file,
};

type WorkflowRunner = fn() -> Result<(), Failed>;
type WorkflowSpec = (&'static str, GateLevel, WorkflowRunner);

/// One trial per workflow *and gate level*, not per workflow. A trial carries a single
/// ignore flag, so folding a workflow's CLI-only assertions in with its boot-dependent
/// ones would hide the cheap half behind `/dev/kvm` — exactly what the three-level gate
/// exists to avoid. Every trial keeps its `workflow_NN_` prefix so the guide pairing
/// survives the split.
const WORKFLOWS: [WorkflowSpec; 15] = [
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

fn main() -> std::process::ExitCode {
    let args = Arguments::from_args();
    let mut trials = vec![Trial::test("harness_self_check", harness_self_check)];
    for (name, level, runner) in WORKFLOWS {
        trials.push(gated_trial(name, level, runner));
    }
    libtest_mimic::run(&args, trials).exit_code()
}

fn gated_trial(name: &'static str, level: GateLevel, runner: fn() -> Result<(), Failed>) -> Trial {
    let require = std::env::var_os("VIVARIUM_TEST_REQUIRE").is_some_and(|value| value == "1");
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
    // TODO(spec): move marker checks to a cheaper gate when marker-minting verbs are specified.
    check(expect_marker_id(tp.project(), "project"))?;
    check(expect_code(&viv(&tp, &["start"])?, 0))?;

    let status = viv(&tp, &["status", "--json"])?;
    check(expect_code(&status, 0))?;
    check(expect_json_keys(&status, &["state", "stale"]))?;
    let status_text = String::from_utf8_lossy(&status.stdout);
    let allowed_states = [
        "absent", "built", "starting", "running", "stopping", "failed",
    ];
    if !allowed_states
        .iter()
        .any(|state| status_text.contains(&format!("\"{state}\"")))
    {
        return fail("status did not report one of the six specified states");
    }
    if status_text.contains("\"failed\"") {
        check(expect_json_keys(&status, &["reason"]))?;
    }
    Ok(())
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

// Guide: docs/guides/team-shared-personal-overrides.md
fn workflow_03() -> Result<(), Failed> {
    let tp = TempProject::new().map_err(io_failed)?;
    arrange_manifest(
        &tp,
        "shared-personal",
        "pieces = [ \"team\", \"personal\" ]\n",
        "",
    )?;
    // Both layers set the same key to values that appear nowhere else in the fixture,
    // so the assertions below cannot be satisfied by an incidental substring.
    write_piece(
        &tp,
        "team",
        r#"{ lib, ... }: {
    vivarium.env.WF3_OVERRIDE = lib.mkDefault "team-shadowed";
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
    write_piece(
        &tp,
        "personal",
        "{ ... }: { vivarium.env.WF3_OVERRIDE = \"personal-wins\"; }\n",
    )?;
    bind(&tp, "shared-personal")?;

    let evaluated = viv(&tp, &["config", "eval", "--json"])?;
    check(expect_code(&evaluated, 0))?;
    check(expect_json_keys(
        &evaluated,
        &["manifest", "image", "pieces", "config"],
    ))?;
    // The merged view carries the winner and *only* the winner.
    check(expect_stdout_mentions(&evaluated, "personal-wins"))?;
    check(expect_stdout_lacks(&evaluated, "team-shadowed"))?;
    // Host-side variables in mount sources stay unexpanded through evaluation.
    check(expect_stdout_mentions(&evaluated, "${HOME}"))?;

    let sources = viv(&tp, &["config", "sources", "--json"])?;
    check(expect_code(&sources, 0))?;
    check(expect_json_keys(
        &sources,
        &[
            "manifest",
            "image",
            "pieces",
            "values",
            "conflicts",
            "effective",
            "winner",
            "contributors",
        ],
    ))?;
    // The provenance view is the mirror image: it must carry the shadowed
    // contributor that `config eval` dropped.
    check(expect_stdout_mentions(&sources, "personal-wins"))?;
    check(expect_stdout_mentions(&sources, "team-shadowed"))?;

    // A literal personal path in a shared layer fails validation before the build.
    write_piece(
        &tp,
        "team",
        r#"{ ... }: {
    vivarium.mounts = [
        {
            source = "/home/alice/.config/team";
            target = "~/.config/team";
            readonly = true;
        }
    ];
}
"#,
    )?;
    let invalid = viv(&tp, &["config", "eval", "--json"])?;
    // TODO(spec): pin the validating command and its exit code.
    check(expect_nonzero(&invalid))?;
    check(expect_stderr_mentions(&invalid, "/home/alice"))?;

    workflow_03_tie(&tp)
}

/// The equal-priority tie, built the way spec/01's own worked example builds it: one
/// `resources.mem_mib` set at normal priority by a piece and by the manifest leaf.
///
/// TODO(spec): two unspecified things meet here. (1) The piece-side option path for
/// `[resources]` is not named anywhere — ADR-0021 names only `vivarium.mounts` and
/// `vivarium.env` — so `vivarium.resources.mem_mib` is this fixture's guess. (2) spec/01
/// specifies a tie as exit `0` with an order-resolved winner, while spec/04 says the
/// NixOS module system does the merging with no separate vivarium engine — under which
/// an equal-priority scalar conflict is an evaluation error (`70`), not an ordered win.
/// Those two need reconciling before this assertion can be trusted.
fn workflow_03_tie(tp: &TempProject) -> Result<(), Failed> {
    arrange_manifest(
        tp,
        "shared-personal-tie",
        "pieces = [ \"tie\" ]\n",
        "\n[resources]\nmem_mib = 8192\n",
    )?;
    write_piece(
        tp,
        "tie",
        "{ ... }: { vivarium.resources.mem_mib = 4096; }\n",
    )?;
    bind(tp, "shared-personal-tie")?;

    let tie = viv(tp, &["config", "sources", "--json"])?;
    check(expect_code(&tie, 0))?;
    check(expect_json_keys(&tie, &["conflicts"]))?;
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
            "mem_mib",
            "vcpu",
            "egress",
            "mode",
            "allow",
            "extends",
        ],
    ))?;
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
    check(expect_json_keys(
        &list,
        &["manifests", "name", "path", "image", "pieces"],
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
        &["sandbox", "egress", "mode", "allow"],
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
    // TODO(spec): pin the denial surface when the networking backend lands. Enforcement
    // is host-side by design, and no exit code is specified for a blocked connection —
    // after guest-process start `exec` returns the guest program's own status verbatim.
    // Asserting only non-zero keeps this to the guest program's own pass-through status
    // rather than inventing a vivarium code, while refusing a fetch that quietly
    // succeeded. The one thing the spec does require is that a denial be legible.
    check(expect_nonzero(&denied))?;
    if denied.stdout.is_empty() && denied.stderr.is_empty() {
        return fail("egress denial was a silent timeout with no legible message");
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
    // TODO(spec): assert the volume list --json envelope once it is specified.
    check(expect_stdout_mentions(&volumes, "default"))?;
    check(expect_stdout_mentions(&volumes, "cache"))?;
    check(expect_volume_image(&tp, "volume-project", "default"))?;
    check(expect_volume_image(&tp, "volume-project", "cache"))?;

    check(expect_code(&viv(&tp, &["stop"])?, 0))?;
    check(expect_code(&viv(&tp, &["stop"])?, 0))?;
    let status = viv(&tp, &["status", "--json"])?;
    check(expect_code(&status, 0))?;
    check(expect_stdout_mentions(&status, "built"))?;

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
        .map_err(|reason| Failed::from(format!("gate unmet but VIVARIUM_TEST_REQUIRE=1: {reason}")))
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
