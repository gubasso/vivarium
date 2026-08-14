//! Argv construction and lifecycle for the user+network namespace pair.
//!
//! The pair cannot be made in-process twice over: the workspace forbids `unsafe`, and
//! `unshare(CLONE_NEWUSER)` refuses a multithreaded caller outright, which the
//! supervisor's runtime is. So the seam is the pinned util-linux pair — `unshare`
//! creates and holds the namespaces, `nsenter` joins them by the holder's pid — and
//! what this module owns is the exact flag rendering, the same way `policy.rs` owns
//! `setpriv`'s, plus the readiness watch a joiner needs before the pid is safe to
//! join. Q-005's spike record: full `CapEff` inside the pair for an unprivileged
//! caller, and a host-side `AF_UNIX` listener reachable from inside, because unix
//! sockets are filesystem-scoped rather than netns-scoped.

use std::time::Duration;

use thiserror::Error;

/// The legs `unshare` needs to create the pair and hold root's mapping inside it.
///
/// `--map-root-user` rather than `--map-current-user`: the tap, ruleset, and route
/// setup inside the namespace all want `CAP_NET_ADMIN`, and mapping the caller to
/// root is what makes the full capability set effective there.
const UNSHARE_PAIR: [&str; 3] = ["--user", "--net", "--map-root-user"];

/// The legs `nsenter` needs to join a pair created by [`UNSHARE_PAIR`].
///
/// `--preserve-credentials` keeps the entering process's uid mapping as the holder
/// established it instead of resetting to the caller's, which is what lets a second
/// unprivileged process act with the pair's own capabilities.
const NSENTER_JOIN: [&str; 3] = ["--preserve-credentials", "--user", "--net"];

/// Arguments for `unshare` that create the pair and exec the holder program inside it.
#[must_use]
pub fn create_pair_args(holder: &[String]) -> Vec<String> {
    let mut args: Vec<String> = UNSHARE_PAIR.iter().map(ToString::to_string).collect();
    args.extend_from_slice(holder);
    args
}

/// Arguments for `nsenter` that join the pair held by `holder_pid` and exec a program
/// inside it.
#[must_use]
pub fn enter_pair_args(holder_pid: u32, program: &[String]) -> Vec<String> {
    let mut args: Vec<String> = NSENTER_JOIN.iter().map(ToString::to_string).collect();
    args.push("--target".to_owned());
    args.push(holder_pid.to_string());
    args.extend_from_slice(program);
    args
}

/// The program the holder execs once the pair exists: a pinned `sleep` that keeps
/// both namespaces referenced for the VM's lifetime without doing anything else.
#[must_use]
pub fn holder_program(sleep: &str) -> Vec<String> {
    vec![sleep.to_owned(), "infinity".to_owned()]
}

/// A pair that never became joinable.
#[derive(Debug, Error)]
pub enum PairError {
    /// The holder is gone: it exited, or was never the pid the caller thought.
    #[error("the namespace holder (pid {holder_pid}) is gone before its pair became distinct")]
    HolderExited { holder_pid: u32 },
    /// The holder is alive but still shares this process's namespaces.
    #[error(
        "the namespace pair held by pid {holder_pid} did not become distinct within {waited_ms} ms"
    )]
    Timeout { holder_pid: u32, waited_ms: u64 },
}

/// Wait until the holder's user and net namespaces are both distinct from this
/// process's, which is the moment `nsenter` joins the new pair rather than the old
/// namespaces.
///
/// The holder's pid is observable before `unshare` has made the namespaces, so
/// joining on spawn alone races the syscall; the namespace links flipping to new
/// inodes is the readiness signal the race needs.
///
/// # Errors
///
/// Returns [`PairError::HolderExited`] when the holder's `/proc` entry disappears,
/// and [`PairError::Timeout`] when the links stay shared past `timeout`.
pub async fn await_pair(holder_pid: u32, timeout: Duration) -> Result<(), PairError> {
    let started = std::time::Instant::now();
    loop {
        match pair_distinct(holder_pid) {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(_) => return Err(PairError::HolderExited { holder_pid }),
        }
        if started.elapsed() >= timeout {
            return Err(PairError::Timeout {
                holder_pid,
                waited_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            });
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// Whether both of the holder's namespace links differ from this process's own.
fn pair_distinct(holder_pid: u32) -> std::io::Result<bool> {
    for namespace in ["user", "net"] {
        let own = std::fs::read_link(format!("/proc/self/ns/{namespace}"))?;
        let held = std::fs::read_link(format!("/proc/{holder_pid}/ns/{namespace}"))?;
        if own == held {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(ToString::to_string).collect()
    }

    /// One table over the three argv renderings: the pair-creating `unshare`
    /// flags, the pinned holder, and the `nsenter` join flags. Each expected
    /// vector is written as a literal so a flag change is a visible diff here,
    /// not something re-derived from the constant under test.
    #[test]
    fn argv_renderings_are_pinned() {
        let rows: &[(Vec<String>, &[&str])] = &[
            (
                create_pair_args(&strings(&["sleep", "infinity"])),
                &["--user", "--net", "--map-root-user", "sleep", "infinity"],
            ),
            (
                holder_program("/nix/store/x-coreutils/bin/sleep"),
                &["/nix/store/x-coreutils/bin/sleep", "infinity"],
            ),
            (
                enter_pair_args(4242, &strings(&["nft", "-j", "-f", "-"])),
                &[
                    "--preserve-credentials",
                    "--user",
                    "--net",
                    "--target",
                    "4242",
                    "nft",
                    "-j",
                    "-f",
                    "-",
                ],
            ),
        ];
        for (rendered, expected) in rows {
            assert_eq!(rendered, &strings(expected));
        }
    }

    #[tokio::test]
    async fn awaiting_a_gone_holder_reports_it_rather_than_waiting() {
        // A pid that cannot exist: one past the kernel's own PID_MAX_LIMIT. The
        // read fails immediately, which must be HolderExited, not a timeout.
        let result = await_pair(4_194_305, Duration::from_secs(5)).await;
        assert!(matches!(
            result,
            Err(PairError::HolderExited {
                holder_pid: 4_194_305
            })
        ));
    }

    #[tokio::test]
    async fn awaiting_a_shared_namespace_times_out() {
        // This process trivially shares its own namespaces, so the watch can never
        // see a distinct pair and must give up at the deadline.
        let own_pid = std::process::id();
        let result = await_pair(own_pid, Duration::from_millis(30)).await;
        assert!(matches!(result, Err(PairError::Timeout { .. })));
    }
}
