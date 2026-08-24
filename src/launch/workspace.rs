//! What a declared workspace may be, decided before anything is built.
//!
//! Since `ADR-0110` a workspace compiles to an ordinary declared mount whose `target` equals its
//! `source`, so nothing downstream of the generated flake can tell one from any other mount. Every
//! rule that applies to a workspace and not to a mount therefore has to be decided here, ahead of
//! evaluation, or it is not decided at all.
//!
//! The refusals are the ones `ADR-0100` named and `ADR-0108` made pairwise. What moved with
//! `ADR-0110` is only when they fire: from launch, against a built contract, to resolution,
//! against manifest text and the host environment. That is the input `ADR-0109` already decides
//! ownership from, which is why these stay `78` rather than becoming a merged-configuration
//! defect.

use std::path::{Path, PathBuf};

use super::mounts::{MountSourceDefect, expand_source};

/// Guest paths the image owns, which a workspace may neither occupy nor contain.
///
/// The list exists so the refusal carries a diagnostic before anything is evaluated, and so a
/// generation whose `/etc` is not a mount of its own still refuses `/etc/...`. Since `ADR-0110`
/// the guest also refuses these as ordinary mount targets, from `guestOwnedTargetRoots` in
/// `nix/guest.nix`, which the contract check holds against this list.
///
/// Two whole subtrees are deliberately absent, and both for the same reason: a host keeps real
/// projects under them, so a blanket denial refuses ordinary work. `/var`, because ostree-based
/// hosts put home directories at `/var/home/<user>`. And `/run`, because a removable drive is
/// mounted at `/run/media/<user>/<label>` on an ordinary desktop — measured, by this repository's
/// own acceptance fixtures, which live on exactly such a drive and were refused by the first
/// version of this list. What the guest owns under those two roots is named individually instead.
pub const GUEST_OWNED_PATHS: &[&str] = &[
    "/nix",
    "/proc",
    "/sys",
    "/dev",
    "/etc",
    "/boot",
    "/usr",
    "/bin",
    "/sbin",
    "/lib",
    "/lib64",
    "/root",
    "/tmp",
    "/var/lib",
    "/var/log",
    "/var/tmp",
    "/var/empty",
    // The guest's own `/run` names. `/run/vivarium` is the agent's `RuntimeDirectory`, and
    // `/run/vivarium-mounts` is where every share mounts before the bind unit places it.
    "/run/vivarium",
    "/run/vivarium-mounts",
    "/run/user",
    "/run/current-system",
    "/run/booted-system",
    "/run/wrappers",
    "/run/systemd",
    "/run/udev",
    "/run/dbus",
    "/run/lock",
    "/run/log",
    "/run/keys",
    "/run/credentials",
    "/run/binfmt",
    "/run/nscd",
    "/run/opengl-driver",
    // The guest home, and not a corner case: a host user named `vivarium` keeps their projects
    // under exactly this path, and it is an ext4 volume, so mounting into it would create
    // root-owned directories inside a persistent home the user cannot clear. Refused rather than
    // worked around; moving the guest home is `Q-017` in `docs/plan/open-questions.md`.
    "/home/vivarium",
];

/// Why a host path cannot be a workspace, or `None` when it can.
///
/// Returns the message the refusal is stated with, so the diagnostic and the launch backstop share
/// one vocabulary instead of describing the same rule twice.
#[must_use]
pub fn guest_owned_collision(path: &Path) -> Option<&'static str> {
    if !path.is_absolute() {
        return Some("workspace path must be absolute to be mounted into the guest");
    }
    if path.parent().is_none() {
        return Some("the filesystem root cannot be mounted into the guest");
    }
    for owned in GUEST_OWNED_PATHS {
        let owned = Path::new(owned);
        // `Path::starts_with` compares whole components, so `/nix` does not match
        // `/nixos-projects`. A string prefix would, which is the bug this notes rather than risks.
        if path.starts_with(owned) || owned.starts_with(path) {
            return Some("workspace path collides with a path the guest owns");
        }
    }
    None
}

/// The first equal or nested pair, with both paths preserved for diagnostics.
///
/// Two workspaces where one contains the other cannot both be mounted at their own host paths: the
/// inner bind would land inside the outer share and be shadowed by it, so a session in the inner
/// tree would silently read the outer tree's copy. Refused rather than ordered, because no order
/// makes both promises true.
#[must_use]
pub fn nested_pair<'a>(paths: impl IntoIterator<Item = &'a Path>) -> Option<(&'a Path, &'a Path)> {
    let paths: Vec<_> = paths.into_iter().collect();
    for (index, left) in paths.iter().enumerate() {
        for right in &paths[index + 1..] {
            if left.starts_with(right) || right.starts_with(left) {
                return Some((*left, *right));
            }
        }
    }
    None
}

/// Why a declared workspace is refused, with the declared spelling that produced it.
///
/// The declared string travels with the defect because the refusal reads against what the user
/// wrote rather than against an expansion they never typed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceDefect {
    pub declared: String,
    pub kind: WorkspaceDefectKind,
}

/// One variant per invariant, so each refusal keeps its own diagnostic id.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkspaceDefectKind {
    /// `ADR-0020`'s grammar: an unset variable is refused rather than expanded to nothing.
    UnsetVariable {
        variable: String,
    },
    NotAbsolute {
        expanded: PathBuf,
    },
    Missing {
        expanded: PathBuf,
    },
    /// The host refused to say what the path is; carried verbatim so a permission problem reads as
    /// itself rather than as absence.
    Unreadable {
        expanded: PathBuf,
        error: String,
    },
    /// A workspace is a tree. A regular file, socket, FIFO, or device node is not one, and unlike
    /// a `[[mounts]]` row there is no file-mount shape for it to take.
    NotDirectory {
        expanded: PathBuf,
    },
    /// `ADR-0100`'s refusal set, carrying the reason `guest_owned_collision` stated it with.
    GuestOwned {
        expanded: PathBuf,
        reason: &'static str,
    },
    /// `ADR-0108`'s pairwise rule. Both sides are named because either one could be the edit.
    Nested {
        left: PathBuf,
        right: PathBuf,
    },
}

/// Expand and validate every declared workspace source.
///
/// Returns them in the canonical set order [`canonicalize_set`] fixes, not in declaration order:
/// what the caller does with them — compares them against a boot record, renders them into the
/// generated flake — is set-shaped, and declaration order is the user's business rather than the
/// sandbox's. Refusals are still raised in declaration order, so a user fixes one row at a time in
/// the order they wrote them.
///
/// One list, derived once, reaches both the owner index and the generated flake. Deriving it twice
/// is how the two come to disagree about which trees a manifest declares, and a disagreement there
/// is silent: the sandbox would mount a tree that resolves to no manifest, or resolve a tree it
/// never mounted.
///
/// Paths are canonicalized, because the value written into the flake as both `source` and `target`
/// has to be the value the launch-time classification arrives at. `target == source` is what marks
/// a row as a workspace once it reaches Nix, so a spelling difference would quietly unmake one.
///
/// # Errors
///
/// The first defect in declaration order, so a user fixes one row at a time in the order they
/// wrote them rather than meeting an unordered set.
pub fn resolve(
    declared: &[String],
    lookup: &dyn Fn(&str) -> Option<String>,
) -> Result<Vec<PathBuf>, WorkspaceDefect> {
    let mut resolved: Vec<PathBuf> = Vec::with_capacity(declared.len());
    for source in declared {
        let defect = |kind| WorkspaceDefect {
            declared: source.clone(),
            kind,
        };
        let expanded = expand_source(source, lookup).map_err(|error| match error {
            MountSourceDefect::UnsetVariable { variable } => {
                defect(WorkspaceDefectKind::UnsetVariable { variable })
            }
            // `expand_source` returns no other variant: the rest are classification defects it
            // never reaches. Mapped rather than unwrapped so a later variant cannot arrive here
            // silently as the wrong id.
            other => defect(WorkspaceDefectKind::NotAbsolute {
                expanded: PathBuf::from(format!("{other:?}")),
            }),
        })?;
        let path = PathBuf::from(&expanded);
        if !path.is_absolute() {
            return Err(defect(WorkspaceDefectKind::NotAbsolute { expanded: path }));
        }
        let metadata = match std::fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(defect(WorkspaceDefectKind::Missing { expanded: path }));
            }
            Err(error) => {
                return Err(defect(WorkspaceDefectKind::Unreadable {
                    expanded: path,
                    error: error.to_string(),
                }));
            }
        };
        if !metadata.is_dir() {
            return Err(defect(WorkspaceDefectKind::NotDirectory { expanded: path }));
        }
        // After the existence check, so a missing path reads as missing rather than as an I/O
        // error about a path the user can see is absent.
        let path = path.canonicalize().map_err(|error| {
            defect(WorkspaceDefectKind::Unreadable {
                expanded: path.clone(),
                error: error.to_string(),
            })
        })?;
        if let Some(reason) = guest_owned_collision(&path) {
            return Err(defect(WorkspaceDefectKind::GuestOwned {
                expanded: path,
                reason,
            }));
        }
        resolved.push(path);
    }
    if let Some((left, right)) = nested_pair(resolved.iter().map(PathBuf::as_path)) {
        let (left, right) = (left.to_path_buf(), right.to_path_buf());
        // The pair rule has no single declared spelling to blame, so it names the second row: the
        // one whose addition made the set unsatisfiable, reading in declaration order.
        return Err(WorkspaceDefect {
            declared: declared.last().cloned().unwrap_or_default(),
            kind: WorkspaceDefectKind::Nested { left, right },
        });
    }
    canonicalize_set(&mut resolved);
    Ok(resolved)
}

/// Put a workspace set into the one form every comparison of it is made against.
///
/// A set, not a list: which order a user wrote two disjoint trees in is not a fact about the
/// sandbox, and two spellings of one set must never read as two sets. That matters because the
/// set is compared across a process boundary — the supervisor derives it from the launch
/// specification and writes it to `boot.json`, while resolution derives it from manifest text —
/// and `reusable()` compares those two directly on a path that deliberately evaluates nothing.
/// Both sides call this, which is what makes the comparison sound rather than coincidental.
///
/// The dedup is unreachable through `resolve`, where equal paths are refused as a nested pair
/// before this runs. It is here for the other caller: a launch specification is a file, and a
/// file can name one tree twice.
pub fn canonicalize_set(paths: &mut Vec<PathBuf>) {
    paths.sort();
    paths.dedup();
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::{
        GUEST_OWNED_PATHS, WorkspaceDefectKind, guest_owned_collision, nested_pair, resolve,
    };
    use std::path::{Path, PathBuf};

    #[test]
    fn guest_owned_collision_names_the_paths_the_guest_owns() {
        // Refused: the guest owns these, or the mount would contain something it owns.
        for refused in [
            "/",
            "/nix",
            "/nix/store/x",
            "/etc/projects",
            "/home/vivarium",
            "/home/vivarium/work",
            // A proper ancestor: mounting here would bury `/home/vivarium` under the share.
            "/home",
            "/var/lib/thing",
            "/run/vivarium/x",
            "/run/vivarium-mounts/x",
            "/run/user/1000/x",
            "relative/path",
        ] {
            assert!(
                guest_owned_collision(Path::new(refused)).is_some(),
                "{refused} should be refused"
            );
        }
        // Allowed. `/nixos-projects` is the component-wise check earning its keep: a string prefix
        // would read it as `/nix`. `/var/home/u` is the ostree layout a blanket `/var` would break.
        for allowed in [
            "/home/u/Projects/foo",
            "/nixos-projects/foo",
            "/var/home/u/foo",
            // A removable drive on an ordinary desktop, and the reason `/run` is not denied
            // wholesale: this repository's own acceptance fixtures live at exactly this shape.
            "/run/media/u/drive/projects/foo",
            "/mnt/work/foo",
            "/srv/foo",
            "/data/foo",
            "/home/vivariumesque/foo",
        ] {
            assert!(
                guest_owned_collision(Path::new(allowed)).is_none(),
                "{allowed} should be allowed"
            );
        }
    }

    /// This list and the guest module's are two copies of one rule, pinned against each other
    /// rather than maintained in parallel.
    ///
    /// Since `ADR-0110` the guest-side copy is `guestOwnedTargetRoots` in `nix/guest.nix`, which
    /// refuses the same paths as ordinary mount targets — there is no mirror unit with a denylist
    /// of its own any more. The two sets are not identical, and the difference is the assertion's
    /// substance rather than a reason to skip it: the guest names `mountInternalRoot` and
    /// `upperRoot` through Nix identifiers rather than as literals, and the guest home is
    /// deliberately absent there because an identity mount under `~` is that surface's primary
    /// use, guarded as an exact match and an ancestor separately.
    ///
    /// What is not deliberate is drift. A path only the guest refuses turns a resolution-time `78`
    /// into a failed evaluation, which is the same answer arrived at later and with a worse
    /// message.
    #[test]
    fn the_host_and_the_guest_refuse_the_same_owned_paths() {
        let guest =
            std::str::from_utf8(crate::config::embedded_file("nix/guest.nix").unwrap()).unwrap();
        let body = guest
            .split_once("guestOwnedTargetRoots = [")
            .expect("the guest module still denies a list of owned target roots")
            .1
            .split_once("];")
            .expect("the owned-root list is still a Nix list")
            .0;
        // Quoted literals only. `mountInternalRoot` and `upperRoot` are identifiers there, so they
        // are named below rather than parsed out of a spelling that does not exist in the source.
        let guest_owned: std::collections::BTreeSet<&str> = body
            .split('"')
            .skip(1)
            .step_by(2)
            .filter(|word| word.starts_with('/'))
            .collect();
        let host_owned: std::collections::BTreeSet<&str> = GUEST_OWNED_PATHS
            .iter()
            .copied()
            // Named through an identifier on the guest side.
            .filter(|path| *path != "/run/vivarium-mounts")
            // Guarded separately on the guest side, as an exact match and an ancestor.
            .filter(|path| *path != "/home/vivarium")
            .collect();
        assert_eq!(
            host_owned, guest_owned,
            "the host and the guest disagree about which paths the guest owns"
        );
        assert!(
            body.contains("mountInternalRoot"),
            "the guest no longer refuses a target on the shares' internal root"
        );
    }

    /// The overlap rule, which exists in two deliberate copies and had assertions in neither.
    ///
    /// `ADR-0108` gives every declared workspace its own host-symmetric mount, and two trees where
    /// one contains the other cannot both have one: the inner bind lands inside the outer share
    /// and is shadowed by it, so a session in the inner tree silently reads the outer tree's copy.
    /// No declaration order makes both promises true, which is why the pair is refused rather than
    /// ordered.
    #[test]
    fn nested_pair_finds_equal_and_contained_pairs_component_wise() {
        let pairs_that_overlap: &[(&str, &str)] = &[
            // Equal, which is the degenerate nesting and the one an `is_prefix`-style rule that
            // tested only strict containment would miss.
            ("/home/u/app", "/home/u/app"),
            // Contained, in each declaration order, because the refusal may not depend on which
            // of the two the user happened to write first.
            ("/home/u", "/home/u/app"),
            ("/home/u/app", "/home/u"),
            ("/", "/home/u/app"),
        ];
        for (left, right) in pairs_that_overlap {
            assert!(
                nested_pair([Path::new(left), Path::new(right)]).is_some(),
                "{left} and {right} overlap and must be refused"
            );
        }
        // Component-wise, not string prefix: `/a` does not contain `/ab`. This is the same rule
        // `guest_owned_collision` relies on to let `/nixos-projects` past a `/nix` denial,
        // asserted here too because the two use different code paths to reach it.
        let pairs_that_are_disjoint: &[(&str, &str)] = &[
            ("/home/u/a", "/home/u/ab"),
            ("/home/u/app", "/home/u/app-2"),
            ("/home/u/one", "/home/u/two"),
            ("/srv/a", "/data/b"),
        ];
        for (left, right) in pairs_that_are_disjoint {
            assert!(
                nested_pair([Path::new(left), Path::new(right)]).is_none(),
                "{left} and {right} are disjoint and must be allowed"
            );
        }
        // Total over the set rather than over adjacent pairs: the overlapping pair here is the
        // first and the last, which a scan comparing only neighbours would walk straight past.
        assert!(
            nested_pair([
                Path::new("/home/u/app"),
                Path::new("/srv/other"),
                Path::new("/home/u/app/inner"),
            ])
            .is_some()
        );
        // Zero and one are both non-overlapping by construction; asserted so the empty case is a
        // decision rather than an accident of the loop bounds.
        assert!(nested_pair([]).is_none());
        assert!(nested_pair([Path::new("/home/u/app")]).is_none());
    }

    fn unset(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn resolve_refuses_each_defect_with_the_declared_spelling_it_came_from() {
        let defect = resolve(&["${NOT_SET_HERE}/tree".to_owned()], &unset).unwrap_err();
        assert_eq!(defect.declared, "${NOT_SET_HERE}/tree");
        assert!(matches!(
            defect.kind,
            WorkspaceDefectKind::UnsetVariable { ref variable } if variable == "NOT_SET_HERE"
        ));

        let defect = resolve(&["relative/tree".to_owned()], &unset).unwrap_err();
        assert!(matches!(
            defect.kind,
            WorkspaceDefectKind::NotAbsolute { .. }
        ));

        let defect = resolve(&["/nonexistent-vivarium-tree".to_owned()], &unset).unwrap_err();
        assert!(matches!(defect.kind, WorkspaceDefectKind::Missing { .. }));

        // A regular file, which has no file-mount shape to fall back to the way a `[[mounts]]`
        // row does: a workspace is a tree or it is refused.
        let defect = resolve(
            &[format!("{}/Cargo.toml", env!("CARGO_MANIFEST_DIR"))],
            &unset,
        )
        .unwrap_err();
        assert!(matches!(
            defect.kind,
            WorkspaceDefectKind::NotDirectory { .. }
        ));

        let defect = resolve(&["/etc".to_owned()], &unset).unwrap_err();
        assert!(matches!(
            defect.kind,
            WorkspaceDefectKind::GuestOwned { .. }
        ));
    }

    #[test]
    fn resolve_refuses_a_nested_pair_and_accepts_a_disjoint_one() {
        let root = env!("CARGO_MANIFEST_DIR");
        let nested = resolve(&[root.to_owned(), format!("{root}/src")], &unset).unwrap_err();
        assert!(matches!(nested.kind, WorkspaceDefectKind::Nested { .. }));

        let disjoint = resolve(&[format!("{root}/src"), format!("{root}/tests")], &unset).unwrap();
        assert_eq!(disjoint.len(), 2);
        // Canonical, because the value written into the flake as both `source` and `target` has to
        // be the one the launch-time classification arrives at.
        assert!(disjoint.iter().all(|path| path.is_absolute()));
    }

    /// Declaration order must not survive into the returned set, and this is the regression the
    /// whole suite missed: every fixture happened to declare its trees in ascending order, so a
    /// sorted set and a declaration-ordered list were the same list, and the comparison
    /// `reusable()` makes looked sound. Declared in descending order they are not, and a healthy
    /// VM then reads as foreign on every invocation after the first.
    #[test]
    fn resolve_returns_a_canonical_set_whatever_order_it_was_declared_in() {
        let root = env!("CARGO_MANIFEST_DIR");
        let ascending = resolve(&[format!("{root}/src"), format!("{root}/tests")], &unset).unwrap();
        let descending =
            resolve(&[format!("{root}/tests"), format!("{root}/src")], &unset).unwrap();
        assert_eq!(ascending, descending);
        assert!(ascending[0].ends_with("src"));
        assert!(ascending[1].ends_with("tests"));

        // Zero rows is not this module's refusal: a manifest declaring none is refused by the
        // undeclared-directory rule (ADR-0109), which names the block to add.
        assert!(resolve(&[], &unset).unwrap().is_empty());
    }

    /// The form both sides of the reuse comparison are put through, asserted on its own so the two
    /// callers cannot drift into sorting differently.
    #[test]
    fn canonicalize_set_orders_and_deduplicates() {
        let mut paths = vec![
            PathBuf::from("/z"),
            PathBuf::from("/a"),
            PathBuf::from("/z"),
            PathBuf::from("/m"),
        ];
        super::canonicalize_set(&mut paths);
        assert_eq!(
            paths,
            vec![
                PathBuf::from("/a"),
                PathBuf::from("/m"),
                PathBuf::from("/z"),
            ]
        );
    }
}
