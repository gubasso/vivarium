//! The `egress.allow` entry grammar and its matcher.
//!
//! The five entry forms and their exact matching rules are spec/05's table; the two
//! wildcard tokens are deliberately separate so a manifest says which span it means.
//! Name entries gate DNS answers in the resolver; address and block entries need no
//! resolution and are programmed into the filter directly.

use std::net::IpAddr;
use std::str::FromStr;

use ipnet::IpNet;
use thiserror::Error;

/// A single parsed `egress.allow` entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AllowEntry {
    /// `example.com` — that name exactly.
    Exact(String),
    /// `*.example.com` — exactly one additional label.
    OneLabel(String),
    /// `**.example.com` — one or more additional labels.
    ManyLabels(String),
    /// `192.0.2.10`, `2001:db8::1` — that address, reached without DNS.
    Addr(IpAddr),
    /// `192.0.2.0/24`, `2001:db8::/32` — any address in that block.
    Net(IpNet),
}

#[derive(Debug, Error)]
pub enum AllowlistError {
    /// The entry parses as none of the five forms.
    ///
    /// A scheme, path, or port is refused here rather than stripped: an entry is a
    /// destination, and silently narrowing `https://example.com/x` to a name would
    /// grant something the manifest never said.
    #[error("egress.allow entry {entry:?} is not a name, wildcard name, address, or CIDR block")]
    InvalidEntry { entry: String },
}

impl AllowEntry {
    /// Parse one allowlist entry string.
    ///
    /// # Errors
    ///
    /// Returns [`AllowlistError::InvalidEntry`] when the string is none of the five
    /// specified forms.
    pub fn parse(entry: &str) -> Result<Self, AllowlistError> {
        let invalid = || AllowlistError::InvalidEntry {
            entry: entry.to_owned(),
        };
        if let Ok(net) = IpNet::from_str(entry) {
            return Ok(Self::Net(net));
        }
        if let Ok(addr) = IpAddr::from_str(entry) {
            return Ok(Self::Addr(addr));
        }
        if let Some(suffix) = entry.strip_prefix("**.") {
            let suffix = normalize_name(suffix).ok_or_else(invalid)?;
            return Ok(Self::ManyLabels(suffix));
        }
        if let Some(suffix) = entry.strip_prefix("*.") {
            let suffix = normalize_name(suffix).ok_or_else(invalid)?;
            return Ok(Self::OneLabel(suffix));
        }
        let name = normalize_name(entry).ok_or_else(invalid)?;
        Ok(Self::Exact(name))
    }
}

/// A parsed allowlist, matched against queried names and literal destinations.
#[derive(Clone, Debug, Default)]
pub struct Allowlist {
    entries: Vec<AllowEntry>,
}

impl Allowlist {
    /// Parse every entry, refusing the list on the first invalid one.
    ///
    /// # Errors
    ///
    /// Returns [`AllowlistError::InvalidEntry`] naming the entry that failed.
    pub fn parse(entries: &[String]) -> Result<Self, AllowlistError> {
        let entries = entries
            .iter()
            .map(|entry| AllowEntry::parse(entry))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { entries })
    }

    /// Whether a queried name is permitted.
    ///
    /// The name is normalized the way entries are — lowercased, one trailing dot
    /// stripped — so the match is over DNS's own case-insensitive space.
    #[must_use]
    pub fn matches_name(&self, name: &str) -> bool {
        let Some(name) = normalize_name(name) else {
            return false;
        };
        self.entries.iter().any(|entry| match entry {
            AllowEntry::Exact(exact) => name == *exact,
            AllowEntry::OneLabel(suffix) => name
                .strip_suffix(suffix.as_str())
                .and_then(|head| head.strip_suffix('.'))
                .is_some_and(|label| !label.is_empty() && !label.contains('.')),
            AllowEntry::ManyLabels(suffix) => name
                .strip_suffix(suffix.as_str())
                .and_then(|head| head.strip_suffix('.'))
                .is_some_and(|labels| !labels.is_empty()),
            AllowEntry::Addr(_) | AllowEntry::Net(_) => false,
        })
    }

    /// Whether a literal destination address is permitted without DNS.
    #[must_use]
    pub fn matches_addr(&self, addr: IpAddr) -> bool {
        self.entries.iter().any(|entry| match entry {
            AllowEntry::Addr(allowed) => *allowed == addr,
            AllowEntry::Net(net) => net.contains(&addr),
            AllowEntry::Exact(_) | AllowEntry::OneLabel(_) | AllowEntry::ManyLabels(_) => false,
        })
    }

    /// The literal address and block entries, which the filter is programmed with at
    /// setup rather than per answer.
    #[must_use]
    pub fn literal_destinations(&self) -> Vec<&AllowEntry> {
        self.entries
            .iter()
            .filter(|entry| matches!(entry, AllowEntry::Addr(_) | AllowEntry::Net(_)))
            .collect()
    }
}

/// Lowercase a DNS name and strip one trailing dot, refusing anything that is not a
/// dot-joined sequence of ordinary labels.
///
/// Labels accept letters, digits, hyphen, and underscore — underscore because real
/// destinations carry it — but not a leading or trailing hyphen, not emptiness, and
/// not a length past DNS's own 63-per-label and 253-total budgets.
fn normalize_name(name: &str) -> Option<String> {
    let name = name.strip_suffix('.').unwrap_or(name);
    if name.is_empty() || name.len() > 253 {
        return None;
    }
    let lowered = name.to_ascii_lowercase();
    let valid = lowered.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    });
    valid.then_some(lowered)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn list(entries: &[&str]) -> Allowlist {
        let owned: Vec<String> = entries.iter().map(ToString::to_string).collect();
        Allowlist::parse(&owned).unwrap()
    }

    /// One table over spec/05's name grammar: exact entries match that name alone
    /// (case- and trailing-dot-insensitive), `*.` spans exactly one label, `**.`
    /// spans one or more, and a wildcard is a label suffix, never a substring —
    /// `*.example.com` must not admit `evil-example.com`, which ends with the
    /// suffix's characters but not with its labels.
    #[test]
    fn name_matching_follows_the_pattern_grammar() {
        let rows: &[(&[&str], &str, bool)] = &[
            (&["example.com"], "example.com", true),
            (&["example.com"], "EXAMPLE.COM", true),
            (&["example.com"], "example.com.", true),
            (&["example.com"], "api.example.com", false),
            (&["example.com"], "notexample.com", false),
            (&["*.example.com"], "api.example.com", true),
            (&["*.example.com"], "example.com", false),
            (&["*.example.com"], "a.b.example.com", false),
            (&["**.example.com"], "api.example.com", true),
            (&["**.example.com"], "a.b.example.com", true),
            (&["**.example.com"], "example.com", false),
            (
                &["*.example.com", "**.example.org"],
                "evil-example.com",
                false,
            ),
            (
                &["*.example.com", "**.example.org"],
                "x.badexample.org",
                false,
            ),
        ];
        for (patterns, name, expected) in rows {
            assert_eq!(
                list(patterns).matches_name(name),
                *expected,
                "{patterns:?} vs {name:?}"
            );
        }
    }

    #[test]
    fn literal_addresses_and_blocks_match_addresses_not_names() {
        let allow = list(&[
            "192.0.2.10",
            "2001:db8::1",
            "198.51.100.0/24",
            "2001:db8:1::/48",
        ]);
        assert!(allow.matches_addr("192.0.2.10".parse().unwrap()));
        assert!(!allow.matches_addr("192.0.2.11".parse().unwrap()));
        assert!(allow.matches_addr("2001:db8::1".parse().unwrap()));
        assert!(allow.matches_addr("198.51.100.7".parse().unwrap()));
        assert!(!allow.matches_addr("198.51.101.7".parse().unwrap()));
        assert!(allow.matches_addr("2001:db8:1::42".parse().unwrap()));
        assert!(!allow.matches_name("192.0.2.10"));
        assert_eq!(allow.literal_destinations().len(), 4);
    }

    #[test]
    fn invalid_entries_are_refused_not_narrowed() {
        for entry in [
            "https://example.com",
            "example.com/path",
            "example.com:443",
            "*example.com",
            "a.*.example.com",
            "***.example.com",
            ".example.com",
            "exa mple.com",
            "-bad.example.com",
            "",
            "not a pattern",
        ] {
            assert!(
                AllowEntry::parse(entry).is_err(),
                "entry {entry:?} should be refused"
            );
        }
    }

    #[test]
    fn unmatchable_query_shapes_are_denied_rather_than_erred() {
        let allow = list(&["example.com"]);
        assert!(!allow.matches_name(""));
        assert!(!allow.matches_name("bad..name"));
    }
}
