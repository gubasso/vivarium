//! The `eval` lane: the workflow trials whose evidence is a Nix evaluation.
//!
//! What it proves is that a selected manifest composes to a generated flake and
//! that the flake evaluates — the merge, the provenance, the content defects, the
//! moved pin. Nothing here realises a derivation and nothing boots.
//!
//! The lane declares `nix` on `PATH` and nothing else, and it declares it once, in
//! `main`. A host without Nix fails every trial with that reason rather than
//! reporting a green run over trials that did not happen.
//!
//! Its siblings are `local_workflows` (needs nothing) and `boot_workflows` (needs a
//! guest). The lane register is `docs/reference/testing-lanes.md`.

mod support;

use std::fs;
use std::path::{Path, PathBuf};

use libtest_mimic::{Arguments, Failed, Trial};
use serde_json::Value;
use support::{
    EX_CONFIG, EX_DATAERR, EX_SOFTWARE, TempProject, arrange_egress_fixture, arrange_manifest,
    check, expect_code, expect_json_array_items, expect_json_array_nonempty, expect_json_fields_at,
    expect_json_keys, expect_json_map_entries, expect_no_volume_images, expect_soft_check,
    expect_stderr_mentions, expect_stdout_lacks, expect_stdout_mentions, fail, io_failed,
    preflight, viv, viv_with_env, write_file, write_piece,
};

fn main() -> std::process::ExitCode {
    let args = Arguments::from_args();
    // After argument parsing and never before it: nextest builds its test list by
    // running this binary with `--list`, and a listing that refuses on an unmet
    // need would abort the whole run rather than failing this lane's own trials.
    if !args.list
        && let Err(reason) = preflight::nix()
    {
        eprintln!("the eval lane needs Nix: {reason}");
        return std::process::ExitCode::from(69);
    }
    let trials = vec![
        Trial::test(
            "eval_bound_manifest_evaluates",
            eval_bound_manifest_evaluates,
        ),
        Trial::test("workflow_03_team_shared_and_personal_override", workflow_03),
        Trial::test("workflow_04_inspect_before_run", workflow_04_eval),
        Trial::test(
            "workflow_05_restrict_egress_config_surface",
            workflow_05_config,
        ),
        Trial::test("workflow_17_declared_mounts_eval", workflow_17_eval),
        Trial::test("workflow_18_update_moves_the_pin", workflow_18_update_pin),
        Trial::test(
            "workflow_23_agent_channel_config_surface",
            workflow_23_config,
        ),
        Trial::test(
            "a_new_target_takes_the_shipped_pin",
            a_new_target_takes_the_shipped_pin,
        ),
        Trial::test("a_base_keeps_its_own_nixpkgs", a_base_keeps_its_own_nixpkgs),
        Trial::test(
            "the_pre_flight_leaves_no_residue",
            the_pre_flight_leaves_no_residue,
        ),
        Trial::test(
            "a_failed_pre_flight_leaves_no_lock",
            a_failed_pre_flight_leaves_no_lock,
        ),
        Trial::test(
            "a_failed_evaluation_keeps_the_created_pin",
            a_failed_evaluation_keeps_the_created_pin,
        ),
    ];
    libtest_mimic::run(&args, trials).exit_code()
}

/// That this host evaluates a bound manifest end to end, and that an unbound one
/// still fails closed.
///
/// This was the `ConfigEval` gate probe, and promoting it to a trial is the whole
/// point of the lane split. As a probe it decided whether the trials behind it ran,
/// so a host that could not evaluate reported a green run of trials that never
/// happened. As a trial it reports the same fact as a red.
///
/// Two halves, because they fail for different reasons and must not arrive as one
/// red. The unbound half proves the verb exists and fails closed with `78`; anything
/// else means the verb is not there to fail closed. The bound half proves this host
/// can reach the flake inputs an evaluation resolves — the expensive question, and
/// nothing cheaper separates "cannot evaluate" from "evaluates wrongly".
fn eval_bound_manifest_evaluates() -> Result<(), Failed> {
    let tp = TempProject::new().map_err(io_failed)?;

    let unbound = viv(&tp, &["config", "eval", "--json"])?;
    check(expect_code(&unbound, EX_CONFIG))?;

    let library = tp.config().join("vivarium");
    write_file(&library.join("images").join("probe.nix"), "{ ... }: { }\n").map_err(io_failed)?;
    write_file(
        &library.join("manifests").join("probe.toml"),
        &format!(
            "image = \"probe\"\n\n[[workspaces]]\nsource = '{}'\n",
            tp.project().display()
        ),
    )
    .map_err(io_failed)?;

    let bound = viv(&tp, &["config", "eval", "--json"])?;
    if bound.status.code() != Some(0) {
        return fail(format!(
            "this host cannot evaluate a bound manifest (exited {:?}): {}",
            bound.status.code(),
            String::from_utf8_lossy(&bound.stderr).trim()
        ));
    }
    Ok(())
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
    let url = format!("path:{}", leaf.display()); // nix-path-ok: a throwaway fixture folder
    write_file(
        &images.join("flake.nix"),
        &format!(
            "{{ inputs.nixpkgs.url = \"{url}\";\n  \
            outputs = {{ self, nixpkgs }}: {{ }}; }}\n"
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

    // Only the base is named. A first update with no names would move every input, and the
    // shipped `microvm` reference would then resolve live from its branch.
    let first = viv(&tp, &["update", "my-base", "--json"])?;
    check(expect_code(&first, 0))?;
    check(expect_json_keys(&first, &["manifest", "lock", "inputs"]))?;
    check(expect_stderr_mentions(&first, "created the pin"))?;
    // The pre-flight wrote the lock before the update ran, so `before` reads that lock. The
    // base row stays `null` because a relative base node has no display pin (ADR-0112).
    let base_row = update_row(&first, "my-base")?;
    if !base_row["before"].is_null() {
        return fail(format!("the base row has a display pin: {base_row}"));
    }
    let owned = owned_lock(&tp, "update-pin");
    if !owned.is_file() {
        return Err(Failed::from(
            "the first update did not create the owned lock",
        ));
    }

    // The repeat: the base's `nixpkgs` is a local leaf and nothing upstream moved, so every row
    // is `changed: false`, the requested row included, and the run is `0`.
    let second = viv(&tp, &["update", "nixpkgs", "--json"])?;
    check(expect_code(&second, 0))?;
    check(expect_stdout_mentions(&second, "\"changed\":false"))?;
    check(expect_stdout_lacks(&second, "\"changed\":true"))?;
    // The requested row carries the user's own string, translation or not (spec/01), and a
    // node with a display pin reports one before as after.
    let nixpkgs_row = update_row(&second, "nixpkgs")?;
    if nixpkgs_row["before"].is_null() || nixpkgs_row["before"] != nixpkgs_row["after"] {
        return fail(format!("the unchanged nixpkgs row reads: {nixpkgs_row}"));
    }

    // No staged residue beside the owned lock: the update's temp file is consumed by the
    // rename or removed on failure, never left for the next reader to misread.
    expect_lock_alone(&owned)
}

/// The row `name` of an update's JSON record.
fn update_row(out: &support::VivOutput, name: &str) -> Result<Value, Failed> {
    let record: Value = serde_json::from_slice(&out.stdout)
        .map_err(|error| Failed::from(format!("the update printed no JSON: {error}")))?;
    record["inputs"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["name"] == name))
        .cloned()
        .ok_or_else(|| Failed::from(format!("the update reported no `{name}` row: {record}")))
}

/// The tool-owned lock of the default target of manifest `name`.
fn owned_lock(tp: &TempProject, name: &str) -> PathBuf {
    tp.data()
        .join("vivarium")
        .join("projects")
        .join(name)
        .join("default")
        .join("flake.lock")
}

/// That the owned lock's directory holds `flake.lock` and nothing else: the pre-flight's
/// reference and staged result, and an update's staged output, are consumed or removed.
fn expect_lock_alone(owned: &Path) -> Result<(), Failed> {
    let entries = lock_directory_entries(owned)?;
    if entries != ["flake.lock"] {
        return fail(format!("the owned lock's directory holds: {entries:?}"));
    }
    Ok(())
}

/// Every name in the owned lock's directory, sorted; empty when the directory does not exist.
fn lock_directory_entries(owned: &Path) -> Result<Vec<String>, Failed> {
    let directory = owned.parent().ok_or("owned lock parent")?;
    let mut entries: Vec<String> = match fs::read_dir(directory) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(io_failed(error)),
    };
    entries.sort();
    Ok(entries)
}

/// Reads a lock file as JSON.
fn read_lock(path: &Path) -> Result<Value, Failed> {
    let bytes = fs::read(path).map_err(io_failed)?;
    serde_json::from_slice(&bytes)
        .map_err(|error| Failed::from(format!("{} is no JSON lock: {error}", path.display())))
}

/// The pin this repository ships, which the binary under test compiled in.
fn shipped_lock() -> Result<Value, Failed> {
    read_lock(&Path::new(env!("CARGO_MANIFEST_DIR")).join("nix/flake.lock"))
}

/// The node a path of input names reaches from the root, following `follows` edges.
fn node_at(lock: &Value, path: &[&str]) -> Option<String> {
    let mut current = lock["root"].as_str()?.to_owned();
    for segment in path {
        current = match &lock["nodes"][&current]["inputs"][*segment] {
            Value::String(label) => label.clone(),
            Value::Array(follows) => {
                let follows: Vec<&str> =
                    follows.iter().map(Value::as_str).collect::<Option<_>>()?;
                node_at(lock, &follows)?
            }
            _ => return None,
        };
    }
    Some(current)
}

/// The `locked` field of the node a path of input names reaches.
fn locked_at(lock: &Value, path: &[&str]) -> Result<Value, Failed> {
    let node = node_at(lock, path)
        .ok_or_else(|| Failed::from(format!("the lock reaches no node at {path:?}")))?;
    Ok(lock["nodes"][&node]["locked"].clone())
}

/// That `lock` pins the input at each path exactly as the shipped pin does.
fn expect_shipped_nodes(lock: &Value, paths: &[&[&str]]) -> Result<(), Failed> {
    let shipped = shipped_lock()?;
    for path in paths {
        let (ours, theirs) = (locked_at(lock, path)?, locked_at(&shipped, path)?);
        if ours != theirs {
            return fail(format!(
                "the node at {path:?} is {ours}, and the shipped pin has {theirs}"
            ));
        }
    }
    Ok(())
}

/// That a target with no lock gets the pin the binary ships as its first lock, announced.
///
/// The exact path of a new user: a fresh data root, no variable, and the pre-flight writing
/// `nix/flake.lock` out through Nix's lock step (ADR-0116).
fn a_new_target_takes_the_shipped_pin() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("shipped-pin").map_err(io_failed)?;
    arrange_manifest(&tp, "shipped-pin", "", "")?;
    let eval = viv(&tp, &["config", "eval", "--json"])?;
    check(expect_code(&eval, 0))?;
    check(expect_stderr_mentions(&eval, "created the pin"))?;
    let lock = read_lock(&owned_lock(&tp, "shipped-pin"))?;
    expect_shipped_nodes(
        &lock,
        &[&["nixpkgs"], &["microvm"], &["microvm", "spectrum"]],
    )
}

/// That a base flake's own `nixpkgs` survives the shipped reference (ADR-0112).
///
/// The reference lock holds no node under the base, so the base's declaration decides its
/// `nixpkgs`, while `microvm` and `spectrum` still come from the shipped pin. The leaf has no
/// outputs, so the evaluation after the pre-flight fails; only the lock is read.
fn a_base_keeps_its_own_nixpkgs() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("own-nixpkgs").map_err(io_failed)?;
    let images = tp.config().join("vivarium").join("images").join("my-base");
    write_file(&images.join("default.nix"), "{ ... }: { }\n").map_err(io_failed)?;
    let leaf = tp.root().join("own-nixpkgs-leaf");
    write_file(&leaf.join("flake.nix"), "{ outputs = { self }: { }; }\n").map_err(io_failed)?;
    let url = format!("path:{}", leaf.display()); // nix-path-ok: a throwaway fixture folder
    write_file(
        &images.join("flake.nix"),
        &format!(
            "{{ inputs.nixpkgs.url = \"{url}\";\n  outputs = {{ self, nixpkgs }}: {{ }}; }}\n"
        ),
    )
    .map_err(io_failed)?;
    write_file(
        &tp.config()
            .join("vivarium")
            .join("manifests")
            .join("own-nixpkgs.toml"),
        &format!(
            "image = \"my-base\"\n\n[[workspaces]]\nsource = '{}'\n",
            tp.project().display()
        ),
    )
    .map_err(io_failed)?;
    let eval = viv(&tp, &["config", "eval", "--json"])?;
    check(expect_stderr_mentions(&eval, "created the pin"))?;
    let lock = read_lock(&owned_lock(&tp, "own-nixpkgs"))?;
    let nixpkgs = locked_at(&lock, &["nixpkgs"])?;
    let leaf_path = leaf.display().to_string();
    if nixpkgs["type"] != "path" || nixpkgs["path"].as_str() != Some(leaf_path.as_str()) {
        return fail(format!("the base's nixpkgs is not its own leaf: {nixpkgs}"));
    }
    expect_shipped_nodes(&lock, &[&["microvm"], &["microvm", "spectrum"]])
}

/// That the pre-flight leaves nothing beside the lock it wrote.
fn the_pre_flight_leaves_no_residue() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("no-residue").map_err(io_failed)?;
    arrange_manifest(&tp, "no-residue", "", "")?;
    let eval = viv(&tp, &["config", "eval", "--json"])?;
    check(expect_code(&eval, 0))?;
    expect_lock_alone(&owned_lock(&tp, "no-residue"))
}

/// That a pre-flight Nix cannot complete installs no lock and leaves no temporary file.
fn a_failed_pre_flight_leaves_no_lock() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("failed-pre-flight").map_err(io_failed)?;
    let images = tp.config().join("vivarium").join("images").join("my-base");
    write_file(&images.join("default.nix"), "{ ... }: { }\n").map_err(io_failed)?;
    let missing = tp.root().join("no-such-folder");
    let url = format!("path:{}", missing.display()); // nix-path-ok: a missing fixture folder
    write_file(
        &images.join("flake.nix"),
        &format!("{{ inputs.gone.url = \"{url}\";\n  outputs = {{ self, gone }}: {{ }}; }}\n"),
    )
    .map_err(io_failed)?;
    write_file(
        &tp.config()
            .join("vivarium")
            .join("manifests")
            .join("failed-pre-flight.toml"),
        &format!(
            "image = \"my-base\"\n\n[[workspaces]]\nsource = '{}'\n",
            tp.project().display()
        ),
    )
    .map_err(io_failed)?;
    let eval = viv(&tp, &["config", "eval", "--json"])?;
    check(expect_code(&eval, EX_SOFTWARE))?;
    check(expect_stderr_mentions(&eval, "lock.preflight-failed"))?;
    let entries = lock_directory_entries(&owned_lock(&tp, "failed-pre-flight"))?;
    if !entries.is_empty() {
        return fail(format!("a failed pre-flight left: {entries:?}"));
    }
    Ok(())
}

/// That a lock which resolves but does not evaluate stays, announced once (ADR-0116).
fn a_failed_evaluation_keeps_the_created_pin() -> Result<(), Failed> {
    let tp = TempProject::with_project_name("kept-pin").map_err(io_failed)?;
    // A parse error, which the lock step never reads and the evaluation cannot catch.
    support::arrange_manifest_with_image(&tp, "kept-pin", "", "", "{ ... }: {\n")?;
    let eval = viv(&tp, &["config", "eval", "--json"])?;
    if expect_code(&eval, 0).is_ok() {
        return fail("an image that does not parse evaluated");
    }
    let stderr = String::from_utf8_lossy(&eval.stderr);
    let announced = stderr.matches("created the pin").count();
    if announced != 1 {
        return fail(format!(
            "the created pin was announced {announced} times: {stderr}"
        ));
    }
    expect_lock_alone(&owned_lock(&tp, "kept-pin"))
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
    // The report's own entry, not the process exit code. `viv doctor` exits `78`
    // on any failing hard probe, so reading the code here made this trial depend
    // on the host's virtualization support to assert what a channel declaration
    // puts in a report. Measured 2026-09-09 on a GitHub runner, where
    // `host-userns-available` trips and this failed naming neither.
    //
    // The severity is asserted rather than implied. The old exit-code reading
    // caught a probe turned hard, because a hard failure is what makes doctor
    // exit `78`, and spec/13 fixes this one as soft with its reasoning: a user
    // who starts a VM to run tests needing no key has legitimately waived it.
    // Finding the row by id alone would let that contract move unnoticed.
    check(expect_soft_check(&doctor, "agent-source-usable"))?;
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
    check(expect_soft_check(&extends_doctor, "agent-source-usable"))?;
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
