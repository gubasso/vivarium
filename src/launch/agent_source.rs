//! Launch-time resolution of declared credential channels (spec/07, ADR-0071).
//!
//! A layer declares `vivarium.credentials.agents` as a member of a closed enum — never a host
//! path — and this module is where that id meets the host: `ssh` resolves from `$SSH_AUTH_SOCK`
//! and nothing else, `gpg` from the agent's restricted extra socket as the GPG installation
//! itself reports it. A source that is unset, missing, not a socket, or not owned by the user is
//! refused here, before any build or boot, because a relay that is missing rather than refused is
//! discovered as an authentication failure inside the guest. The caller in `src/cli/lifecycle.rs`
//! turns each [`AgentSourceDefect`] into the legible pre-boot diagnostic; the runner's usage
//! guard and `LaunchSpec::validate`'s exact-socket rule stay the backstops behind it.
//!
//! Asking `gpgconf` where the extra socket lives is a path query, not the provider hook spec/07
//! forbids: the answer is a filesystem name, and no secret ever enters this address space.

use crate::protocol::CredentialId;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};

/// Why a declared channel's host source is refused: spec/13's four faults, one variant each so
/// the message can say which rule was broken and its own repair.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentSourceDefect {
    /// Nothing to resolve from: `$SSH_AUTH_SOCK` unset or empty, or `gpgconf` unable to name an
    /// extra socket. Carries what was consulted, not a path — there is no path yet.
    Unset { consulted: String },
    /// The resolved path names nothing on this host.
    Missing { path: PathBuf },
    /// Not the exact socket object: a regular file, directory, device — or a symlink, refused
    /// unfollowed for the same reason the runner's `-S && ! -L` belt refuses one.
    NotSocket { path: PathBuf },
    /// A socket, but somebody else's; forwarding it would relay another user's agent.
    NotOwned { path: PathBuf, owner: u32, uid: u32 },
}

/// One declared channel resolved against this host: what the runner's socket argument carries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedAgentSource {
    pub id: CredentialId,
    pub host_socket: PathBuf,
}

/// Where the host looks for one id's socket.
///
/// Pure over `lookup` for `ssh`; the `gpg` arm asks the installed `gpgconf`, which is host state
/// by design — the restricted extra socket has no environment variable, and spec/07 records the
/// query as the resolution rule.
///
/// # Errors
///
/// Returns [`AgentSourceDefect::Unset`] naming what was consulted when nothing resolves.
pub fn resolve_source(
    id: CredentialId,
    lookup: &dyn Fn(&str) -> Option<String>,
) -> Result<PathBuf, AgentSourceDefect> {
    match id {
        CredentialId::Ssh => match lookup("SSH_AUTH_SOCK") {
            Some(value) if !value.is_empty() => Ok(PathBuf::from(value)),
            _ => Err(AgentSourceDefect::Unset {
                consulted: "$SSH_AUTH_SOCK".to_owned(),
            }),
        },
        CredentialId::Gpg => gpg_extra_socket(),
    }
}

/// The restricted extra socket, as the GPG installation itself names it — never the ordinary
/// socket, which spec/07 deliberately refuses to forward.
fn gpg_extra_socket() -> Result<PathBuf, AgentSourceDefect> {
    const CONSULTED: &str = "gpgconf --list-dirs agent-extra-socket";
    let unset = || AgentSourceDefect::Unset {
        consulted: CONSULTED.to_owned(),
    };
    let output = std::process::Command::new("gpgconf")
        .args(["--list-dirs", "agent-extra-socket"])
        .output()
        .map_err(|_| unset())?;
    if !output.status.success() {
        return Err(unset());
    }
    let answer = String::from_utf8_lossy(&output.stdout);
    let path = answer.trim();
    if path.is_empty() {
        return Err(unset());
    }
    Ok(PathBuf::from(path))
}

/// The stat half: the path names the exact socket object and `uid` owns it.
///
/// `symlink_metadata` deliberately — a symlink to a socket is refused, matching
/// `LaunchSpec::validate`'s rule that a credential leg names the socket object itself.
///
/// # Errors
///
/// Returns the first of [`AgentSourceDefect::Missing`], [`AgentSourceDefect::NotSocket`], or
/// [`AgentSourceDefect::NotOwned`] that holds.
pub fn check_socket(path: &Path, uid: u32) -> Result<(), AgentSourceDefect> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| AgentSourceDefect::Missing {
        path: path.to_path_buf(),
    })?;
    if !metadata.file_type().is_socket() {
        return Err(AgentSourceDefect::NotSocket {
            path: path.to_path_buf(),
        });
    }
    if metadata.uid() != uid {
        return Err(AgentSourceDefect::NotOwned {
            path: path.to_path_buf(),
            owner: metadata.uid(),
            uid,
        });
    }
    Ok(())
}

/// The final component appended to its kernel-resolved directory.
///
/// The directory half resolves by the kernel's own rules — `canonicalize` follows intermediate
/// symlinks in order and refuses a missing component, exactly as a `connect()` on the declared
/// spelling would — so this names the same object the agent's own client would reach, and never
/// a lexical rewrite of it. The final component is appended untouched, which is what keeps the
/// symlinked-socket refusal below meaningful. The result is absolute and dot-free, the only
/// shape the launch specification's `require_absolute_resolved` admits, so a spelling that would
/// die there dies here instead, under the agent-source refusal.
fn resolve_directory(joined: &Path) -> Result<PathBuf, AgentSourceDefect> {
    let Some(name) = joined.file_name() else {
        // The root, or a spelling ending in `..`: a directory name, never the socket object.
        return Err(AgentSourceDefect::NotSocket {
            path: joined.to_path_buf(),
        });
    };
    let parent = joined.parent().unwrap_or_else(|| Path::new("/"));
    let directory = std::fs::canonicalize(parent).map_err(|_| AgentSourceDefect::Missing {
        path: joined.to_path_buf(),
    })?;
    Ok(directory.join(name))
}

/// Resolve one declared id against this host or say why it cannot be.
///
/// A relative value resolves against the invoking directory — the reading `ssh` itself gives a
/// relative `SSH_AUTH_SOCK`. The declared spelling is then checked as the kernel reads it,
/// before any reshaping: `symlink_metadata` on the spelling is what an agent client's own
/// `connect()` would experience, so a spelling the kernel refuses — a trailing separator or `.`
/// after the socket, a missing intermediate — is refused here too, never normalized into a path
/// that happens to work. Only then does the containing directory resolve
/// ([`resolve_directory`]) into the absolute, dot-free carry the launch contract admits, and the
/// carried name is checked once more so the two spellings provably name one socket.
///
/// # Errors
///
/// Returns the defect from [`resolve_source`], either [`check_socket`], or
/// [`resolve_directory`], in declaration order; an invoking directory the host cannot name reads
/// as [`AgentSourceDefect::Missing`].
pub fn resolve_and_check(
    id: CredentialId,
    lookup: &dyn Fn(&str) -> Option<String>,
    uid: u32,
) -> Result<ResolvedAgentSource, AgentSourceDefect> {
    let mut host_socket = resolve_source(id, lookup)?;
    if host_socket.is_relative() {
        let invoked_in = std::env::current_dir().map_err(|_| AgentSourceDefect::Missing {
            path: host_socket.clone(),
        })?;
        host_socket = invoked_in.join(host_socket);
    }
    check_socket(&host_socket, uid)?;
    let host_socket = resolve_directory(&host_socket)?;
    check_socket(&host_socket, uid)?;
    Ok(ResolvedAgentSource { id, host_socket })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name: &str| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    #[test]
    fn ssh_resolves_from_its_variable_and_nothing_else() {
        let lookup = env(&[("SSH_AUTH_SOCK", "/run/user/1000/agent.sock")]);
        assert_eq!(
            resolve_source(CredentialId::Ssh, &lookup).unwrap(),
            PathBuf::from("/run/user/1000/agent.sock")
        );
        // Unset and empty are the same fault: nothing to resolve from.
        assert_eq!(
            resolve_source(CredentialId::Ssh, &env(&[])).unwrap_err(),
            AgentSourceDefect::Unset {
                consulted: "$SSH_AUTH_SOCK".into()
            }
        );
        assert_eq!(
            resolve_source(CredentialId::Ssh, &env(&[("SSH_AUTH_SOCK", "")])).unwrap_err(),
            AgentSourceDefect::Unset {
                consulted: "$SSH_AUTH_SOCK".into()
            }
        );
    }

    #[test]
    fn a_relative_source_resolves_against_the_invoking_directory() {
        // The cwd-joined path is observable through the defect that carries it: the file does
        // not exist, and the `Missing` it reports names the joined spelling a launch would have
        // tried to resolve.
        let lookup = env(&[("SSH_AUTH_SOCK", "vivarium-relative-agent.sock")]);
        let uid = crate::config::effective_uid();
        let defect = resolve_and_check(CredentialId::Ssh, &lookup, uid).unwrap_err();
        let expected = std::env::current_dir()
            .unwrap()
            .join("vivarium-relative-agent.sock");
        assert_eq!(defect, AgentSourceDefect::Missing { path: expected });
    }

    #[test]
    fn a_spelling_naming_a_directory_is_not_a_socket() {
        // A value ending in `..` names a directory under any resolution, so it is refused as
        // the wrong object rather than rewritten into a sibling path.
        let lookup = env(&[("SSH_AUTH_SOCK", "/run/user/..")]);
        let uid = crate::config::effective_uid();
        assert!(matches!(
            resolve_and_check(CredentialId::Ssh, &lookup, uid).unwrap_err(),
            AgentSourceDefect::NotSocket { .. }
        ));
    }

    #[test]
    fn the_stat_half_names_each_broken_invariant() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        // Under the runtime directory rather than `TMPDIR`, which a gated run binds onto the
        // heavy drive: a bound socket's path must stay inside the 108-byte `SUN_LEN` budget.
        let short_root = std::env::var_os("XDG_RUNTIME_DIR")
            .map_or_else(|| PathBuf::from("/tmp"), PathBuf::from);
        let base = short_root.join(format!("vivarium-agent-source-{unique}"));
        std::fs::create_dir_all(&base).unwrap();
        let uid = crate::config::effective_uid();

        // A socket you bound is yours: the positive case.
        let socket = base.join("agent.sock");
        let _listener = UnixListener::bind(&socket).unwrap();
        assert_eq!(check_socket(&socket, uid), Ok(()));

        // A trailing separator or terminal `.` makes the kernel treat the socket as a
        // directory (`ENOTDIR`), and Rust's `Path` would happily normalize either away — so
        // both spellings must refuse even though the socket itself is real and usable.
        for spelling in [
            format!("{}/", socket.display()),
            format!("{}/.", socket.display()),
        ] {
            assert!(
                matches!(
                    resolve_and_check(
                        CredentialId::Ssh,
                        &env(&[("SSH_AUTH_SOCK", &spelling)]),
                        uid
                    )
                    .unwrap_err(),
                    AgentSourceDefect::Missing { .. }
                ),
                "`{spelling}` must refuse as the kernel reads it"
            );
        }

        // Directory resolution follows the kernel's rules, not a lexical rewrite. An absent
        // intermediate component refuses even though collapsing `ghost/..` lexically would have
        // named a socket that exists.
        let ghost = format!("{}/ghost/../agent.sock", base.display());
        assert!(matches!(
            resolve_and_check(CredentialId::Ssh, &env(&[("SSH_AUTH_SOCK", &ghost)]), uid)
                .unwrap_err(),
            AgentSourceDefect::Missing { .. }
        ));

        // And an intermediate symlink is followed before `..` is applied, exactly as the
        // agent's own client would resolve the spelling: `link -> nested/deep`, so
        // `link/../agent.sock` names `nested/agent.sock` and never `agent.sock` beside `link`.
        let nested = base.join("nested");
        std::fs::create_dir_all(nested.join("deep")).unwrap();
        let nested_socket = nested.join("agent.sock");
        let _nested_listener = UnixListener::bind(&nested_socket).unwrap();
        std::os::unix::fs::symlink(nested.join("deep"), base.join("link")).unwrap();
        let through_link = format!("{}/link/../agent.sock", base.display());
        let resolved = resolve_and_check(
            CredentialId::Ssh,
            &env(&[("SSH_AUTH_SOCK", &through_link)]),
            uid,
        )
        .unwrap();
        assert_eq!(
            resolved.host_socket,
            std::fs::canonicalize(&nested).unwrap().join("agent.sock")
        );

        // Wrong owner is its own fault, with both uids carried for the message.
        assert_eq!(
            check_socket(&socket, uid + 1),
            Err(AgentSourceDefect::NotOwned {
                path: socket.clone(),
                owner: uid,
                uid: uid + 1,
            })
        );

        // A plain file is not the socket object.
        let file = base.join("not-a-socket");
        std::fs::write(&file, b"").unwrap();
        assert!(matches!(
            check_socket(&file, uid),
            Err(AgentSourceDefect::NotSocket { .. })
        ));

        // A symlink to a real socket is refused unfollowed, matching the runner's `! -L` belt.
        let link = base.join("agent-link.sock");
        std::os::unix::fs::symlink(&socket, &link).unwrap();
        assert!(matches!(
            check_socket(&link, uid),
            Err(AgentSourceDefect::NotSocket { .. })
        ));

        assert!(matches!(
            check_socket(&base.join("absent.sock"), uid),
            Err(AgentSourceDefect::Missing { .. })
        ));

        std::fs::remove_dir_all(&base).unwrap();
    }
}
