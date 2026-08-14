//! Argv construction for the user+network namespace pair.
//!
//! The pair cannot be made in-process twice over: the workspace forbids `unsafe`, and
//! `unshare(CLONE_NEWUSER)` refuses a multithreaded caller outright, which the
//! supervisor's runtime is. So the seam is the pinned util-linux pair — `unshare`
//! creates and holds the namespaces, `nsenter` joins them by the holder's pid — and
//! what this module owns is the exact flag rendering, the same way `policy.rs` owns
//! `setpriv`'s. Q-005's spike record: full `CapEff` inside the pair for an
//! unprivileged caller, and a host-side `AF_UNIX` listener reachable from inside,
//! because unix sockets are filesystem-scoped rather than netns-scoped.

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

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn create_renders_the_pair_flags_then_the_holder() {
        assert_eq!(
            create_pair_args(&strings(&["sleep", "infinity"])),
            strings(&["--user", "--net", "--map-root-user", "sleep", "infinity"])
        );
    }

    #[test]
    fn enter_renders_the_join_flags_target_then_the_program() {
        assert_eq!(
            enter_pair_args(4242, &strings(&["nft", "-j", "-f", "-"])),
            strings(&[
                "--preserve-credentials",
                "--user",
                "--net",
                "--target",
                "4242",
                "nft",
                "-j",
                "-f",
                "-",
            ])
        );
    }
}
