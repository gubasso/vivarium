//! The network-scope probes, behind `--online`: the default run is fully offline (spec/13).
//!
//! Each one talks to something the host does not own, so each degrades to `skipped` /
//! `not-applicable` with the fault named when its channel cannot be used at all — an unreachable
//! probe is not a finding about the thing it probes.

use std::net::{TcpStream, ToSocketAddrs};
use std::process::Command;
use std::time::Duration;

use crate::config::{EgressMode, Environment};

use super::{Finding, Inputs, Probe};

/// One connect attempt's budget; three unreachable substituters cost at most three of these.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) fn run<E: Environment>(probe: &'static Probe, inputs: &Inputs<'_, E>) -> Finding {
    match probe.id {
        "nix-version-currency" => nix_version_currency(probe),
        "substituter-reachability" => substituter_reachability(probe),
        "egress-allowlist-dns" => egress_allowlist_dns(probe, inputs),
        _ => Finding::skipped(probe, "not-applicable", "not a network probe"),
    }
}

fn nix_version_currency(probe: &'static Probe) -> Finding {
    // `git ls-remote` against the upstream repository is the one release feed reachable without
    // an HTTP client in the dependency set, and git is already in every flakes-capable
    // installation's orbit. A tag listing is names, not code.
    let output = Command::new("git")
        .args(["ls-remote", "--tags", "https://github.com/NixOS/nix.git"])
        .output();
    let Ok(output) = output else {
        return Finding::skipped(probe, "not-applicable", "`git` could not be run");
    };
    if !output.status.success() {
        return Finding::skipped(
            probe,
            "not-applicable",
            "the upstream release listing could not be fetched",
        );
    }
    let tags = String::from_utf8_lossy(&output.stdout);
    let Some(latest) = latest_stable_tag(&tags) else {
        return Finding::skipped(
            probe,
            "not-applicable",
            "no stable release tag could be read from the listing",
        );
    };
    let Some(installed) = installed_nix_version() else {
        return Finding::skipped(probe, "not-applicable", "`nix --version` could not be read");
    };
    if installed >= latest {
        Finding::pass(
            probe,
            format!(
                "nix {}.{}.{} is current (latest stable {}.{}.{})",
                installed.0, installed.1, installed.2, latest.0, latest.1, latest.2
            ),
        )
    } else {
        Finding::tripped(
            probe,
            format!(
                "nix {}.{}.{} trails the latest stable release {}.{}.{}",
                installed.0, installed.1, installed.2, latest.0, latest.1, latest.2
            ),
            "upgrade nix when convenient; nothing refuses on currency alone",
        )
    }
}

/// The highest `x.y.z` among `refs/tags/x.y.z` lines, skipping pre-releases.
fn latest_stable_tag(listing: &str) -> Option<(u64, u64, u64)> {
    listing
        .lines()
        .filter_map(|line| line.split("refs/tags/").nth(1))
        .filter(|tag| !tag.contains('^'))
        .filter_map(parse_triple)
        .max()
}

/// `x.y.z` (or `x.y`) with nothing after it — a suffix marks a pre-release and is skipped.
fn parse_triple(tag: &str) -> Option<(u64, u64, u64)> {
    let mut parts = tag.split('.');
    let major: u64 = parts.next()?.parse().ok()?;
    let minor: u64 = parts.next()?.parse().ok()?;
    let patch: u64 = match parts.next() {
        Some(part) => part.parse().ok()?,
        None => 0,
    };
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

fn installed_nix_version() -> Option<(u64, u64, u64)> {
    let output = Command::new("nix").arg("--version").output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_triple(stdout.split_whitespace().last()?)
}

fn substituter_reachability(probe: &'static Probe) -> Finding {
    let output = Command::new("nix")
        .args(["config", "show", "substituters"])
        .output();
    let Ok(output) = output else {
        return Finding::skipped(probe, "not-applicable", "`nix` could not be run");
    };
    let configured = String::from_utf8_lossy(&output.stdout);
    let substituters: Vec<&str> = configured.split_whitespace().collect();
    if substituters.is_empty() {
        return Finding::pass(probe, "no substituters are configured");
    }
    let mut unreachable = Vec::new();
    for substituter in &substituters {
        if let Some(authority) = connect_authority(substituter)
            && !tcp_reachable(&authority)
        {
            unreachable.push((*substituter).to_owned());
        }
    }
    if unreachable.is_empty() {
        Finding::pass(
            probe,
            format!("all {} substituters are reachable", substituters.len()),
        )
    } else {
        Finding::tripped(
            probe,
            format!("`{}` is unreachable", unreachable.join("`, `")),
            "builds fall back to source builds while a substituter is down; check the \
            network path or drop the entry from nix.conf",
        )
    }
}

/// `host:port` for a substituter URL, `None` for the schemes with nothing to connect to.
fn connect_authority(substituter: &str) -> Option<String> {
    let (scheme, rest) = substituter.split_once("://")?;
    let default_port = match scheme {
        "https" => 443,
        "http" => 80,
        // `file://`, `s3://` with credentials, daemons — not this probe's to reach.
        _ => return None,
    };
    let authority = rest.split('/').next()?;
    if authority.is_empty() {
        return None;
    }
    Some(if authority.contains(':') {
        authority.to_owned()
    } else {
        format!("{authority}:{default_port}")
    })
}

fn tcp_reachable(authority: &str) -> bool {
    let Ok(addresses) = authority.to_socket_addrs() else {
        return false;
    };
    addresses
        .into_iter()
        .any(|address| TcpStream::connect_timeout(&address, CONNECT_TIMEOUT).is_ok())
}

fn egress_allowlist_dns<E: Environment>(probe: &'static Probe, inputs: &Inputs<'_, E>) -> Finding {
    // Also project scope (spec/13): probed only when the bound manifest sets allowlist mode.
    let Some(project) = &inputs.project else {
        return Finding::skipped(probe, "no-manifest-bound", "");
    };
    let Ok(manifest) = &project.parsed else {
        return Finding::skipped(probe, "not-applicable", "the manifest does not parse");
    };
    let Some(egress) = &manifest.egress else {
        return Finding::skipped(
            probe,
            "not-applicable",
            "the manifest declares no egress table",
        );
    };
    if egress.mode != Some(EgressMode::Allowlist) {
        return Finding::skipped(probe, "not-applicable", "egress mode is not `allowlist`");
    }
    // Only a wildcard-free entry names a single host to look up; a manifest of nothing but
    // wildcards yields no finding rather than a false one (spec/13).
    let names: Vec<&String> = egress
        .allow
        .iter()
        .filter(|entry| !entry.contains('*') && entry.parse::<std::net::IpAddr>().is_err())
        .filter(|entry| entry.parse::<ipnet::IpNet>().is_err())
        .collect();
    if names.is_empty() {
        return Finding::pass(probe, "the allowlist names no wildcard-free hostnames");
    }
    let mut unresolved = Vec::new();
    for name in &names {
        if format!("{name}:443").to_socket_addrs().is_err() {
            unresolved.push((*name).clone());
        }
    }
    if unresolved.is_empty() {
        Finding::pass(
            probe,
            format!(
                "all {} wildcard-free allowlisted names resolve",
                names.len()
            ),
        )
    } else {
        Finding::tripped(
            probe,
            format!("`{}` does not resolve", unresolved.join("`, `")),
            "the guest will meet the upstream answer as NXDOMAIN or SERVFAIL; fix the \
            entry or the upstream zone",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_latest_stable_tag_skips_prereleases_and_peeled_refs() {
        let listing = concat!(
            "aaa\trefs/tags/2.33.1\n",
            "bbb\trefs/tags/2.34.8\n",
            "ccc\trefs/tags/2.34.8^{}\n",
            "ddd\trefs/tags/2.35.0pre20260801\n",
            "eee\trefs/tags/2.9\n",
        );
        assert_eq!(latest_stable_tag(listing), Some((2, 34, 8)));
    }

    #[test]
    fn a_connect_authority_exists_only_for_http_schemes() {
        assert_eq!(
            connect_authority("https://cache.nixos.org"),
            Some("cache.nixos.org:443".to_owned())
        );
        assert_eq!(
            connect_authority("http://mirror:8080/prefix"),
            Some("mirror:8080".to_owned())
        );
        assert_eq!(connect_authority("file:///var/cache"), None);
        assert_eq!(connect_authority("s3://bucket"), None);
    }
}
