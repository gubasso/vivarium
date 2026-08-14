//! The gating resolver: the ordering between filter programming and answer release,
//! and the UDP serve loop that carries it.
//!
//! spec/05 specifies this as an ordering, not an optimization, and the shape of this
//! module is that sentence made structural: the upstream answer's bytes are returned
//! only after the filter programmer has accepted every address in it, and a name no
//! entry matches is answered `REFUSED` without the upstream ever seeing it — a denied
//! name is not even leaked. Upstream failures pass through as themselves — with the
//! one exception of an upstream `REFUSED`, downgraded to `SERVFAIL` — so `REFUSED`
//! means policy, unambiguously. [`serve`] is the loop the wired resolver runs inside
//! the VM's namespace: UDP only, because the filter permits the guest no other DNS
//! path, one upstream exchange per query through the namespace's uplink.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use hickory_proto::op::{Message, MessageType, OpCode, ResponseCode};
use hickory_proto::rr::{RData, RecordType};
use thiserror::Error;
use tokio::net::UdpSocket;

use super::allowlist::Allowlist;

/// The largest datagram either side of the serve loop accepts: the EDNS ceiling.
const MAX_DATAGRAM: usize = 65_535;

/// One answered address and the record TTL that bounds its life in the filter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimedAddress {
    pub addr: IpAddr,
    pub ttl_seconds: u32,
}

/// The seam between the resolver and the filter.
///
/// The contract is that [`gate_query`] treats `install` returning as the moment the
/// addresses became enforceable; an implementation must not return before its rules
/// are in the kernel.
pub trait FilterProgrammer {
    /// Install or refresh the given addresses in the allowed set.
    fn install(
        &mut self,
        additions: &[TimedAddress],
    ) -> impl Future<Output = Result<(), FilterInstallError>> + Send;
}

/// A filter installation that did not take; the answer it gated is withheld.
#[derive(Debug, Error)]
#[error("filter installation failed: {reason}")]
pub struct FilterInstallError {
    pub reason: String,
}

#[derive(Debug, Error)]
pub enum GateError {
    /// The guest's datagram is not a DNS query this resolver can answer.
    #[error("malformed query from the guest")]
    MalformedQuery,
    /// The upstream transport failed; the caller answers `SERVFAIL`.
    #[error("upstream resolver unreachable: {reason}")]
    Upstream { reason: String },
    /// A reply could not be encoded.
    #[error("encoding a response failed: {0}")]
    Encode(#[from] hickory_proto::ProtoError),
}

/// Gate one guest query: refuse, or forward, program the filter, and only then
/// release the upstream's own bytes.
///
/// `forward` carries the query to the host's configured resolver and returns the raw
/// response; `matches` is the allowlist's name predicate. Only a QUERY-opcode query
/// with exactly one question reaches the allowlist: anything else is answered
/// locally — `NOTIMP` for an unsupported opcode or the zero-question form,
/// `FORMERR` for a multi-question query per RFC 9619 — so no unchecked name rides
/// a forwarded packet. With `withhold_aaaa`, a permitted AAAA question is answered
/// `NoError` with no records instead of being forwarded: the host has no working
/// IPv6 path, and spec/05 has the resolver withhold the family rather than let the
/// guest attempt a connection that cannot complete — denial still wins, so a denied
/// name's AAAA is `REFUSED`, not empty. The response is released byte-verbatim so
/// upstream `NXDOMAIN` and `SERVFAIL` pass through as themselves — except upstream
/// `REFUSED`, which is downgraded to `SERVFAIL` so `REFUSED` keeps its policy-only
/// meaning; when the filter refuses an installation, the answer is withheld and the
/// guest gets `SERVFAIL`, because releasing it would let a connection race the rule
/// that was never installed.
///
/// # Errors
///
/// Returns [`GateError`] when the query is malformed, the upstream transport fails,
/// or a reply cannot be encoded; the serve loop owns turning those into wire answers
/// where one is possible.
pub async fn gate_query<F, Fwd, Fut>(
    query_bytes: &[u8],
    matches: impl Fn(&str) -> bool + Send,
    withhold_aaaa: bool,
    forward: Fwd,
    programmer: &mut F,
) -> Result<Vec<u8>, GateError>
where
    F: FilterProgrammer + Send,
    Fwd: FnOnce(Vec<u8>) -> Fut + Send,
    Fut: Future<Output = Result<Vec<u8>, GateError>> + Send,
{
    let query = Message::from_vec(query_bytes).map_err(|_| GateError::MalformedQuery)?;
    if query.metadata.message_type != MessageType::Query {
        // A response-shaped packet gets no answer at all; answering it would let a
        // guest use this socket as a reflector.
        return Err(GateError::MalformedQuery);
    }
    if query.metadata.op_code != OpCode::Query {
        return Ok(reply(&query, ResponseCode::NotImp)?);
    }
    // RFC 9619: a QUERY with more than one question is FORMERR. Gating only the
    // first question while forwarding the whole packet would leak every other
    // question's name upstream unchecked.
    if query.queries.len() > 1 {
        return Ok(reply(&query, ResponseCode::FormErr)?);
    }
    // A zero-question QUERY is well-formed — RFC 9619 keeps it legal for DNS
    // Cookies — but this resolver implements no zero-question feature, so it is
    // unsupported rather than malformed, and never REFUSED, which means policy.
    let Some(question) = query.queries.first() else {
        return Ok(reply(&query, ResponseCode::NotImp)?);
    };
    let name = question.name().to_ascii();
    let (id, op_code) = (query.metadata.id, query.metadata.op_code);

    if !matches(&name) {
        return Ok(reply(&query, ResponseCode::Refused)?);
    }

    if withhold_aaaa && question.query_type() == RecordType::AAAA {
        return Ok(reply(&query, ResponseCode::NoError)?);
    }

    let response_bytes = forward(query_bytes.to_vec()).await?;
    let response = Message::from_vec(&response_bytes).map_err(|_| GateError::Upstream {
        reason: "upstream returned an unparsable response".to_owned(),
    })?;
    if response.metadata.response_code != ResponseCode::NoError {
        // An upstream REFUSED would wear the code that means vivarium policy
        // (spec/05); it is downgraded to SERVFAIL so REFUSED stays unambiguous.
        if response.metadata.response_code == ResponseCode::Refused {
            return Ok(reply(&query, ResponseCode::ServFail)?);
        }
        return Ok(response_bytes);
    }

    let additions: Vec<TimedAddress> = response
        .answers
        .iter()
        .filter_map(|record| {
            let addr = match &record.data {
                RData::A(a) => IpAddr::V4(a.0),
                RData::AAAA(aaaa) => IpAddr::V6(aaaa.0),
                _ => return None,
            };
            Some(TimedAddress {
                addr,
                ttl_seconds: record.ttl,
            })
        })
        .collect();

    if programmer.install(&additions).await.is_err() {
        let mut servfail = Message::error_msg(id, op_code, ResponseCode::ServFail);
        servfail.add_queries(query.queries.iter().cloned());
        return Ok(servfail.to_vec()?);
    }

    Ok(response_bytes)
}

/// How [`serve`] reaches upstream and shapes answers.
#[derive(Clone, Copy, Debug)]
pub struct ServeConfig {
    /// The host's configured resolver, reached through the namespace's uplink.
    pub upstream: SocketAddr,
    /// Answer permitted AAAA questions empty because the host has no working IPv6
    /// path (spec/05's family withholding).
    pub withhold_aaaa: bool,
    /// How long one upstream exchange may take before the guest gets `SERVFAIL`.
    pub upstream_timeout: Duration,
}

/// Serve gated queries on `socket` until the socket itself fails.
///
/// One query at a time: the gate's filter installation is a low-milliseconds
/// subprocess and the upstream exchange dominates, so ordering stays trivially
/// intact without a concurrent map of in-flight installs. Each exchange uses a
/// fresh ephemeral upstream socket, so upstream ids never cross queries. A
/// malformed or response-shaped datagram is dropped without an answer — answering
/// would make the socket a reflector — and an upstream failure becomes `SERVFAIL`
/// where the query was well-formed enough to answer at all.
///
/// # Errors
///
/// Returns the socket error that ended the loop; a serve loop that returns is a
/// resolver that can no longer hear its guest.
pub async fn serve<F>(
    socket: UdpSocket,
    allowlist: Allowlist,
    config: ServeConfig,
    mut programmer: F,
) -> std::io::Result<()>
where
    F: FilterProgrammer + Send,
{
    let mut buf = vec![0u8; MAX_DATAGRAM];
    loop {
        let (len, from) = socket.recv_from(&mut buf).await?;
        let query_bytes = &buf[..len];
        let outcome = gate_query(
            query_bytes,
            |name| allowlist.matches_name(name),
            config.withhold_aaaa,
            |bytes: Vec<u8>| exchange_upstream(bytes, config.upstream, config.upstream_timeout),
            &mut programmer,
        )
        .await;
        let answer = match outcome {
            Ok(bytes) => Some(bytes),
            Err(GateError::Upstream { .. }) => servfail_for(query_bytes),
            Err(GateError::MalformedQuery | GateError::Encode(_)) => None,
        };
        if let Some(bytes) = answer {
            // A guest that vanished mid-answer is its problem, not the loop's.
            let _ = socket.send_to(&bytes, from).await;
        }
    }
}

/// One upstream exchange over a fresh ephemeral socket.
async fn exchange_upstream(
    bytes: Vec<u8>,
    upstream: SocketAddr,
    timeout: Duration,
) -> Result<Vec<u8>, GateError> {
    let upstream_error = |reason: String| GateError::Upstream { reason };
    let bind_addr: SocketAddr = if upstream.is_ipv4() {
        "0.0.0.0:0"
            .parse()
            .map_err(|_| upstream_error("unbindable".to_owned()))?
    } else {
        "[::]:0"
            .parse()
            .map_err(|_| upstream_error("unbindable".to_owned()))?
    };
    let socket = UdpSocket::bind(bind_addr)
        .await
        .map_err(|error| upstream_error(error.to_string()))?;
    socket
        .connect(upstream)
        .await
        .map_err(|error| upstream_error(error.to_string()))?;
    socket
        .send(&bytes)
        .await
        .map_err(|error| upstream_error(error.to_string()))?;
    let mut buf = vec![0u8; MAX_DATAGRAM];
    let len = tokio::time::timeout(timeout, socket.recv(&mut buf))
        .await
        .map_err(|_| upstream_error(format!("no answer within {} ms", timeout.as_millis())))?
        .map_err(|error| upstream_error(error.to_string()))?;
    buf.truncate(len);
    Ok(buf)
}

/// A `SERVFAIL` for the guest's own query, where the query parses well enough to
/// answer; `None` where it does not, and the datagram is dropped instead.
fn servfail_for(query_bytes: &[u8]) -> Option<Vec<u8>> {
    let query = Message::from_vec(query_bytes).ok()?;
    if query.metadata.message_type != MessageType::Query {
        return None;
    }
    reply(&query, ResponseCode::ServFail).ok()
}

/// A response carrying the query's own id, opcode, and question, with the given code.
fn reply(query: &Message, code: ResponseCode) -> Result<Vec<u8>, hickory_proto::ProtoError> {
    let mut response = Message::error_msg(query.metadata.id, query.metadata.op_code, code);
    response.add_queries(query.queries.iter().cloned());
    response.metadata.recursion_desired = query.metadata.recursion_desired;
    response.to_vec()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use hickory_proto::op::{MessageType, OpCode, Query};
    use hickory_proto::rr::rdata::{A, AAAA};
    use hickory_proto::rr::{Name, Record, RecordType};
    use std::net::Ipv4Addr;
    use std::str::FromStr;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Recording {
        installed: Vec<Vec<TimedAddress>>,
        refuse: bool,
    }

    impl Recording {
        const fn new(refuse: bool) -> Self {
            Self {
                installed: Vec::new(),
                refuse,
            }
        }
    }

    impl FilterProgrammer for Recording {
        async fn install(&mut self, additions: &[TimedAddress]) -> Result<(), FilterInstallError> {
            if self.refuse {
                return Err(FilterInstallError {
                    reason: "test refusal".to_owned(),
                });
            }
            self.installed.push(additions.to_vec());
            Ok(())
        }
    }

    fn query_bytes(name: &str, id: u16) -> Vec<u8> {
        typed_query_bytes(name, id, RecordType::A)
    }

    fn typed_query_bytes(name: &str, id: u16, record_type: RecordType) -> Vec<u8> {
        let mut query = Message::new(id, MessageType::Query, OpCode::Query);
        query.add_query(Query::query(Name::from_str(name).unwrap(), record_type));
        query.to_vec().unwrap()
    }

    fn answer_bytes(name: &str, id: u16, addrs: &[(IpAddr, u32)]) -> Vec<u8> {
        let mut response = Message::response(id, OpCode::Query);
        response.add_query(Query::query(Name::from_str(name).unwrap(), RecordType::A));
        for (addr, ttl) in addrs {
            let record = match addr {
                IpAddr::V4(v4) => {
                    Record::from_rdata(Name::from_str(name).unwrap(), *ttl, RData::A(A(*v4)))
                }
                IpAddr::V6(v6) => {
                    Record::from_rdata(Name::from_str(name).unwrap(), *ttl, RData::AAAA(AAAA(*v6)))
                }
            };
            response.add_answer(record);
        }
        response.to_vec().unwrap()
    }

    #[tokio::test]
    async fn a_denied_name_is_refused_and_never_forwarded() {
        let forwarded = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&forwarded);
        let mut programmer = Recording::new(false);
        let reply = gate_query(
            &query_bytes("blocked.example.", 7),
            |_| false,
            false,
            move |_bytes: Vec<u8>| {
                counter.fetch_add(1, Ordering::SeqCst);
                async move { Ok(Vec::new()) }
            },
            &mut programmer,
        )
        .await
        .unwrap();
        let parsed = Message::from_vec(&reply).unwrap();
        assert_eq!(parsed.metadata.response_code, ResponseCode::Refused);
        assert_eq!(parsed.metadata.id, 7);
        assert_eq!(
            forwarded.load(Ordering::SeqCst),
            0,
            "the denied name leaked upstream"
        );
        assert!(programmer.installed.is_empty());
    }

    #[tokio::test]
    async fn a_second_question_cannot_ride_a_forwarded_packet() {
        // RFC 9619: QDCOUNT greater than one is FORMERR. The packet must never
        // reach the upstream, or its unchecked second name would leak.
        let mut query = Message::new(15, MessageType::Query, OpCode::Query);
        query.add_query(Query::query(
            Name::from_str("allowed.example.com.").unwrap(),
            RecordType::A,
        ));
        query.add_query(Query::query(
            Name::from_str("denied.example.com.").unwrap(),
            RecordType::A,
        ));
        let forwarded = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&forwarded);
        let mut programmer = Recording::new(false);
        let reply = gate_query(
            &query.to_vec().unwrap(),
            |name| name == "allowed.example.com.",
            false,
            move |_bytes: Vec<u8>| {
                counter.fetch_add(1, Ordering::SeqCst);
                async move { Ok(Vec::new()) }
            },
            &mut programmer,
        )
        .await
        .unwrap();
        let parsed = Message::from_vec(&reply).unwrap();
        assert_eq!(parsed.metadata.response_code, ResponseCode::FormErr);
        assert_eq!(
            forwarded.load(Ordering::SeqCst),
            0,
            "a multi-question packet leaked upstream"
        );
        assert!(programmer.installed.is_empty());
    }

    #[tokio::test]
    async fn a_zero_question_query_is_unsupported_not_malformed() {
        // RFC 9619 keeps QDCOUNT=0 legal (DNS Cookies); with no zero-question
        // feature implemented here, the answer is a local NOTIMP, never a forward.
        let query = Message::new(16, MessageType::Query, OpCode::Query);
        let forwarded = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&forwarded);
        let mut programmer = Recording::new(false);
        let reply = gate_query(
            &query.to_vec().unwrap(),
            |_| true,
            false,
            move |_bytes: Vec<u8>| {
                counter.fetch_add(1, Ordering::SeqCst);
                async move { Ok(Vec::new()) }
            },
            &mut programmer,
        )
        .await
        .unwrap();
        let parsed = Message::from_vec(&reply).unwrap();
        assert_eq!(parsed.metadata.response_code, ResponseCode::NotImp);
        assert_eq!(forwarded.load(Ordering::SeqCst), 0);
        assert!(programmer.installed.is_empty());
    }

    #[tokio::test]
    async fn an_upstream_refused_is_downgraded_to_servfail() {
        // REFUSED means vivarium policy; an upstream resolver's own refusal must not
        // wear the same code, so the guest sees SERVFAIL instead.
        let mut refused = Message::error_msg(17, OpCode::Query, ResponseCode::Refused);
        refused.add_query(Query::query(
            Name::from_str("api.example.com.").unwrap(),
            RecordType::A,
        ));
        let upstream = refused.to_vec().unwrap();
        let mut programmer = Recording::new(false);
        let reply = gate_query(
            &query_bytes("api.example.com.", 17),
            |_| true,
            false,
            move |_bytes: Vec<u8>| async move { Ok(upstream) },
            &mut programmer,
        )
        .await
        .unwrap();
        let parsed = Message::from_vec(&reply).unwrap();
        assert_eq!(parsed.metadata.response_code, ResponseCode::ServFail);
        assert_eq!(parsed.metadata.id, 17);
        assert!(programmer.installed.is_empty());
    }

    #[tokio::test]
    async fn a_permitted_answer_is_installed_before_it_is_released() {
        let upstream = answer_bytes(
            "api.example.com.",
            9,
            &[(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10)), 300)],
        );
        let expected = upstream.clone();
        let mut programmer = Recording::new(false);
        let reply = gate_query(
            &query_bytes("api.example.com.", 9),
            |name| name == "api.example.com.",
            false,
            move |_bytes: Vec<u8>| async move { Ok(upstream) },
            &mut programmer,
        )
        .await
        .unwrap();
        assert_eq!(
            reply, expected,
            "the upstream answer must pass through verbatim"
        );
        assert_eq!(
            programmer.installed,
            vec![vec![TimedAddress {
                addr: IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10)),
                ttl_seconds: 300,
            }]]
        );
    }

    #[tokio::test]
    async fn a_failed_installation_withholds_the_answer() {
        // The ordering made falsifiable: if release did not depend on installation,
        // this reply would carry the upstream's addresses. It must carry SERVFAIL.
        let upstream = answer_bytes(
            "api.example.com.",
            11,
            &[(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10)), 300)],
        );
        let mut programmer = Recording::new(true);
        let reply = gate_query(
            &query_bytes("api.example.com.", 11),
            |_| true,
            false,
            move |_bytes: Vec<u8>| async move { Ok(upstream) },
            &mut programmer,
        )
        .await
        .unwrap();
        let parsed = Message::from_vec(&reply).unwrap();
        assert_eq!(parsed.metadata.response_code, ResponseCode::ServFail);
        assert!(parsed.answers.is_empty());
    }

    #[tokio::test]
    async fn an_upstream_nxdomain_passes_through_verbatim() {
        let mut nxdomain = Message::error_msg(13, OpCode::Query, ResponseCode::NXDomain);
        nxdomain.add_query(Query::query(
            Name::from_str("gone.example.com.").unwrap(),
            RecordType::A,
        ));
        let upstream = nxdomain.to_vec().unwrap();
        let expected = upstream.clone();
        let mut programmer = Recording::new(false);
        let reply = gate_query(
            &query_bytes("gone.example.com.", 13),
            |_| true,
            false,
            move |_bytes: Vec<u8>| async move { Ok(upstream) },
            &mut programmer,
        )
        .await
        .unwrap();
        assert_eq!(reply, expected);
        assert!(
            programmer.installed.is_empty(),
            "an errored answer must not program the filter"
        );
    }

    #[tokio::test]
    async fn a_permitted_aaaa_is_withheld_empty_when_the_host_has_no_ipv6_path() {
        // spec/05: where the host has no working IPv6 path, the resolver withholds
        // AAAA rather than releasing addresses the guest cannot reach. Withheld
        // means NoError with no records — never REFUSED, which means policy.
        let forwarded = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&forwarded);
        let mut programmer = Recording::new(false);
        let reply = gate_query(
            &typed_query_bytes("api.example.com.", 23, RecordType::AAAA),
            |_| true,
            true,
            move |_bytes: Vec<u8>| {
                counter.fetch_add(1, Ordering::SeqCst);
                async move { Ok(Vec::new()) }
            },
            &mut programmer,
        )
        .await
        .unwrap();
        let parsed = Message::from_vec(&reply).unwrap();
        assert_eq!(parsed.metadata.response_code, ResponseCode::NoError);
        assert!(parsed.answers.is_empty());
        assert_eq!(forwarded.load(Ordering::SeqCst), 0);
        assert!(programmer.installed.is_empty());
    }

    #[tokio::test]
    async fn a_denied_aaaa_is_refused_even_while_withholding() {
        // Denial outranks family withholding: REFUSED is the policy signal and an
        // empty answer would hide it.
        let mut programmer = Recording::new(false);
        let reply = gate_query(
            &typed_query_bytes("blocked.example.", 24, RecordType::AAAA),
            |_| false,
            true,
            move |_bytes: Vec<u8>| async move { Ok(Vec::new()) },
            &mut programmer,
        )
        .await
        .unwrap();
        let parsed = Message::from_vec(&reply).unwrap();
        assert_eq!(parsed.metadata.response_code, ResponseCode::Refused);
    }

    #[tokio::test]
    async fn an_aaaa_answer_installs_the_v6_family_identically() {
        // IPv6 parity (spec/05): both families are programmed from the same
        // answers, through the same install call.
        let v6: IpAddr = "2001:db8::7".parse().unwrap();
        let upstream = answer_bytes("api.example.com.", 25, &[(v6, 120)]);
        let mut programmer = Recording::new(false);
        gate_query(
            &typed_query_bytes("api.example.com.", 25, RecordType::AAAA),
            |_| true,
            false,
            move |_bytes: Vec<u8>| async move { Ok(upstream) },
            &mut programmer,
        )
        .await
        .unwrap();
        assert_eq!(
            programmer.installed,
            vec![vec![TimedAddress {
                addr: v6,
                ttl_seconds: 120,
            }]]
        );
    }

    /// A programmer the serve-loop tests can inspect from outside the loop.
    #[derive(Clone)]
    struct SharedRecording {
        installed: Arc<std::sync::Mutex<Vec<Vec<TimedAddress>>>>,
    }

    impl FilterProgrammer for SharedRecording {
        async fn install(&mut self, additions: &[TimedAddress]) -> Result<(), FilterInstallError> {
            self.installed.lock().unwrap().push(additions.to_vec());
            Ok(())
        }
    }

    async fn ask(client: &tokio::net::UdpSocket, resolver: SocketAddr, query: &[u8]) -> Message {
        client.send_to(query, resolver).await.unwrap();
        let mut buf = [0u8; 512];
        let len = tokio::time::timeout(Duration::from_secs(5), client.recv(&mut buf))
            .await
            .unwrap()
            .unwrap();
        Message::from_vec(&buf[..len]).unwrap()
    }

    #[tokio::test]
    async fn the_serve_loop_gates_over_udp_end_to_end() {
        let upstream_socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_socket.local_addr().unwrap();
        tokio::spawn(async move {
            let mut buf = [0u8; 512];
            loop {
                let (len, from) = upstream_socket.recv_from(&mut buf).await.unwrap();
                let query = Message::from_vec(&buf[..len]).unwrap();
                let answer = answer_bytes(
                    "api.example.com.",
                    query.metadata.id,
                    &[(IpAddr::V4(Ipv4Addr::new(198, 51, 100, 7)), 60)],
                );
                upstream_socket.send_to(&answer, from).await.unwrap();
            }
        });
        let serve_socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let resolver_addr = serve_socket.local_addr().unwrap();
        let installed = Arc::new(std::sync::Mutex::new(Vec::new()));
        let loop_handle = tokio::spawn(serve(
            serve_socket,
            Allowlist::parse(&["api.example.com".to_owned()]).unwrap(),
            ServeConfig {
                upstream: upstream_addr,
                withhold_aaaa: false,
                upstream_timeout: Duration::from_secs(2),
            },
            SharedRecording {
                installed: Arc::clone(&installed),
            },
        ));
        let client = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();

        let allowed = ask(&client, resolver_addr, &query_bytes("api.example.com.", 31)).await;
        assert_eq!(allowed.metadata.response_code, ResponseCode::NoError);
        assert_eq!(allowed.metadata.id, 31);
        assert_eq!(installed.lock().unwrap().len(), 1);

        let denied = ask(&client, resolver_addr, &query_bytes("denied.example.", 32)).await;
        assert_eq!(denied.metadata.response_code, ResponseCode::Refused);
        assert_eq!(
            installed.lock().unwrap().len(),
            1,
            "a denied name must not program the filter"
        );
        loop_handle.abort();
    }

    #[tokio::test]
    async fn the_serve_loop_answers_servfail_when_upstream_is_unreachable() {
        // A bound-then-dropped socket leaves a loopback port with no listener: the
        // exchange fails fast (ICMP refusal) or times out, and either way the guest
        // must see SERVFAIL rather than silence.
        let dead = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let dead_addr = dead.local_addr().unwrap();
        drop(dead);
        let serve_socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let resolver_addr = serve_socket.local_addr().unwrap();
        let loop_handle = tokio::spawn(serve(
            serve_socket,
            Allowlist::parse(&["api.example.com".to_owned()]).unwrap(),
            ServeConfig {
                upstream: dead_addr,
                withhold_aaaa: false,
                upstream_timeout: Duration::from_millis(400),
            },
            SharedRecording {
                installed: Arc::new(std::sync::Mutex::new(Vec::new())),
            },
        ));
        let client = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let answer = ask(&client, resolver_addr, &query_bytes("api.example.com.", 33)).await;
        assert_eq!(answer.metadata.response_code, ResponseCode::ServFail);
        loop_handle.abort();
    }

    #[tokio::test]
    async fn gating_holds_over_a_real_loopback_socket() {
        // The spike-D shape: the upstream is a real UDP responder, so the forward leg
        // exercises the same transport the wiring slice will use.
        let upstream_socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_socket.local_addr().unwrap();
        tokio::spawn(async move {
            let mut buf = [0u8; 512];
            let (len, from) = upstream_socket.recv_from(&mut buf).await.unwrap();
            let query = Message::from_vec(&buf[..len]).unwrap();
            let answer = answer_bytes(
                "api.example.com.",
                query.metadata.id,
                &[(IpAddr::V4(Ipv4Addr::new(198, 51, 100, 7)), 60)],
            );
            upstream_socket.send_to(&answer, from).await.unwrap();
        });
        let mut programmer = Recording::new(false);
        let reply = gate_query(
            &query_bytes("api.example.com.", 21),
            |name| name == "api.example.com.",
            false,
            move |bytes: Vec<u8>| async move {
                let socket = tokio::net::UdpSocket::bind("127.0.0.1:0")
                    .await
                    .map_err(|error| GateError::Upstream {
                        reason: error.to_string(),
                    })?;
                socket
                    .send_to(&bytes, upstream_addr)
                    .await
                    .map_err(|error| GateError::Upstream {
                        reason: error.to_string(),
                    })?;
                let mut buf = vec![0u8; 512];
                let len = socket
                    .recv(&mut buf)
                    .await
                    .map_err(|error| GateError::Upstream {
                        reason: error.to_string(),
                    })?;
                buf.truncate(len);
                Ok(buf)
            },
            &mut programmer,
        )
        .await
        .unwrap();
        let parsed = Message::from_vec(&reply).unwrap();
        assert_eq!(parsed.metadata.response_code, ResponseCode::NoError);
        assert_eq!(programmer.installed.len(), 1);
        assert_eq!(
            programmer.installed[0][0].addr,
            IpAddr::V4(Ipv4Addr::new(198, 51, 100, 7))
        );
    }
}
