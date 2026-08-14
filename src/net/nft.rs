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

use nftables::expr::{CT, Elem, Expression, Meta, MetaKey, NamedExpression, Payload, PayloadField};
use nftables::schema::{
    Chain, Element, NfCmd, NfListObject, NfObject, Nftables, Rule, Set, SetFlag, SetType,
    SetTypeValue, Table,
};
use nftables::stmt::{Match, Operator, Reject, RejectType, Statement};
use nftables::types::{NfChainPolicy, NfChainType, NfFamily, NfHook, RejectCode};

/// The one table this subsystem owns inside the VM's namespace.
pub const TABLE: &str = "vivarium";
/// The IPv4 allowed-address set; elements carry the answering record's TTL.
pub const SET_V4: &str = "allow4";
/// The IPv6 allowed-address set.
pub const SET_V6: &str = "allow6";
/// The default-deny chain on the forward hook, where guest traffic crosses the
/// namespace toward the uplink.
pub const CHAIN: &str = "egress";

/// The base allowlist-mode ruleset: default-deny, established and related replies
/// accepted, TCP into the two timed sets, DNS permitted to the gating resolver
/// alone, and reject-not-drop for the rest.
#[must_use]
pub fn base_ruleset(resolver: IpAddr, resolver_port: u16) -> Nftables<'static> {
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
        // The resolver is the only DNS destination; a guest asking anyone else falls
        // through to the reject rules like any other denied traffic.
        add(rule(vec![
            match_eq(
                payload(ip_protocol(resolver), "daddr"),
                string(resolver.to_string()),
            ),
            match_eq(
                payload("udp", "dport"),
                Expression::Number(u32::from(resolver_port)),
            ),
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
    let set = match addr {
        IpAddr::V4(_) => SET_V4,
        IpAddr::V6(_) => SET_V6,
    };
    let element = Element {
        family: NfFamily::INet,
        table: TABLE.into(),
        name: set.into(),
        elem: vec![Expression::Named(NamedExpression::Elem(Elem {
            val: Box::new(string(addr.to_string())),
            timeout: Some(ttl_seconds),
            expires: None,
            comment: None,
            counter: None,
        }))]
        .into(),
    };
    Nftables {
        objects: vec![add(NfListObject::Element(element))].into(),
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

const fn ip_protocol(addr: IpAddr) -> &'static str {
    match addr {
        IpAddr::V4(_) => "ip",
        IpAddr::V6(_) => "ip6",
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

    // The table, set, chain, DNS, and reject shapes are the ones the Q-005 spike
    // applied through `nft -j -f` inside an unprivileged namespace pair, so a
    // rendering that drifts from them is a rendering `nft` has not accepted. The
    // ct-state and l4proto guards joined at review; they use the same libnftables
    // JSON schema forms and meet real `nft` in the wiring slice's packet-level trial.

    #[test]
    fn base_ruleset_renders_the_spike_accepted_shape() {
        let rendered =
            serde_json::to_value(base_ruleset("10.177.0.1".parse().unwrap(), 53)).unwrap();
        let objects = rendered.get("nftables").unwrap().as_array().unwrap();
        assert_eq!(
            objects[0],
            json!({"add": {"table": {"family": "inet", "name": "vivarium"}}})
        );
        assert_eq!(
            objects[1],
            json!({"add": {"set": {
                "family": "inet", "table": "vivarium", "name": "allow4",
                "type": "ipv4_addr", "flags": ["timeout"]
            }}})
        );
        assert_eq!(
            objects[3],
            json!({"add": {"chain": {
                "family": "inet", "table": "vivarium", "name": "egress",
                "type": "filter", "hook": "forward", "prio": 0, "policy": "drop"
            }}})
        );
        assert_eq!(
            objects[4]["add"]["rule"]["expr"][0],
            json!({"match": {
                "left": {"ct": {"key": "state"}},
                "right": ["established", "related"], "op": "in"
            }})
        );
        let dns_rule = &objects[5]["add"]["rule"]["expr"];
        assert_eq!(
            dns_rule[0],
            json!({"match": {
                "left": {"payload": {"protocol": "ip", "field": "daddr"}},
                "right": "10.177.0.1", "op": "=="
            }})
        );
        assert_eq!(
            dns_rule[1]["match"]["left"],
            json!({"payload": {"protocol": "udp", "field": "dport"}})
        );
        let allow4_rule = &objects[6]["add"]["rule"]["expr"];
        assert_eq!(
            allow4_rule[0]["match"]["left"],
            json!({"meta": {"key": "l4proto"}})
        );
        assert_eq!(allow4_rule[0]["match"]["right"], json!("tcp"));
        assert_eq!(allow4_rule[1]["match"]["right"], json!("@allow4"));
        let allow6_rule = &objects[7]["add"]["rule"]["expr"];
        assert_eq!(allow6_rule[0]["match"]["right"], json!("tcp"));
        assert_eq!(allow6_rule[1]["match"]["right"], json!("@allow6"));
        let tcp_reject = &objects[8]["add"]["rule"]["expr"];
        assert_eq!(
            tcp_reject[0]["match"]["left"],
            json!({"meta": {"key": "l4proto"}})
        );
        assert_eq!(tcp_reject[1], json!({"reject": {"type": "tcp reset"}}));
        assert_eq!(
            objects[9]["add"]["rule"]["expr"][0],
            json!({"reject": {"type": "icmpx", "expr": "admin-prohibited"}})
        );
    }

    #[test]
    fn timed_element_targets_the_family_set_with_the_ttl() {
        let v4 = serde_json::to_value(timed_element("192.0.2.10".parse().unwrap(), 300)).unwrap();
        assert_eq!(
            v4["nftables"][0],
            json!({"add": {"element": {
                "family": "inet", "table": "vivarium", "name": "allow4",
                "elem": [{"elem": {
                    "val": "192.0.2.10", "timeout": 300,
                    "expires": null, "comment": null, "counter": null
                }}]
            }}})
        );
        let v6 = serde_json::to_value(timed_element("2001:db8::1".parse().unwrap(), 60)).unwrap();
        assert_eq!(v6["nftables"][0]["add"]["element"]["name"], json!("allow6"));
    }
}
