//! Launch-time resolution of declared mount sources (spec/06, ADR-0020).
//!
//! A `[[mounts]]` source is declared with host-side `${VAR}` references left unexpanded — that is
//! what lets one shared piece be adopted by two people — and this module is where the declaration
//! meets the host: variables expand against the launch environment, and a source that is unset,
//! missing, not a regular file or directory, or a host session directory is refused here, before
//! any VM exists. The caller in `src/cli/lifecycle.rs` turns each [`MountSourceDefect`] into the
//! legible pre-boot diagnostic; `LaunchSpec::validate` and the supervisor's descriptor check stay
//! the backstops behind it.
//!
//! The mount list itself is read from the selected build's published contract
//! (`share/vivarium/launch-arguments.json`) rather than from the manifest: a piece-declared mount
//! exists only in the merged evaluation, and `--no-rebuild` evaluates nothing, so the built
//! artifact is the one place the list exists on every path. The declared credential ids ride the
//! same contract for the same reason, so this module is also where the launcher reads them.

use crate::launch::spec::MountPlanKind;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// One `shareLaunch` entry of the built contract, read tolerantly: the runner consumes the full
/// document, and this reader takes only what the pre-boot refusals need.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuiltShare {
    pub tag: String,
    pub origin: String,
    pub source_token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuiltContract {
    share_launch: Vec<BuiltShare>,
    #[serde(default)]
    credential_ids: Vec<crate::protocol::CredentialId>,
}

/// The declared shares of a built contract, in `shareLaunch` order.
///
/// # Errors
///
/// Returns the read or parse error verbatim; the caller owns the diagnostic.
pub fn declared_shares(store_path: &str) -> Result<Vec<BuiltShare>, std::io::Error> {
    Ok(read_contract(store_path)?
        .share_launch
        .into_iter()
        .filter(|share| share.origin == "declared")
        .collect())
}

/// The credential channels the built contract declares (`credentialIds`).
///
/// Read from the same artifact as the mounts and for the same reason: a piece-declared channel
/// exists only in the merged evaluation, and `--no-rebuild` evaluates nothing.
///
/// # Errors
///
/// Returns the read or parse error verbatim; the caller owns the diagnostic.
pub fn declared_credentials(
    store_path: &str,
) -> Result<Vec<crate::protocol::CredentialId>, std::io::Error> {
    Ok(read_contract(store_path)?.credential_ids)
}

fn read_contract(store_path: &str) -> Result<BuiltContract, std::io::Error> {
    let path = Path::new(store_path)
        .join("share")
        .join("vivarium")
        .join("launch-arguments.json");
    let raw = std::fs::read(&path)?;
    serde_json::from_slice(&raw).map_err(std::io::Error::other)
}

/// A declared mount resolved against the host: what the runner's `--mount` argument group and the
/// share's [`crate::launch::spec::MountPlan`] carry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedMount {
    pub tag: String,
    pub kind: MountPlanKind,
    /// The declared source, symlinks resolved, for either kind.
    ///
    /// A directory share is served as it stands. A regular file is not served directly — virtiofs
    /// exports a tree — but it is not served through its parent either: the supervisor stages it as
    /// the only entry of a private export root (ADR-0105), so this stays the file itself and the
    /// parent directory never reaches a daemon.
    pub share_source: PathBuf,
    /// The percent-encoded basename of a `file` mount, in the alphabet `mount-bind.sh` checks
    /// before it decodes.
    pub entry: Option<String>,
}

/// Why a declared source is refused before boot.
///
/// Each variant is one invariant, so the caller can give each its own diagnostic id and the
/// message can say which rule was broken rather than that one of four was.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MountSourceDefect {
    /// ADR-0020: an unset variable aborts before boot. Carries the variable, not a path — there
    /// is no path yet.
    UnsetVariable { variable: String },
    /// The expansion produced something no daemon can serve as a path.
    NotAbsolute { expanded: PathBuf },
    /// ADR-0020: a missing host path aborts before boot.
    Missing { expanded: PathBuf },
    /// The host refused to say what the source is; carried verbatim so permission problems read
    /// as themselves rather than as absence.
    Unreadable { expanded: PathBuf, error: String },
    /// ADR-0071: a source is filesystem data — a regular file or a directory, never a socket,
    /// FIFO, or device node.
    NotMountable { expanded: PathBuf },
    /// N24: no mount source resolves to a host session directory or an ancestor of one.
    SessionDirectory { expanded: PathBuf },
}

/// Expand `${VAR}` references against the host environment (ADR-0020's grammar).
///
/// Only the braced form expands; a bare `$VAR` and an unclosed `${` pass through as literal text,
/// where the missing-path refusal then names them. An unset variable is refused rather than
/// expanded to nothing: nothing-plus-the-rest is an existing path often enough that the failure
/// would move somewhere illegible.
///
/// # Errors
///
/// Returns [`MountSourceDefect::UnsetVariable`] naming the first unset variable.
pub fn expand_source(
    declared: &str,
    lookup: &dyn Fn(&str) -> Option<String>,
) -> Result<String, MountSourceDefect> {
    let mut expanded = String::with_capacity(declared.len());
    let mut rest = declared;
    while let Some(start) = rest.find("${") {
        expanded.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            expanded.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let variable = &after[..end];
        let named = !variable.is_empty()
            && variable
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
        if named {
            match lookup(variable) {
                Some(value) => expanded.push_str(&value),
                None => {
                    return Err(MountSourceDefect::UnsetVariable {
                        variable: variable.to_owned(),
                    });
                }
            }
        } else {
            // `${}` and `${not a name}` are not references; they stay literal and fail as a
            // missing path, which names the spelling the user wrote.
            expanded.push_str(&rest[start..=start + 2 + end]);
        }
        rest = &after[end + 1..];
    }
    expanded.push_str(rest);
    Ok(expanded)
}

/// Decide what the share serves for an expanded source, refusing everything spec/06 refuses.
///
/// `session_roots` are the host's session directories — `/tmp`, `/var/tmp`, and the resolved
/// `${XDG_RUNTIME_DIR}` when the host has one (N24); ancestors are refused in both directions,
/// against the declared spelling and again against what it resolves to, because a symlink
/// spelled innocently can name a session directory. The check here reads the path; the
/// supervisor re-reads the served directory through the descriptor it hands the daemon, which
/// is what closes the gap between checking and serving (spec/06).
///
/// # Errors
///
/// Returns the [`MountSourceDefect`] naming the broken invariant.
pub fn classify_source(
    tag: &str,
    expanded: &str,
    session_roots: &[PathBuf],
) -> Result<ResolvedMount, MountSourceDefect> {
    let path = PathBuf::from(expanded);
    if !path.is_absolute() {
        return Err(MountSourceDefect::NotAbsolute { expanded: path });
    }
    // The declared spelling first: a session path is refused even when nothing exists at it,
    // because the spelling alone decides it (N24).
    for root in session_roots {
        if path.starts_with(root) || root.starts_with(&path) {
            return Err(MountSourceDefect::SessionDirectory { expanded: path });
        }
    }
    // Then what the spelling names: `fs::metadata` follows symbolic links, so an innocently
    // spelled link can land in a session directory — or anywhere else — and the daemon would
    // serve the destination rather than the name. Resolve first and classify the resolved
    // object, and let the share carry it, so the path checked is the path served.
    let resolved = match std::fs::canonicalize(&path) {
        Ok(resolved) => resolved,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(MountSourceDefect::Missing { expanded: path });
        }
        Err(error) => {
            return Err(MountSourceDefect::Unreadable {
                expanded: path,
                error: error.to_string(),
            });
        }
    };
    for root in session_roots {
        if resolved.starts_with(root) || root.starts_with(&resolved) {
            return Err(MountSourceDefect::SessionDirectory { expanded: resolved });
        }
    }
    let metadata = match std::fs::metadata(&resolved) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(MountSourceDefect::Missing { expanded: resolved });
        }
        Err(error) => {
            return Err(MountSourceDefect::Unreadable {
                expanded: resolved,
                error: error.to_string(),
            });
        }
    };
    if metadata.is_dir() {
        return Ok(ResolvedMount {
            tag: tag.to_owned(),
            kind: MountPlanKind::Dir,
            share_source: resolved,
            entry: None,
        });
    }
    if !metadata.is_file() {
        return Err(MountSourceDefect::NotMountable { expanded: resolved });
    }
    // The file itself is what the share carries. The supervisor stages it as the only entry of the
    // share's own export root (ADR-0105), so the name is needed twice: to name that entry, which it
    // reads from this path, and to tell the guest which entry of the share to bind, which travels
    // encoded on the kernel command line. A path that resolves to a file always has one.
    let name = resolved
        .file_name()
        .ok_or_else(|| MountSourceDefect::NotMountable {
            expanded: resolved.clone(),
        })?
        .to_owned();
    Ok(ResolvedMount {
        tag: tag.to_owned(),
        kind: MountPlanKind::File,
        share_source: resolved,
        entry: Some(encode_entry(name.as_os_str())),
    })
}

/// Percent-encode a single name for the kernel command line and the runner's `--mount` argument.
///
/// The unreserved set is the workspace encoder's without `/` — an entry is one name inside the
/// share, never a path — and the hex is upper-case, matching the allowlist `mount-bind.sh` and
/// the runner check before decoding.
pub(crate) fn encode_entry(name: &std::ffi::OsStr) -> String {
    use std::fmt::Write as _;
    use std::os::unix::ffi::OsStrExt as _;
    let mut encoded = String::new();
    for byte in name.as_bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(byte) {
            encoded.push(char::from(*byte));
        } else {
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name: &str| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    #[test]
    fn expansion_follows_the_braced_grammar_only() {
        let lookup = env(&[("HOME", "/home/ana"), ("XDG_DATA_HOME", "/home/ana/.data")]);
        assert_eq!(
            expand_source("${HOME}/.cargo", &lookup).unwrap(),
            "/home/ana/.cargo"
        );
        assert_eq!(
            expand_source("${HOME}/${XDG_DATA_HOME}", &lookup).unwrap(),
            "/home/ana//home/ana/.data"
        );
        // Bare `$VAR`, `${}`, and an unclosed brace stay literal.
        assert_eq!(expand_source("$HOME/x", &lookup).unwrap(), "$HOME/x");
        assert_eq!(expand_source("/a/${}/b", &lookup).unwrap(), "/a/${}/b");
        assert_eq!(expand_source("/a/${HOME", &lookup).unwrap(), "/a/${HOME");
        assert_eq!(
            expand_source("/a/${no space}", &lookup).unwrap(),
            "/a/${no space}"
        );
        assert_eq!(
            expand_source("${MISSING}/x", &lookup).unwrap_err(),
            MountSourceDefect::UnsetVariable {
                variable: "MISSING".into()
            }
        );
    }

    #[test]
    fn classification_names_each_broken_invariant() {
        let sessions = [PathBuf::from("/tmp"), PathBuf::from("/run/user/1000")];
        let dir = std::env::current_dir().unwrap();
        let resolved = classify_source("mnt0", dir.to_str().unwrap(), &sessions).unwrap();
        assert_eq!(resolved.kind, MountPlanKind::Dir);
        assert_eq!(resolved.share_source, dir);
        assert_eq!(resolved.entry, None);

        assert!(matches!(
            classify_source("mnt0", "relative/path", &sessions),
            Err(MountSourceDefect::NotAbsolute { .. })
        ));
        assert!(matches!(
            classify_source("mnt0", "/tmp/anything", &sessions),
            Err(MountSourceDefect::SessionDirectory { .. })
        ));
        // The ancestor rule in both directions: `/` contains every session root.
        assert!(matches!(
            classify_source("mnt0", "/", &sessions),
            Err(MountSourceDefect::SessionDirectory { .. })
        ));
        assert!(matches!(
            classify_source("mnt0", "/nonexistent-vivarium-source", &sessions),
            Err(MountSourceDefect::Missing { .. })
        ));
        assert!(matches!(
            classify_source("mnt0", "/dev/null", &sessions),
            Err(MountSourceDefect::NotMountable { .. })
        ));
    }

    #[test]
    fn a_symlinked_source_is_classified_by_what_it_resolves_to() {
        // N24 must hold for the object served, not the spelling declared: a link outside every
        // session root whose destination is inside one is the bypass this test pins closed.
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("vivarium-mounts-link-{unique}"));
        let session = base.join("session");
        let elsewhere = base.join("elsewhere");
        std::fs::create_dir_all(&session).unwrap();
        std::fs::create_dir_all(&elsewhere).unwrap();
        let link = elsewhere.join("innocent");
        std::os::unix::fs::symlink(&session, &link).unwrap();
        // Canonicalized like the caller's real roots, so a symlinked temp dir on the machine
        // running the test cannot turn this into a comparison of two different spellings.
        let sessions = [session.canonicalize().unwrap()];
        assert!(matches!(
            classify_source("mnt0", link.to_str().unwrap(), &sessions),
            Err(MountSourceDefect::SessionDirectory { .. })
        ));
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_file_source_carries_itself_with_an_encoded_entry() {
        // No session roots on purpose: this fixture lives in the test temp dir, and what is
        // under test is the file/entry pairing, not N24.
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("vivarium-mounts-test-{unique}"));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a config.toml");
        std::fs::write(&file, "x").unwrap();
        let resolved = classify_source("mnt1", file.to_str().unwrap(), &[]).unwrap();
        assert_eq!(resolved.kind, MountPlanKind::File);
        // The file, not the directory that holds it: the parent never reaches a daemon.
        assert_eq!(resolved.share_source, file.canonicalize().unwrap());
        assert_ne!(resolved.share_source, dir);
        assert_eq!(resolved.entry.as_deref(), Some("a%20config.toml"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
