//! The two-legged egress fixture spec/05 requires and Q-022 specified.
//!
//! Everything lives inside the VM's own network namespace, reached through the
//! VMM's pid, so the trial needs no external network and no public delegation:
//! a stub upstream DNS bound on the uplink's DNS-forward address (added to the
//! pair's loopback, so the gating resolver's upstream exchanges land here instead
//! of the real uplink), and an HTTP endpoint in a nested network namespace behind
//! a veth — nested rather than on the pair's loopback, because a destination
//! local to the pair would be reached over the input path and the allowed leg
//! would never traverse the forward chain the allowlist filters. Two `.test`
//! names resolve to two different addresses; only the first is allowed.
//!
//! The stub processes are this test binary re-executed in fixture modes, joined
//! into the namespaces with the same pinned `nsenter` shape the supervisor uses.

#![allow(clippy::unwrap_used)]

use std::io::{Read as _, Write as _};
use std::net::{TcpListener, UdpSocket};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use super::harness::wait_until;

/// The name the allowlist admits; the stub upstream answers it with [`ALLOWED_ADDR`].
pub const ALLOWED_NAME: &str = "allowed.test";
/// Allowlisted, but the stub upstream answers `NXDOMAIN`: the leg that proves the
/// resolver passes upstream failures through as themselves.
pub const GHOST_NAME: &str = "ghost.test";
/// In no allowlist entry; its denial must be `REFUSED`, never a resolution error.
pub const DENIED_NAME: &str = "denied.test";
/// The endpoint address the allowed name resolves to.
pub const ALLOWED_ADDR: &str = "10.199.0.1";
/// The second endpoint address: reachable in the fixture, admitted by nothing.
pub const DENIED_ADDR: &str = "10.199.0.2";
/// The body the allowed leg must read back, byte-for-byte.
pub const ALLOWED_BODY: &str = "vivarium-egress-allowed";

/// The pair side of the veth toward the nested endpoint namespace.
const VETH_GATEWAY: &str = "10.199.0.254";
/// The uplink's DNS-forward address, from `nix/default.nix`'s network layout; the
/// fixture adds it to the pair's loopback to interpose the resolver's upstream.
const UPSTREAM_ADDR: &str = "10.177.53.53";
/// The gating resolver's socket, from the same layout.
const RESOLVER_ADDR: &str = "10.177.0.1:53";
/// The answering record's TTL: long enough that the trial's connect cannot race
/// the element's expiry.
const STUB_TTL: u32 = 120;

/// Dispatch for the fixture modes this binary re-executes itself in.
///
/// Returns `None` when the first argument is not a fixture mode, so the caller
/// falls through to the ordinary trial harness.
#[must_use]
pub fn run_mode() -> Option<std::process::ExitCode> {
    let mut args = std::env::args().skip(1);
    let mode = args.next()?;
    let code = match mode.as_str() {
        "egress-fixture-dns" => serve_stub_dns(),
        "egress-fixture-endpoint" => serve_endpoints(),
        "egress-fixture-probe" => probe_mode(&args.next()?, &args.next()?),
        "egress-fixture-claim" => claim_resolver_socket(),
        _ => return None,
    };
    Some(code)
}

/// The stub upstream: authoritative for exactly two names, `NXDOMAIN` for the rest.
fn serve_stub_dns() -> std::process::ExitCode {
    use hickory_proto::op::{Message, OpCode, ResponseCode};
    use hickory_proto::rr::rdata::A;
    use hickory_proto::rr::{RData, Record, RecordType};
    let Ok(socket) = UdpSocket::bind((UPSTREAM_ADDR, 53)) else {
        eprintln!("egress-fixture-dns: cannot bind {UPSTREAM_ADDR}:53");
        return std::process::ExitCode::FAILURE;
    };
    let mut buf = [0u8; 4096];
    loop {
        let Ok((len, from)) = socket.recv_from(&mut buf) else {
            return std::process::ExitCode::FAILURE;
        };
        let Ok(query) = Message::from_vec(&buf[..len]) else {
            continue;
        };
        let Some(question) = query.queries.first().cloned() else {
            continue;
        };
        let name = question.name().to_ascii().to_lowercase();
        let mut response = Message::response(query.metadata.id, OpCode::Query);
        response.add_query(question.clone());
        if name.trim_end_matches('.') == ALLOWED_NAME {
            if question.query_type() == RecordType::A {
                response.add_answer(Record::from_rdata(
                    question.name().clone(),
                    STUB_TTL,
                    RData::A(A(ALLOWED_ADDR.parse().unwrap())),
                ));
            }
            // A non-A question gets NoError with no records (NODATA).
        } else {
            response.metadata.response_code = ResponseCode::NXDomain;
        }
        if let Ok(bytes) = response.to_vec() {
            let _ = socket.send_to(&bytes, from);
        }
    }
}

/// The endpoint: one minimal HTTP responder per address, in the nested namespace.
///
/// Binding retries, because the interfaces this binds on are configured by the
/// harness after this process is already inside its fresh namespace.
fn serve_endpoints() -> std::process::ExitCode {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut listeners = Vec::new();
    for addr in [ALLOWED_ADDR, DENIED_ADDR] {
        let listener = loop {
            match TcpListener::bind((addr, 80)) {
                Ok(listener) => break listener,
                Err(_) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(error) => {
                    eprintln!("egress-fixture-endpoint: cannot bind {addr}:80: {error}");
                    return std::process::ExitCode::FAILURE;
                }
            }
        };
        listeners.push(listener);
    }
    let mut threads = Vec::new();
    for listener in listeners {
        threads.push(std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut discard = [0u8; 1024];
                let _ = stream.read(&mut discard);
                let body = format!("{ALLOWED_BODY}\n");
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        }));
    }
    for thread in threads {
        let _ = thread.join();
    }
    std::process::ExitCode::FAILURE
}

/// One A query against a nameserver, printing the response code by name.
///
/// Run inside the pair against the gating resolver, this is what distinguishes a
/// policy `REFUSED` from an upstream `NXDOMAIN` — the distinction Q-022's exit
/// requires and no guest exit code carries.
fn probe_mode(server: &str, name: &str) -> std::process::ExitCode {
    use hickory_proto::op::{Message, MessageType, OpCode, Query};
    use hickory_proto::rr::{Name, RecordType};
    use std::str::FromStr as _;
    let Ok(target) = Name::from_str(name) else {
        return std::process::ExitCode::FAILURE;
    };
    let mut query = Message::new(4242, MessageType::Query, OpCode::Query);
    query.add_query(Query::query(target, RecordType::A));
    let Ok(socket) = UdpSocket::bind("0.0.0.0:0") else {
        return std::process::ExitCode::FAILURE;
    };
    let _ = socket.set_read_timeout(Some(Duration::from_secs(5)));
    let Ok(bytes) = query.to_vec() else {
        return std::process::ExitCode::FAILURE;
    };
    if socket.send_to(&bytes, server).is_err() {
        eprintln!("egress-fixture-probe: send to {server} failed");
        return std::process::ExitCode::FAILURE;
    }
    let mut buf = [0u8; 4096];
    let Ok(len) = socket.recv(&mut buf) else {
        eprintln!("egress-fixture-probe: no answer from {server}");
        return std::process::ExitCode::FAILURE;
    };
    let Ok(answer) = Message::from_vec(&buf[..len]) else {
        return std::process::ExitCode::FAILURE;
    };
    println!("rcode={:?}", answer.metadata.response_code);
    std::process::ExitCode::SUCCESS
}

/// Try to bind the gating resolver's own socket; a clean exit means nothing holds it.
///
/// Run inside the pair, this is open mode's absence probe: a resolver that was
/// wrongly spawned would hold the address and the bind would fail `EADDRINUSE`, so
/// success is positive proof of an empty socket rather than a timeout read as one.
fn claim_resolver_socket() -> std::process::ExitCode {
    match UdpSocket::bind(RESOLVER_ADDR) {
        Ok(_) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("egress-fixture-claim: cannot bind {RESOLVER_ADDR}: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

/// Assert open mode ships no filter and no resolver — absent, not inert (spec/05).
///
/// Two observations inside the pair, each made where the artifact would exist: the
/// kernel lists no vivarium table, and the resolver's socket is claimable. A
/// supervisor that grew an unconditional ruleset apply or resolver spawn fails
/// here instead of passing on a neutralized knob.
///
/// # Errors
///
/// Returns the observation that contradicted absence.
pub fn expect_open_mode_absence(vm_pid: u32) -> Result<(), String> {
    let tables = enter_pair(vm_pid, &[tool("nft")?, "list".into(), "tables".into()])?
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("listing tables in the pair: {error}"))?;
    if !tables.status.success() {
        return Err(format!(
            "`nft list tables` in the pair failed: {}",
            String::from_utf8_lossy(&tables.stderr).trim()
        ));
    }
    if String::from_utf8_lossy(&tables.stdout).contains(vivarium::net::nft::TABLE) {
        return Err(format!(
            "open mode must install no ruleset, but the kernel lists table `{}`",
            vivarium::net::nft::TABLE
        ));
    }
    let exe = std::env::current_exe().map_err(|error| format!("current_exe: {error}"))?;
    run_in_pair(
        vm_pid,
        &[exe.display().to_string(), "egress-fixture-claim".into()],
    )
    .map_err(|error| format!("open mode must spawn no resolver, but its socket is held: {error}"))
}

/// The installed fixture; dropping it kills the stub processes. The veth and the
/// loopback address die with the pair itself when the VM stops.
pub struct EgressFixture {
    vm_pid: u32,
    children: Vec<Child>,
}

impl EgressFixture {
    /// Install the fixture into the pair the VMM with `vm_pid` runs inside.
    ///
    /// # Errors
    ///
    /// Returns a description of the step that failed; every step names itself.
    #[allow(clippy::too_many_lines)] // argv tables, not logic
    pub fn install(vm_pid: u32) -> Result<Self, String> {
        let exe = std::env::current_exe().map_err(|error| format!("current_exe: {error}"))?;
        let mut fixture = Self {
            vm_pid,
            children: Vec::new(),
        };
        // Interpose the resolver's upstream: the DNS-forward address becomes local
        // to the pair, so the resolver's exchanges land on the stub instead of the
        // uplink.
        run_in_pair(
            vm_pid,
            &[
                ip()?,
                "addr".into(),
                "add".into(),
                format!("{UPSTREAM_ADDR}/32"),
                "dev".into(),
                "lo".into(),
            ],
        )?;
        fixture.spawn_in_pair(&[exe.display().to_string(), "egress-fixture-dns".into()])?;
        wait_until(
            || probe(vm_pid, &format!("{UPSTREAM_ADDR}:53"), ALLOWED_NAME).is_ok(),
            "the stub upstream did not start answering",
        )?;

        // The endpoint namespace: this binary under a fresh `--net`, then a veth
        // from the pair into it. `unshare` execs in place, so the child's pid is
        // the nested namespace's holder.
        let endpoint_pid = fixture.spawn_in_pair(&[
            tool("unshare")?,
            "--net".into(),
            exe.display().to_string(),
            "egress-fixture-endpoint".into(),
        ])?;
        // Distinct from the pair AND from this process's own namespace, because
        // the endpoint crosses three: it starts in the host's (already distinct
        // from the pair), joins the pair's, then unshares its own. Waiting on
        // "differs from the pair" alone is satisfied at spawn, and the veth peer
        // would land wherever the process happened to be at that moment.
        wait_until(
            || nested_namespace_ready(vm_pid, endpoint_pid),
            "the endpoint namespace did not become distinct",
        )?;
        run_in_pair(
            vm_pid,
            &[
                ip()?,
                "link".into(),
                "add".into(),
                "v-fix0".into(),
                "type".into(),
                "veth".into(),
                "peer".into(),
                "name".into(),
                "v-fix1".into(),
                "netns".into(),
                endpoint_pid.to_string(),
            ],
        )?;
        run_in_pair(
            vm_pid,
            &[
                ip()?,
                "addr".into(),
                "add".into(),
                format!("{VETH_GATEWAY}/24"),
                "dev".into(),
                "v-fix0".into(),
            ],
        )?;
        run_in_pair(
            vm_pid,
            &[
                ip()?,
                "link".into(),
                "set".into(),
                "v-fix0".into(),
                "up".into(),
            ],
        )?;
        for args in [
            vec![ip()?, "link".into(), "set".into(), "lo".into(), "up".into()],
            vec![
                ip()?,
                "addr".into(),
                "add".into(),
                format!("{ALLOWED_ADDR}/24"),
                "dev".into(),
                "v-fix1".into(),
            ],
            vec![
                ip()?,
                "addr".into(),
                "add".into(),
                format!("{DENIED_ADDR}/32"),
                "dev".into(),
                "v-fix1".into(),
            ],
            vec![
                ip()?,
                "link".into(),
                "set".into(),
                "v-fix1".into(),
                "up".into(),
            ],
            vec![
                ip()?,
                "route".into(),
                "add".into(),
                "default".into(),
                "via".into(),
                VETH_GATEWAY.into(),
            ],
        ] {
            run_in_nested(vm_pid, endpoint_pid, &args)?;
        }
        Ok(fixture)
    }

    /// The response code the gating resolver answers `name` with, as `Debug` text
    /// (`Refused`, `NXDomain`, `NoError`), observed on the same socket the guest
    /// queries.
    ///
    /// # Errors
    ///
    /// Returns the probe's own failure text when no answer arrives.
    pub fn resolver_rcode(&self, name: &str) -> Result<String, String> {
        probe(self.vm_pid, RESOLVER_ADDR, name)
    }

    fn spawn_in_pair(&mut self, program: &[String]) -> Result<u32, String> {
        let child = enter_pair(self.vm_pid, program)?
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("spawning {program:?} in the pair: {error}"))?;
        let pid = child.id();
        self.children.push(child);
        Ok(pid)
    }
}

impl Drop for EgressFixture {
    fn drop(&mut self) {
        for child in &mut self.children {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// The VMM's pid, read from the launch's own runtime record — the one process the
/// trial knows to be inside the pair.
///
/// # Errors
///
/// Returns a description when no `vm.pid` exists under the runtime root yet.
pub fn read_vm_pid(runtime_root: &Path) -> Result<u32, String> {
    let base = runtime_root.join("vivarium");
    let projects =
        std::fs::read_dir(&base).map_err(|error| format!("reading {}: {error}", base.display()))?;
    for project in projects.flatten() {
        let Ok(targets) = std::fs::read_dir(project.path()) else {
            continue;
        };
        for target in targets.flatten() {
            let candidate = target.path().join("vm.pid");
            if let Ok(text) = std::fs::read_to_string(&candidate)
                && let Ok(pid) = text.trim().parse::<u32>()
            {
                return Ok(pid);
            }
        }
    }
    Err(format!("no vm.pid under {}", base.display()))
}

fn probe(vm_pid: u32, server: &str, name: &str) -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|error| format!("current_exe: {error}"))?;
    let output = enter_pair(
        vm_pid,
        &[
            exe.display().to_string(),
            "egress-fixture-probe".into(),
            server.into(),
            format!("{name}."),
        ],
    )?
    .stdin(Stdio::null())
    .output()
    .map_err(|error| format!("running the probe: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "probe of {name} against {server} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .strip_prefix("rcode=")
        .map(ToString::to_string)
        .ok_or_else(|| "probe printed no rcode".to_owned())
}

/// `nsenter` joining the pair by the VMM's pid — the argv the supervisor
/// renders, taken from the product (`netns::enter_pair_args`) so a flag change
/// there is what this fixture verifies rather than a copy that drifts. Only the
/// binary is resolved from `PATH`, because the trial runs in the dev shell
/// rather than from a launch specification.
fn enter_pair(vm_pid: u32, program: &[String]) -> Result<Command, String> {
    let mut command = Command::new(super::harness::tool_on_path("nsenter")?);
    command.args(vivarium::net::netns::enter_pair_args(vm_pid, program));
    Ok(command)
}

fn run_in_pair(vm_pid: u32, program: &[String]) -> Result<(), String> {
    run_ok(enter_pair(vm_pid, program)?, program)
}

/// Run inside the nested endpoint namespace: the pair's user namespace (which owns
/// the nested net), joined by path, plus the endpoint's own net.
fn run_in_nested(vm_pid: u32, endpoint_pid: u32, program: &[String]) -> Result<(), String> {
    let mut command = Command::new(tool("nsenter")?);
    command.args([
        "--preserve-credentials".to_owned(),
        format!("--user=/proc/{vm_pid}/ns/user"),
        format!("--net=/proc/{endpoint_pid}/ns/net"),
    ]);
    command.args(program);
    run_ok(command, program)
}

fn run_ok(mut command: Command, program: &[String]) -> Result<(), String> {
    let output = command
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("running {program:?}: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{program:?} exited {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn nested_namespace_ready(vm_pid: u32, endpoint_pid: u32) -> bool {
    let read = |path: String| std::fs::read_link(path);
    let (Ok(pair), Ok(own), Ok(endpoint)) = (
        read(format!("/proc/{vm_pid}/ns/net")),
        read("/proc/self/ns/net".to_owned()),
        read(format!("/proc/{endpoint_pid}/ns/net")),
    ) else {
        return false;
    };
    endpoint != pair && endpoint != own
}

fn ip() -> Result<String, String> {
    tool("ip")
}

/// `harness::tool_on_path`, as the `String` this fixture's argv vectors carry.
fn tool(name: &str) -> Result<String, String> {
    super::harness::tool_on_path(name).map(|found| found.display().to_string())
}
