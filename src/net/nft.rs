//! The typed egress ruleset and its per-answer element additions.
//!
//! Everything here renders to the libnftables JSON schema and is applied through the
//! pinned `nft` binary — the parser trusted with anything guest-driven is nftables'
//! own userland, and addresses enter as typed values rather than authored strings.
//! Denials reject rather than drop (ADR-0044): a blocked `connect()` fails in
//! milliseconds instead of hanging, with one `icmpx` rule covering both families.
//! Q-005's spike record holds the applied-ruleset and element-expiry evidence and the
//! measured cost of one element add through a fresh `nft` process.

use std::collections::HashSet;
use std::net::IpAddr;
use std::path::PathBuf;
use std::process::Stdio;

use nftables::expr::{
    CT, Elem, Expression, Meta, MetaKey, NamedExpression, Payload, PayloadField, Prefix,
};
use nftables::schema::{
    Chain, Element, NfCmd, NfListObject, NfObject, Nftables, Rule, Set, SetFlag, SetType,
    SetTypeValue, Table,
};
use nftables::stmt::{Match, Operator, Reject, RejectType, Statement};
use nftables::types::{NfChainPolicy, NfChainType, NfFamily, NfHook, RejectCode};
use thiserror::Error;
use tokio::io::AsyncWriteExt as _;

use super::allowlist::AllowEntry;
use super::resolver::{FilterInstallError, FilterProgrammer, TimedAddress};

/// The one table this subsystem owns inside the VM's namespace.
pub const TABLE: &str = "vivarium";
/// The IPv4 allowed-address set; elements carry the answering record's TTL.
pub const SET_V4: &str = "allow4";
/// The IPv6 allowed-address set.
pub const SET_V6: &str = "allow6";
/// The IPv4 literal-destination set: address and CIDR entries, programmed at setup.
///
/// A separate interval set rather than more flags on [`SET_V4`], because an
/// interval set with element timeouts is a combination the resolver never needs and
/// nftables has not always supported.
pub const SET_V4_LIT: &str = "allow4net";
/// The IPv6 literal-destination set.
pub const SET_V6_LIT: &str = "allow6net";
/// The default-deny chain on the forward hook, where guest traffic crosses the
/// namespace toward the uplink.
pub const CHAIN: &str = "egress";

/// The base allowlist-mode ruleset: default-deny on the forward hook, established
/// and related replies accepted, TCP into the timed and literal sets, and
/// reject-not-drop for the rest.
///
/// Guest DNS needs no rule here: the resolver listens on the namespace's own
/// gateway address, and locally delivered traffic takes the input hook, which this
/// table leaves unfiltered — the resolver is the namespace's only listener. DNS
/// toward any other destination is forwarded traffic and falls through to the
/// reject tail like every other denial.
#[must_use]
pub fn base_ruleset() -> Nftables<'static> {
    let objects = vec![
        add(NfListObject::Table(Table {
            family: NfFamily::INet,
            name: TABLE.into(),
            handle: None,
        })),
        add(NfListObject::Set(Box::new(timed_set(
            SET_V4,
            SetType::Ipv4Addr,
        )))),
        add(NfListObject::Set(Box::new(timed_set(
            SET_V6,
            SetType::Ipv6Addr,
        )))),
        add(NfListObject::Set(Box::new(interval_set(
            SET_V4_LIT,
            SetType::Ipv4Addr,
        )))),
        add(NfListObject::Set(Box::new(interval_set(
            SET_V6_LIT,
            SetType::Ipv6Addr,
        )))),
        add(NfListObject::Chain(Chain {
            family: NfFamily::INet,
            table: TABLE.into(),
            name: CHAIN.into(),
            newname: None,
            handle: None,
            _type: Some(NfChainType::Filter),
            hook: Some(NfHook::Forward),
            prio: Some(0),
            dev: None,
            policy: Some(NfChainPolicy::Drop),
        })),
        // Replies to flows an accept rule admitted cross this same forward hook in
        // the other direction; without this rule the reject tail would reset the
        // returning half of every permitted connection.
        add(rule(vec![
            Statement::Match(Match {
                left: ct_state(),
                right: Expression::List(vec![
                    string("established".to_owned()),
                    string("related".to_owned()),
                ]),
                op: Operator::IN,
            }),
            Statement::Accept(None),
        ])),
        // The allowlist grants destinations for TCP alone (spec/05): UDP is denied
        // outright except DNS to the resolver, so set membership is not enough.
        add(rule(vec![
            match_eq(meta_l4proto(), string("tcp".to_owned())),
            match_eq(payload("ip", "daddr"), string(format!("@{SET_V4}"))),
            Statement::Accept(None),
        ])),
        add(rule(vec![
            match_eq(meta_l4proto(), string("tcp".to_owned())),
            match_eq(payload("ip6", "daddr"), string(format!("@{SET_V6}"))),
            Statement::Accept(None),
        ])),
        add(rule(vec![
            match_eq(meta_l4proto(), string("tcp".to_owned())),
            match_eq(payload("ip", "daddr"), string(format!("@{SET_V4_LIT}"))),
            Statement::Accept(None),
        ])),
        add(rule(vec![
            match_eq(meta_l4proto(), string("tcp".to_owned())),
            match_eq(payload("ip6", "daddr"), string(format!("@{SET_V6_LIT}"))),
            Statement::Accept(None),
        ])),
        add(rule(vec![
            match_eq(meta_l4proto(), string("tcp".to_owned())),
            Statement::Reject(Some(Reject {
                _type: Some(RejectType::TCPReset),
                expr: None,
            })),
        ])),
        add(rule(vec![Statement::Reject(Some(Reject {
            _type: Some(RejectType::ICMPX),
            expr: Some(RejectCode::AdminProhibited),
        }))])),
    ];
    Nftables {
        objects: objects.into(),
    }
}

/// One answered address entering the allowed set of its family, expiring with the
/// record's TTL.
///
/// The element is an `add`: re-answering a name refreshes the element, and nftables
/// replaces an existing element's timeout rather than erroring.
#[must_use]
pub fn timed_element(addr: IpAddr, ttl_seconds: u32) -> Nftables<'static> {
    timed_elements(&[TimedAddress { addr, ttl_seconds }])
}

/// One answer's addresses entering the allowed sets of their families, each
/// expiring with its record's TTL — the batch [`NftProgrammer`] installs per
/// released answer, through one `nft` process.
#[must_use]
pub fn timed_elements(additions: &[TimedAddress]) -> Nftables<'static> {
    let objects: Vec<NfObject<'static>> = additions
        .iter()
        .map(|addition| {
            let set = match addition.addr {
                IpAddr::V4(_) => SET_V4,
                IpAddr::V6(_) => SET_V6,
            };
            add(NfListObject::Element(Element {
                family: NfFamily::INet,
                table: TABLE.into(),
                name: set.into(),
                elem: vec![Expression::Named(NamedExpression::Elem(Elem {
                    val: Box::new(string(addition.addr.to_string())),
                    timeout: Some(addition.ttl_seconds),
                    expires: None,
                    comment: None,
                    counter: None,
                }))]
                .into(),
            }))
        })
        .collect();
    Nftables {
        objects: objects.into(),
    }
}

/// The allowlist's literal address and CIDR entries entering the interval sets of
/// their families, without expiry.
///
/// Nothing has to resolve for these, so they are programmed once at setup rather
/// than per answer. Name entries are skipped — their addresses arrive through the
/// resolver.
#[must_use]
pub fn literal_elements(entries: &[&AllowEntry]) -> Nftables<'static> {
    let objects: Vec<NfObject<'static>> = entries
        .iter()
        .filter_map(|entry| {
            let (set, value) = match entry {
                AllowEntry::Addr(addr) => (family_literal_set(addr), string(addr.to_string())),
                AllowEntry::Net(net) => (
                    family_literal_set(&net.addr()),
                    Expression::Named(NamedExpression::Prefix(Prefix {
                        addr: Box::new(string(net.addr().to_string())),
                        len: u32::from(net.prefix_len()),
                    })),
                ),
                AllowEntry::Exact(_) | AllowEntry::OneLabel(_) | AllowEntry::ManyLabels(_) => {
                    return None;
                }
            };
            Some(add(NfListObject::Element(Element {
                family: NfFamily::INet,
                table: TABLE.into(),
                name: set.into(),
                elem: vec![value].into(),
            })))
        })
        .collect();
    Nftables {
        objects: objects.into(),
    }
}

const fn family_literal_set(addr: &IpAddr) -> &'static str {
    match addr {
        IpAddr::V4(_) => SET_V4_LIT,
        IpAddr::V6(_) => SET_V6_LIT,
    }
}

fn timed_set(name: &'static str, set_type: SetType) -> Set<'static> {
    Set {
        family: NfFamily::INet,
        table: TABLE.into(),
        name: name.into(),
        handle: None,
        set_type: SetTypeValue::Single(set_type),
        policy: None,
        flags: Some(HashSet::from([SetFlag::Timeout])),
        elem: None,
        timeout: None,
        gc_interval: None,
        size: None,
        comment: None,
    }
}

fn interval_set(name: &'static str, set_type: SetType) -> Set<'static> {
    Set {
        family: NfFamily::INet,
        table: TABLE.into(),
        name: name.into(),
        handle: None,
        set_type: SetTypeValue::Single(set_type),
        policy: None,
        flags: Some(HashSet::from([SetFlag::Interval])),
        elem: None,
        timeout: None,
        gc_interval: None,
        size: None,
        comment: None,
    }
}

/// The application seam: a rendered ruleset fed to the pinned `nft` binary.
///
/// The payload goes to `nft -j -f -` as libnftables JSON — directly when the caller
/// is already inside the namespace, or through the `nsenter` join when it is not.
#[derive(Clone, Debug)]
pub struct NftRunner {
    program: PathBuf,
    leading_args: Vec<String>,
}

impl NftRunner {
    /// Run `nft` directly — the shape for a process already inside the pair, which
    /// is where the resolver lives.
    pub fn direct(nft: impl Into<PathBuf>) -> Self {
        Self {
            program: nft.into(),
            leading_args: Vec::new(),
        }
    }

    /// Run `nft` through the `nsenter` join into the pair held by `holder_pid` —
    /// the shape for the supervisor, which stays outside.
    pub fn entered(nsenter: impl Into<PathBuf>, holder_pid: u32, nft: &str) -> Self {
        Self {
            program: nsenter.into(),
            leading_args: super::netns::enter_pair_args(holder_pid, &[nft.to_owned()]),
        }
    }

    /// Apply one rendered payload through a fresh `nft` process.
    ///
    /// A fresh process per apply is the measured shape: Q-005's spike put one
    /// element add at a low-milliseconds median, well under the upstream lookup it
    /// accompanies.
    ///
    /// # Errors
    ///
    /// Returns [`NftApplyError`] when the payload cannot be rendered, the process
    /// cannot be spawned or fed, or `nft` itself rejects the payload.
    pub async fn apply(&self, payload: &Nftables<'_>) -> Result<(), NftApplyError> {
        let rendered = serde_json::to_vec(payload)?;
        let mut child = tokio::process::Command::new(&self.program)
            .args(&self.leading_args)
            .args(["-j", "-f", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| NftApplyError::Spawn {
                program: self.program.clone(),
                source,
            })?;
        let Some(mut stdin) = child.stdin.take() else {
            return Err(NftApplyError::Feed {
                reason: "the spawned process has no stdin".to_owned(),
            });
        };
        stdin
            .write_all(&rendered)
            .await
            .map_err(|error| NftApplyError::Feed {
                reason: error.to_string(),
            })?;
        drop(stdin);
        let output = child
            .wait_with_output()
            .await
            .map_err(|error| NftApplyError::Feed {
                reason: error.to_string(),
            })?;
        if output.status.success() {
            Ok(())
        } else {
            Err(NftApplyError::Rejected {
                status: output.status.to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            })
        }
    }
}

/// An application that did not take; the kernel's ruleset is unchanged.
#[derive(Debug, Error)]
pub enum NftApplyError {
    /// The typed ruleset would not render to JSON.
    #[error("rendering the ruleset to JSON failed: {0}")]
    Render(#[from] serde_json::Error),
    /// The `nft` (or wrapping `nsenter`) process would not start.
    #[error("spawning {program:?} failed: {source}")]
    Spawn {
        program: PathBuf,
        source: std::io::Error,
    },
    /// The payload could not be written to, or the result read from, the process.
    #[error("feeding the ruleset to `nft` failed: {reason}")]
    Feed { reason: String },
    /// `nft` parsed the payload and refused it.
    #[error("`nft` rejected the ruleset ({status}): {stderr}")]
    Rejected { status: String, stderr: String },
}

/// The [`FilterProgrammer`] the wired resolver uses.
///
/// Each answer's addresses become one timed-element batch through one `nft`
/// process, and the install has not happened until that process exits zero — which
/// is what lets [`gate_query`] treat the return as the addresses being enforceable.
///
/// [`gate_query`]: super::resolver::gate_query
#[derive(Clone, Debug)]
pub struct NftProgrammer {
    runner: NftRunner,
}

impl NftProgrammer {
    #[must_use]
    pub const fn new(runner: NftRunner) -> Self {
        Self { runner }
    }
}

impl FilterProgrammer for NftProgrammer {
    async fn install(&mut self, additions: &[TimedAddress]) -> Result<(), FilterInstallError> {
        if additions.is_empty() {
            return Ok(());
        }
        self.runner
            .apply(&timed_elements(additions))
            .await
            .map_err(|error| FilterInstallError {
                reason: error.to_string(),
            })
    }
}

const fn add(object: NfListObject<'static>) -> NfObject<'static> {
    NfObject::CmdObject(NfCmd::Add(object))
}

fn rule(expr: Vec<Statement<'static>>) -> NfListObject<'static> {
    NfListObject::Rule(Rule {
        family: NfFamily::INet,
        table: TABLE.into(),
        chain: CHAIN.into(),
        expr: expr.into(),
        handle: None,
        index: None,
        comment: None,
    })
}

const fn match_eq(left: Expression<'static>, right: Expression<'static>) -> Statement<'static> {
    Statement::Match(Match {
        left,
        right,
        op: Operator::EQ,
    })
}

fn payload(protocol: &'static str, field: &'static str) -> Expression<'static> {
    Expression::Named(NamedExpression::Payload(Payload::PayloadField(
        PayloadField {
            protocol: protocol.into(),
            field: field.into(),
        },
    )))
}

const fn meta_l4proto() -> Expression<'static> {
    Expression::Named(NamedExpression::Meta(Meta {
        key: MetaKey::L4proto,
    }))
}

fn ct_state() -> Expression<'static> {
    Expression::Named(NamedExpression::CT(CT {
        key: "state".into(),
        family: None,
        dir: None,
    }))
}

fn string(value: String) -> Expression<'static> {
    Expression::String(value.into())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    // The table, set, chain, and reject shapes are the ones the Q-005 spike
    // applied through `nft -j -f` inside an unprivileged namespace pair, so a
    // rendering that drifts from them is a rendering `nft` has not accepted. The
    // ct-state and l4proto guards and the interval literal sets joined later; the
    // gated `net_host` trial applies the whole rendering through the real `nft`.
    //
    // Whole-value goldens rather than index-addressed spot checks, because a
    // spot check cannot notice an inserted or reordered object. The pinned text
    // necessarily encodes some of the `nftables` crate's serialization (field
    // order, `null` optionals), so a crate upgrade that moves the rendering
    // fails these on purpose: the new bytes are what `nft` will be fed, and
    // accepting the diff is a decision, not a re-record.

    fn tcp_daddr_accept(protocol: &str, set: &str) -> serde_json::Value {
        json!({"add": {"rule": {
            "family": "inet", "table": "vivarium", "chain": "egress",
            "expr": [
                {"match": {
                    "left": {"meta": {"key": "l4proto"}},
                    "right": "tcp", "op": "=="
                }},
                {"match": {
                    "left": {"payload": {"protocol": protocol, "field": "daddr"}},
                    "right": set, "op": "=="
                }},
                {"accept": null}
            ]
        }}})
    }

    #[test]
    fn base_ruleset_renders_the_spike_accepted_shape() {
        let rendered = serde_json::to_value(base_ruleset()).unwrap();
        assert_eq!(
            rendered,
            json!({"nftables": [
                {"add": {"table": {"family": "inet", "name": "vivarium"}}},
                {"add": {"set": {
                    "family": "inet", "table": "vivarium", "name": "allow4",
                    "type": "ipv4_addr", "flags": ["timeout"]
                }}},
                {"add": {"set": {
                    "family": "inet", "table": "vivarium", "name": "allow6",
                    "type": "ipv6_addr", "flags": ["timeout"]
                }}},
                {"add": {"set": {
                    "family": "inet", "table": "vivarium", "name": "allow4net",
                    "type": "ipv4_addr", "flags": ["interval"]
                }}},
                {"add": {"set": {
                    "family": "inet", "table": "vivarium", "name": "allow6net",
                    "type": "ipv6_addr", "flags": ["interval"]
                }}},
                {"add": {"chain": {
                    "family": "inet", "table": "vivarium", "name": "egress",
                    "type": "filter", "hook": "forward", "prio": 0, "policy": "drop"
                }}},
                {"add": {"rule": {
                    "family": "inet", "table": "vivarium", "chain": "egress",
                    "expr": [
                        {"match": {
                            "left": {"ct": {"key": "state"}},
                            "right": ["established", "related"], "op": "in"
                        }},
                        {"accept": null}
                    ]
                }}},
                tcp_daddr_accept("ip", "@allow4"),
                tcp_daddr_accept("ip6", "@allow6"),
                tcp_daddr_accept("ip", "@allow4net"),
                tcp_daddr_accept("ip6", "@allow6net"),
                {"add": {"rule": {
                    "family": "inet", "table": "vivarium", "chain": "egress",
                    "expr": [
                        {"match": {
                            "left": {"meta": {"key": "l4proto"}},
                            "right": "tcp", "op": "=="
                        }},
                        {"reject": {"type": "tcp reset"}}
                    ]
                }}},
                {"add": {"rule": {
                    "family": "inet", "table": "vivarium", "chain": "egress",
                    "expr": [
                        {"reject": {"type": "icmpx", "expr": "admin-prohibited"}}
                    ]
                }}}
            ]})
        );
    }

    #[test]
    fn literal_entries_render_into_the_interval_sets_without_expiry() {
        let entries = [
            AllowEntry::parse("192.0.2.10").unwrap(),
            AllowEntry::parse("198.51.100.0/24").unwrap(),
            AllowEntry::parse("2001:db8::/32").unwrap(),
            AllowEntry::parse("example.com").unwrap(),
        ];
        let refs: Vec<&AllowEntry> = entries.iter().collect();
        let rendered = serde_json::to_value(literal_elements(&refs)).unwrap();
        // Three objects from four entries: the name entry contributes nothing,
        // because its addresses arrive per released answer.
        assert_eq!(
            rendered,
            json!({"nftables": [
                {"add": {"element": {
                    "family": "inet", "table": "vivarium", "name": "allow4net",
                    "elem": ["192.0.2.10"]
                }}},
                {"add": {"element": {
                    "family": "inet", "table": "vivarium", "name": "allow4net",
                    "elem": [{"prefix": {"addr": "198.51.100.0", "len": 24}}]
                }}},
                {"add": {"element": {
                    "family": "inet", "table": "vivarium", "name": "allow6net",
                    "elem": [{"prefix": {"addr": "2001:db8::", "len": 32}}]
                }}}
            ]})
        );
    }

    #[tokio::test]
    async fn a_runner_feeds_the_payload_and_reads_the_verdict() {
        // A stand-in that, like `nft`, swallows its flags and stdin and exits by its
        // own verdict. The runner's job — spawn, feed, map the exit status — is the
        // same either way; the pinned `nft` itself meets the rendering in the gated
        // host trial.
        use std::os::unix::fs::PermissionsExt as _;
        let scratch = crate::test_support::ScratchDirectory::new().unwrap();
        let accepting = scratch.path().join("accepts");
        std::fs::write(&accepting, "#!/bin/sh\ncat >/dev/null\nexit 0\n").unwrap();
        std::fs::set_permissions(&accepting, std::fs::Permissions::from_mode(0o755)).unwrap();
        let payload = timed_element("192.0.2.1".parse().unwrap(), 5);

        let accepts = NftRunner::direct(&accepting);
        assert!(accepts.apply(&payload).await.is_ok());

        let rejecting = scratch.path().join("rejects");
        std::fs::write(&rejecting, "#!/bin/sh\ncat >/dev/null\nexit 1\n").unwrap();
        std::fs::set_permissions(&rejecting, std::fs::Permissions::from_mode(0o755)).unwrap();
        let rejects = NftRunner::direct(&rejecting);
        assert!(matches!(
            rejects.apply(&payload).await,
            Err(NftApplyError::Rejected { .. })
        ));

        let missing = NftRunner::direct("/nonexistent/nft");
        assert!(matches!(
            missing.apply(&payload).await,
            Err(NftApplyError::Spawn { .. })
        ));
    }

    #[tokio::test]
    async fn the_programmer_treats_an_empty_answer_as_installed() {
        // A NoError answer with no A or AAAA records has nothing to enforce; the
        // release must not hang on a process that was never needed. The runner
        // points at a path that cannot spawn, so `Ok` is reachable only if no
        // process was started at all — which is the claim.
        let mut programmer = NftProgrammer::new(NftRunner::direct("/nonexistent/nft"));
        assert!(
            programmer.install(&[]).await.is_ok(),
            "an empty batch spawned (or tried to spawn) an nft process"
        );
    }

    #[test]
    fn timed_elements_target_each_family_set_with_the_ttl() {
        let batch = timed_elements(&[
            TimedAddress {
                addr: "192.0.2.10".parse().unwrap(),
                ttl_seconds: 300,
            },
            TimedAddress {
                addr: "2001:db8::1".parse().unwrap(),
                ttl_seconds: 60,
            },
        ]);
        assert_eq!(
            serde_json::to_value(batch).unwrap(),
            json!({"nftables": [
                {"add": {"element": {
                    "family": "inet", "table": "vivarium", "name": "allow4",
                    "elem": [{"elem": {
                        "val": "192.0.2.10", "timeout": 300,
                        "expires": null, "comment": null, "counter": null
                    }}]
                }}},
                {"add": {"element": {
                    "family": "inet", "table": "vivarium", "name": "allow6",
                    "elem": [{"elem": {
                        "val": "2001:db8::1", "timeout": 60,
                        "expires": null, "comment": null, "counter": null
                    }}]
                }}}
            ]})
        );
    }
}
