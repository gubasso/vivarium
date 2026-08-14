//! The teardown boundary: what `viv destroy` removes, and what it spares.
//!
//! Held apart from `lifecycle`, whose job is the three verbs that bring a VM up, report on it, and
//! bring it down. This one removes data across four roots and then clears an identity, and the
//! spared list below is a contract in its own right — a file whose whole job is that boundary can
//! state it once, where the next edit has to read it.
//!
//! spec/10 fixes the order and it is not incidental: the VM comes down first, because unlinking a
//! generation's GC root beside a running guest would expose that guest's own closure to the next
//! collection. The removals then run under the per-target lock, and the identity teardown after it
//! is released — never both at once, which is how ADR-0053's lock order is kept without this file
//! becoming a link in it.

use std::path::{Path, PathBuf};

use super::grammar::Output;
use super::{Context, Failure, Success, diagnosed, lifecycle};
use crate::config::{self, Environment};
use crate::diagnostic::{Locus, Namespace};
use crate::exit::ExitKind;

/// `viv destroy` (spec/10, ADR-0043, ADR-0080).
pub(super) fn destroy<E: Environment>(
    context: &Context<'_, E>,
    keep_volumes: bool,
    yes: bool,
    output: Output,
) -> Result<Success, Failure> {
    // No binding is required, and that is deliberate rather than an omission. spec/14's `destroy`
    // row says so in its notes while the neighbouring `volume prune` row makes an unresolved
    // binding its own `78`; spec/15 leaves the binding untouched, which makes it irrelevant to
    // teardown; and everything removed below is keyed by `<project-id>`, which resolves from the
    // marker and the index with no manifest at all. A project whose manifest was deleted is
    // exactly one that still needs destroying. The `78` this verb can answer is the other kind —
    // a malformed identity index, which is a state defect rather than a missing manifest.
    //
    // Resolved, never minted: spec/14 keeps persistence to `start` and a cold-starting session,
    // and a `destroy` that minted an identity in order to erase it would leave a marker behind on
    // a project that never had one.
    let project_id = config::resolve_identity(&context.roots.state, &context.project)
        .map_err(|error| super::registry_failure(&error))?;
    let runtime_root = config::resolve_runtime_root(context.environment, config::effective_uid())
        .map_err(|error| super::resolution_failure(&error))?;
    let runtime = lifecycle::Runtime::locate(&runtime_root, &project_id, super::DEFAULT_TARGET);

    let plan = Plan::new(&context.roots, &project_id, keep_volumes);

    if !yes && !confirm(context, &project_id, &plan)? {
        return Ok(Success {
            stdout: String::new(),
            notes: "nothing was destroyed\n".to_owned(),
        });
    }

    // The VM comes down before anything is unlinked (spec/10), and only when there is one: a
    // project that never started, or one already stopped, skips straight to the removals and the
    // whole verb is a no-op that exits `0`.
    let built = lifecycle::last_build(&context.roots, &project_id, super::DEFAULT_TARGET).is_some();
    let lock = lifecycle::TargetLock::acquire(&runtime)?;
    if !matches!(
        lifecycle::discriminate(&runtime, built),
        lifecycle::State::Absent | lifecycle::State::Built
    ) {
        // The same ladder `viv stop` walks, at the default grace. Not `--force`: a destroy that
        // killed the guest where it stood would lose the writes spec/10's shutdown transaction
        // exists to commit, and the volumes are usually about to be removed but not always.
        lifecycle::stop_unit(&runtime, false, None)?;
    }

    // Under the lock, which is what the module header promises and what a `viv start` racing this
    // teardown depends on. `start` takes this same lock before it provisions a volume or boots,
    // and releases it once the guest is up — so a removal that ran outside it could delete the
    // images of a VM that had already started, or leave one running with its build records gone.
    for path in plan.removals()? {
        remove_tree(&path)?;
    }

    // The runtime directory last, and its lock file survives. `flock` binds to an inode, so
    // unlinking the pathname while still holding the lock would end the mutual exclusion rather
    // than release it: a racing `viv start` would create a fresh inode at the same name and take
    // an uncontended lock on it beside this very removal. `stop`'s post-condition already spares
    // the lock for that reason, so what destroy leaves behind is one directory holding one
    // zero-byte mutex — which the next `start` reuses rather than notices.
    clear_runtime(&runtime)?;
    drop(lock);

    // Last, and outside the per-target lock: ADR-0053 orders identity ahead of the per-target
    // `flock`, so taking the identity guard while still holding that one would acquire upward.
    // Every acquisition in this program is `try_lock` and nothing waits on one while holding the
    // other, so releasing first is sufficient and no cycle can form.
    config::forget_identity(&context.roots.state, &context.project)
        .map_err(|error| super::registry_failure(&error))?;

    // spec/10 gives `destroy` empty stdout on success and one record under `--json`. The record is
    // not written: no page fixes its shape, and `stop` — which spec/10 binds by the same sentence —
    // does not emit one either, so inventing one here would settle a two-command contract from
    // inside one of them. Named in `docs/reference/implementation-status.md` rather than left for a
    // caller to discover by piping an empty stdout into `jq`.
    let _ = output;
    Ok(Success::plain(String::new()))
}

/// Removes everything in the runtime directory except the startup lock.
///
/// The carve-out is the same one `viv stop`'s post-condition makes, and for the same reason: the
/// lock is the per-target mutex rather than an artifact of the VM, and its pathname is what
/// provides the exclusion. See the call site for why this runs while the lock is still held.
fn clear_runtime(runtime: &lifecycle::Runtime) -> Result<(), Failure> {
    let lock = runtime.lock();
    let entries = match std::fs::read_dir(&runtime.directory) {
        Ok(entries) => entries,
        // Never started, or already swept by the supervisor's own teardown.
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(read_failure(&runtime.directory, &source)),
    };
    for entry in entries {
        let entry = entry.map_err(|source| read_failure(&runtime.directory, &source))?;
        if entry.path() != lock {
            remove_tree(&entry.path())?;
        }
    }
    Ok(())
}

/// What one `destroy` will remove, and what it will not.
///
/// A value rather than a sequence of `remove_dir_all` calls, so the spared half is testable
/// without a VM. That half is the part a later edit will quietly break: nothing fails when a
/// `flake.lock` is deleted, it just re-resolves inputs on the next build and the pin ADR-0059 put
/// under the data root is gone.
struct Plan {
    /// Paths removed whole.
    remove: Vec<PathBuf>,
    /// Directories whose children are removed except for one name kept by `--keep-volumes`.
    ///
    /// Two levels rather than one because spec/15 removes the project's state, not one target's:
    /// with volumes kept, every other target still goes and so does everything beside `volumes/`
    /// in this one. Enumerating the children at run time rather than naming them is what keeps
    /// this correct when a later slice adds a per-project Nix profile beside them — an
    /// exception list would have to be edited then, and nothing would say so.
    keep_within: Vec<(PathBuf, &'static str)>,
    /// Everything this destroy leaves alone, for the prompt and for the test.
    spared: Vec<PathBuf>,
}

impl Plan {
    fn new(roots: &config::XdgRoots, project_id: &str, keep_volumes: bool) -> Self {
        let project_state = roots.state.join("projects").join(project_id);
        let target_state = project_state.join(super::DEFAULT_TARGET);
        let data_target = roots
            .data
            .join("projects")
            .join(project_id)
            .join(super::DEFAULT_TARGET);

        // The build records go individually rather than with their directory, because their
        // sibling is the lockfile ADR-0059 put under the data root precisely so this verb spares
        // it. Removing the parent would be one line shorter and would discard the pin.
        let mut remove = vec![
            data_target.join("last-build"),
            data_target.join("running-build"),
        ];
        let mut keep_within = Vec::new();
        if keep_volumes {
            keep_within.push((target_state.clone(), "volumes"));
            keep_within.push((project_state, super::DEFAULT_TARGET));
        } else {
            remove.push(project_state);
        }

        let mut spared = vec![
            // Deleting the pin would make the next build re-resolve every input, which is the
            // determinism failure N3 forbids — and `destroy` promises at most a rebuild.
            data_target.join("flake.lock"),
            // ADR-0058 makes the generated tree cache, regenerable from the manifest.
            roots.cache.join("flakes").join(project_id),
            // spec/15: the binding survives, which is what makes the next `viv start` a rebuild
            // rather than a re-bind.
            config::registry_path(&roots.state),
        ];
        if keep_volumes {
            spared.push(target_state.join("volumes"));
        }

        Self {
            remove,
            keep_within,
            spared,
        }
    }

    /// Every path this plan removes, with the `--keep-volumes` carve-outs already expanded.
    ///
    /// Reads the directories it prunes, so it answers what is actually there rather than what a
    /// list in this file happens to name.
    fn removals(&self) -> Result<Vec<PathBuf>, Failure> {
        let mut paths = self.remove.clone();
        for (directory, keep) in &self.keep_within {
            match std::fs::read_dir(directory) {
                Ok(entries) => {
                    for entry in entries {
                        let entry = entry.map_err(|source| read_failure(directory, &source))?;
                        if entry.file_name() != *keep {
                            paths.push(entry.path());
                        }
                    }
                }
                // Nothing there is the idempotent path, not a failure.
                Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
                Err(source) => return Err(read_failure(directory, &source)),
            }
        }
        paths.sort();
        Ok(paths)
    }

    /// What the prompt shows, which is the plan as authored rather than as expanded.
    fn scope(&self) -> Vec<String> {
        self.remove
            .iter()
            .map(|path| path.display().to_string())
            .chain(
                self.keep_within
                    .iter()
                    .map(|(directory, keep)| format!("{} (except {keep})", directory.display())),
            )
            .collect()
    }
}

fn confirm<E: Environment>(
    context: &Context<'_, E>,
    project_id: &str,
    plan: &Plan,
) -> Result<bool, Failure> {
    let mut scope = plan.scope();
    scope.push(format!(
        "the identity marker in {}",
        context.project.display()
    ));
    let question = super::prompt::Question {
        headline: &format!("destroy `{project_id}` and everything above?"),
        scope,
        spared: plan
            .spared
            .iter()
            .map(|path| path.display().to_string())
            .chain(std::iter::once("the workspace itself".to_owned()))
            .collect(),
    };
    super::prompt::confirm(
        &question,
        &mut std::io::stdin().lock(),
        &mut std::io::stderr().lock(),
    )
    .map_err(|source| {
        diagnosed(
            Namespace::Host,
            "prompt-unavailable",
            "could not ask for confirmation",
            Locus::Named("terminal"),
            source.to_string(),
            ExitKind::IoErr,
        )
    })
}

/// Removes a file or a directory, treating absence as done.
///
/// Absence is the idempotent path spec/10 requires: a second `destroy` finds nothing and still
/// exits `0`, which is exactly what a first one leaves behind.
fn read_failure(path: &Path, source: &std::io::Error) -> Failure {
    diagnosed(
        Namespace::State,
        "teardown-unreadable",
        "could not read what this project owns",
        Locus::File(path.to_path_buf()),
        source.to_string(),
        ExitKind::IoErr,
    )
}

fn remove_tree(path: &Path) -> Result<(), Failure> {
    let result = if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    match result {
        Ok(()) => Ok(()),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => {
            // spec/14 splits these: a permission denial is `77`, every other channel failure `74`.
            let code = if source.kind() == std::io::ErrorKind::PermissionDenied {
                ExitKind::NoPerm
            } else {
                ExitKind::IoErr
            };
            Err(diagnosed(
                Namespace::State,
                "teardown-removal",
                "could not remove what this project owns",
                Locus::File(path.to_path_buf()),
                source.to_string(),
                code,
            ))
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::test_support::ScratchDirectory;

    fn roots() -> config::XdgRoots {
        config::XdgRoots {
            config: PathBuf::from("/c"),
            state: PathBuf::from("/s"),
            data: PathBuf::from("/d"),
            cache: PathBuf::from("/k"),
        }
    }

    #[test]
    fn the_state_subtree_goes_and_the_pin_the_binding_and_the_cache_stay() {
        // The highest-value assertion in this file: the spared list is what a future edit breaks
        // silently, because removing one of these three fails no test that does not name it.
        let plan = Plan::new(&roots(), "demo", false);
        let removals: Vec<String> = plan
            .removals()
            .unwrap()
            .iter()
            .map(|path| path.display().to_string())
            .collect();
        assert!(
            removals.contains(&"/s/projects/demo".to_owned()),
            "{removals:?}"
        );
        assert!(
            removals.contains(&"/d/projects/demo/default/last-build".to_owned()),
            "{removals:?}"
        );
        assert!(
            removals.contains(&"/d/projects/demo/default/running-build".to_owned()),
            "{removals:?}"
        );

        let spared: Vec<String> = plan
            .spared
            .iter()
            .map(|path| path.display().to_string())
            .collect();
        assert!(
            spared.contains(&"/d/projects/demo/default/flake.lock".to_owned()),
            "{spared:?}"
        );
        assert!(spared.contains(&"/k/flakes/demo".to_owned()), "{spared:?}");
        assert!(
            spared.contains(&"/s/registry.toml".to_owned()),
            "{spared:?}"
        );

        // And nothing spared may also be removed. A prefix test rather than equality, because the
        // way this breaks is a plan that removes a spared path's parent: `/d/projects/demo` would
        // take the lockfile with it and no equality check would notice.
        assert_spared(&plan);
    }

    /// Nothing in the spared list may lie under anything in the removal list.
    fn assert_spared(plan: &Plan) {
        for spare in &plan.spared {
            for removal in plan.removals().unwrap() {
                assert!(
                    !spare.starts_with(&removal),
                    "{} is removed by {}",
                    spare.display(),
                    removal.display()
                );
            }
        }
    }

    #[test]
    fn keep_volumes_spares_the_images_and_still_unlinks_the_build() {
        let plan = Plan::new(&roots(), "demo", true);
        let removals: Vec<String> = plan
            .removals()
            .unwrap()
            .iter()
            .map(|path| path.display().to_string())
            .collect();
        // The project's state subtree is never removed whole under this flag, because the images
        // are inside it.
        assert!(
            !removals.contains(&"/s/projects/demo".to_owned()),
            "{removals:?}"
        );
        assert!(
            plan.spared
                .iter()
                .any(|path| path.ends_with("projects/demo/default/volumes")),
            "{:?}",
            plan.spared
        );
        // The flag is about data, not about generations: a warm destroy is still a cold rebuild.
        assert!(
            removals.contains(&"/d/projects/demo/default/last-build".to_owned()),
            "{removals:?}"
        );
        assert_spared(&plan);
    }

    #[test]
    fn keeping_volumes_removes_every_sibling_it_finds_and_nothing_else() {
        // The carve-out is enumerated from the directory rather than from a list here, so a state
        // file a later slice adds beside the images is removed without this file being edited.
        // That is the property worth pinning: an exception list would silently retain it.
        let scratch = ScratchDirectory::new().unwrap();
        let root = scratch.path().to_path_buf();
        let target = root.join("projects/demo/default");
        std::fs::create_dir_all(target.join("volumes")).unwrap();
        std::fs::write(target.join("volumes/default.img"), b"data").unwrap();
        std::fs::write(target.join("volumes.toml"), b"image = \"x\"\n").unwrap();
        std::fs::write(target.join("something-a-later-slice-added"), b"x").unwrap();

        let roots = config::XdgRoots {
            config: PathBuf::from("/c"),
            state: root,
            data: PathBuf::from("/d"),
            cache: PathBuf::from("/k"),
        };
        let removals = Plan::new(&roots, "demo", true).removals().unwrap();
        assert!(
            removals.contains(&target.join("volumes.toml")),
            "{removals:?}"
        );
        assert!(
            removals.contains(&target.join("something-a-later-slice-added")),
            "{removals:?}"
        );
        assert!(!removals.contains(&target.join("volumes")), "{removals:?}");
    }
}
