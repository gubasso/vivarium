//! Host proof for the four networking seams against the real tools.
//!
//! The unit tests pin renderings; these trials put the renderings in front of the
//! real `unshare`, `nsenter`, `ip`, and `nft` inside an unprivileged namespace pair,
//! which is what the Q-005 spikes did by hand. The gate is evaluated at run time and
//! reports why it did not run, because a lane that skips silently reads as a lane
//! that passed. No trial here boots a guest or writes gigabytes: the pair, the tap,
//! and the ruleset live in the kernel and vanish with the holder.
#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::unwrap_used
)]

use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use libtest_mimic::{Arguments, Failed, Trial};

#[path = "support/harness.rs"]
mod harness;
use tokio::process::{Child, Command};
use vivarium::net::allowlist::{AllowEntry, Allowlist};
use vivarium::net::resolver::{ServeConfig, serve};
use vivarium::net::{netns, nft, tap};

/// The tap name and gateway the Q-005 spike used; the wired launch carries its own
/// through the launch schema, so these are trial-local, not product constants.
const TAP: &str = "viv-tap0";
const GATEWAY_CIDR: &str = "10.177.0.1/24";

fn main() -> std::process::ExitCode {
    let args = Arguments::from_args();
    let decision = gate();
    let require = harness::gate_required();
    let ignored = decision.is_err() && !require;
    if ignored && let Err(reason) = &decision {
        eprintln!("gated: net_host trials — {reason}");
    }
    let trials = vec![
        Trial::test("net_host_self_check", self_check),
        gated_trial("netns_pair_is_distinct_and_joinable", ignored, |tools| {
            Box::pin(pair_is_distinct_and_joinable(tools))
        }),
        gated_trial("tap_configures_without_carrier", ignored, |tools| {
            Box::pin(tap_configures_without_carrier(tools))
        }),
        gated_trial("ruleset_applies_and_elements_expire", ignored, |tools| {
            Box::pin(ruleset_applies_and_elements_expire(tools))
        }),
        gated_trial("resolver_installs_through_real_nft", ignored, |tools| {
            Box::pin(resolver_installs_through_real_nft(tools))
        }),
    ];
    libtest_mimic::run(&args, trials).exit_code()
}

type TrialFuture = std::pin::Pin<Box<dyn Future<Output = ()>>>;

fn gated_trial(name: &'static str, ignored: bool, body: fn(Tools) -> TrialFuture) -> Trial {
    Trial::test(name, move || {
        let tools = gate().map_err(Failed::from)?;
        tokio::runtime::Runtime::new()
            .map_err(|error| Failed::from(error.to_string()))?
            .block_on(body(tools));
        Ok(())
    })
    .with_ignored_flag(ignored)
}

/// The pinned tools every trial needs, resolved from `PATH` — the dev shell carries
/// them from the same nixpkgs the product pins.
#[derive(Clone)]
struct Tools {
    unshare: PathBuf,
    nsenter: PathBuf,
    ip: PathBuf,
    nft: PathBuf,
    sleep: PathBuf,
}

/// Whether this host can run the trials at all: the tools present, and an
/// unprivileged user+net pair actually creatable — which a container or a
/// `kernel.unprivileged_userns_clone=0` host refuses.
fn gate() -> Result<Tools, String> {
    let tools = Tools {
        unshare: harness::tool_on_path("unshare")?,
        nsenter: harness::tool_on_path("nsenter")?,
        ip: harness::tool_on_path("ip")?,
        nft: harness::tool_on_path("nft")?,
        sleep: harness::tool_on_path("sleep")?,
    };
    let probe = std::process::Command::new(&tools.unshare)
        .args(netns::create_pair_args(&netns::holder_program("true")))
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .output()
        .map_err(|error| format!("probing `unshare` failed: {error}"))?;
    if !probe.status.success() {
        return Err(format!(
            "unprivileged user+net namespaces are unavailable: {}",
            String::from_utf8_lossy(&probe.stderr).trim()
        ));
    }
    Ok(tools)
}

/// Assertions that need no namespaces, so the lane is never entirely ignored.
///
/// nextest exits 4 when every trial matching a filter is ignored, and a binary
/// whose every trial is gated reports a failure that means nothing on an
/// ordinary developer host.
fn self_check() -> Result<(), Failed> {
    // An unmet gate must carry a reason a reader can act on — the difference
    // between a skip that informs and one that hides.
    if let Err(reason) = gate()
        && reason.is_empty()
    {
        return Err(Failed::from("the gate gave no reason for not running"));
    }
    // The rendering the gated trials feed the real `nft` is constructible from
    // this vantage too; the exact shape is the unit lane's golden.
    let rendered = serde_json::to_value(nft::base_ruleset())
        .map_err(|error| Failed::from(error.to_string()))?;
    if rendered["nftables"][0]["add"]["table"]["name"] != serde_json::json!("vivarium") {
        return Err(Failed::from(
            "base_ruleset does not open the vivarium table",
        ));
    }
    Ok(())
}

/// A running holder whose pair the trial works inside; killed on drop.
struct Holder {
    child: Child,
    pid: u32,
}

async fn spawn_holder(tools: &Tools) -> Holder {
    let holder = netns::holder_program(&tools.sleep.to_string_lossy());
    let child = Command::new(&tools.unshare)
        .args(netns::create_pair_args(&holder))
        .kill_on_drop(true)
        .spawn()
        .expect("spawning the namespace holder");
    let pid = child.id().expect("the holder has a pid");
    netns::await_pair(pid, Duration::from_secs(5))
        .await
        .expect("the pair becomes distinct");
    Holder { child, pid }
}

impl Holder {
    /// Run a program inside the pair and return its output.
    async fn run(&self, tools: &Tools, program: &Path, args: &[&str]) -> std::process::Output {
        let mut full: Vec<String> = vec![program.to_string_lossy().into_owned()];
        full.extend(args.iter().map(ToString::to_string));
        Command::new(&tools.nsenter)
            .args(netns::enter_pair_args(self.pid, &full))
            .output()
            .await
            .expect("running inside the pair")
    }

    async fn shutdown(mut self) {
        let _ = self.child.kill().await;
    }
}

fn stdout_of(output: &std::process::Output) -> String {
    assert!(
        output.status.success(),
        "in-namespace command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

async fn pair_is_distinct_and_joinable(tools: Tools) {
    let holder = spawn_holder(&tools).await;
    let links = stdout_of(&holder.run(&tools, &tools.ip, &["-j", "link", "show"]).await);
    let parsed: serde_json::Value = serde_json::from_str(&links).expect("`ip -j` emits JSON");
    let names: Vec<&str> = parsed
        .as_array()
        .expect("a link array")
        .iter()
        .filter_map(|link| link.get("ifname").and_then(serde_json::Value::as_str))
        .collect();
    assert_eq!(names, vec!["lo"], "a fresh pair holds lo and nothing else");
    holder.shutdown().await;
}

async fn tap_configures_without_carrier(tools: Tools) {
    let holder = spawn_holder(&tools).await;
    for sequence in tap::setup_sequences(TAP, GATEWAY_CIDR) {
        let args: Vec<&str> = sequence.iter().map(String::as_str).collect();
        stdout_of(&holder.run(&tools, &tools.ip, &args).await);
    }
    let links = stdout_of(&holder.run(&tools, &tools.ip, &["-j", "link", "show"]).await);
    let state = tap::assert_tap_up(&links, TAP).expect("the tap is configured and up");
    assert!(
        !state.carrier,
        "no VMM has opened the tap, so no carrier is the expected state"
    );
    holder.shutdown().await;
}

async fn ruleset_applies_and_elements_expire(tools: Tools) {
    let holder = spawn_holder(&tools).await;
    let runner = nft::NftRunner::entered(&tools.nsenter, holder.pid, &tools.nft.to_string_lossy());
    runner
        .apply(&nft::base_ruleset())
        .await
        .expect("the base ruleset applies through real nft");

    let literals = [
        AllowEntry::parse("192.0.2.10").unwrap(),
        AllowEntry::parse("198.51.100.0/24").unwrap(),
        AllowEntry::parse("2001:db8::/32").unwrap(),
    ];
    let refs: Vec<&AllowEntry> = literals.iter().collect();
    runner
        .apply(&nft::literal_elements(&refs))
        .await
        .expect("literal destinations apply into the interval sets");

    let timed: IpAddr = "203.0.113.9".parse().unwrap();
    runner
        .apply(&nft::timed_element(timed, 2))
        .await
        .expect("a timed element applies");

    let list_args = ["-j", "list", "table", "inet", nft::TABLE];
    let listed = stdout_of(&holder.run(&tools, &tools.nft, &list_args).await);
    assert!(
        listed.contains("203.0.113.9"),
        "the timed element is present right after its add"
    );
    assert!(
        listed.contains("198.51.100.0"),
        "the CIDR literal is present"
    );

    tokio::time::sleep(Duration::from_millis(3500)).await;
    let listed = stdout_of(&holder.run(&tools, &tools.nft, &list_args).await);
    assert!(
        !listed.contains("203.0.113.9"),
        "the timed element expired with its TTL"
    );
    assert!(
        listed.contains("198.51.100.0"),
        "literal destinations do not expire"
    );
    holder.shutdown().await;
}

async fn resolver_installs_through_real_nft(tools: Tools) {
    use hickory_proto::op::{Message, MessageType, OpCode, Query, ResponseCode};
    use hickory_proto::rr::rdata::A;
    use hickory_proto::rr::{Name, RData, Record, RecordType};
    use std::str::FromStr as _;

    let holder = spawn_holder(&tools).await;
    let runner = nft::NftRunner::entered(&tools.nsenter, holder.pid, &tools.nft.to_string_lossy());
    runner
        .apply(&nft::base_ruleset())
        .await
        .expect("the base ruleset applies");

    // A loopback stand-in for the host's configured resolver.
    let upstream_socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let upstream = upstream_socket.local_addr().unwrap();
    tokio::spawn(async move {
        let mut buf = [0u8; 512];
        loop {
            let (len, from) = upstream_socket.recv_from(&mut buf).await.unwrap();
            let query = Message::from_vec(&buf[..len]).unwrap();
            let name = Name::from_str("api.example.com.").unwrap();
            let mut answer = Message::response(query.metadata.id, OpCode::Query);
            answer.add_query(Query::query(name.clone(), RecordType::A));
            answer.add_answer(Record::from_rdata(
                name,
                90,
                RData::A(A("198.51.100.7".parse().unwrap())),
            ));
            upstream_socket
                .send_to(&answer.to_vec().unwrap(), from)
                .await
                .unwrap();
        }
    });

    // The serve loop runs host-side here; what is real is the filter programmer,
    // which crosses into the pair through `nsenter` exactly as the wired resolver's
    // rules do. Release must not happen before that subprocess exits zero.
    let serve_socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let resolver_addr = serve_socket.local_addr().unwrap();
    let loop_handle = tokio::spawn(serve(
        serve_socket,
        Allowlist::parse(&["api.example.com".to_owned()]).unwrap(),
        ServeConfig {
            upstream,
            withhold_aaaa: false,
            upstream_timeout: Duration::from_secs(2),
        },
        nft::NftProgrammer::new(runner),
    ));

    let client = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let mut query = Message::new(41, MessageType::Query, OpCode::Query);
    query.add_query(Query::query(
        Name::from_str("api.example.com.").unwrap(),
        RecordType::A,
    ));
    client
        .send_to(&query.to_vec().unwrap(), resolver_addr)
        .await
        .unwrap();
    let mut buf = [0u8; 512];
    let len = tokio::time::timeout(Duration::from_secs(5), client.recv(&mut buf))
        .await
        .expect("the gated answer arrives")
        .unwrap();
    let answer = Message::from_vec(&buf[..len]).unwrap();
    assert_eq!(answer.metadata.response_code, ResponseCode::NoError);

    // The answer was released, so the address must already be enforceable.
    let list_args = ["-j", "list", "table", "inet", nft::TABLE];
    let listed = stdout_of(&holder.run(&tools, &tools.nft, &list_args).await);
    assert!(
        listed.contains("198.51.100.7"),
        "the released answer's address is in the kernel's allowed set"
    );
    loop_handle.abort();
    holder.shutdown().await;
}
