mod boot;
mod control;
mod credentials;
mod process;
mod session;

use std::collections::HashSet;
use std::path::PathBuf;
use std::str::FromStr;
use tokio_util::sync::CancellationToken;
use tokio_vsock::{VMADDR_CID_ANY, VsockAddr, VsockListener};
use vivarium::protocol::{CONTROL_PORT, CREDENTIAL_PORT, CredentialId};

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("vivarium guest agent: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let credentials = parse_credentials(std::env::args().skip(1))?;
    let cmdline = tokio::fs::read_to_string("/proc/cmdline").await?;
    let boot_identity = boot::parse_boot_identity(&cmdline)?;
    let control_listener = VsockListener::bind(VsockAddr::new(VMADDR_CID_ANY, CONTROL_PORT))?;
    let credential_listener = VsockListener::bind(VsockAddr::new(VMADDR_CID_ANY, CREDENTIAL_PORT))?;
    let cancellation = CancellationToken::new();
    let control = control::accept_loop(
        control_listener,
        boot_identity,
        credentials.iter().copied().collect(),
        triggers(),
        cancellation.clone(),
    );
    let relay = credentials::run(credential_listener, credentials, cancellation.clone());
    tokio::select! {
        result = control => result?,
        result = relay => result?,
        () = std::future::pending::<()>() => unreachable!(),
    }
    cancellation.cancel();
    Ok(())
}

/// Where the root-owned units' trigger files land, one directory for all of them.
///
/// The agent's unit declares `RuntimeDirectory=vivarium`, so systemd names the directory in
/// `RUNTIME_DIRECTORY`; the fallback spells the same path for a hand-run agent. The file names
/// are each half of a contract with `nix/guest.nix`, which points the poweroff and fstrim path
/// units at the same spellings.
fn triggers() -> control::Triggers {
    let directory = std::env::var_os("RUNTIME_DIRECTORY")
        .map_or_else(|| PathBuf::from("/run/vivarium"), PathBuf::from);
    control::Triggers {
        poweroff: directory.join("poweroff-requested"),
        fstrim_request: directory.join("fstrim-requested"),
        fstrim_done: directory.join("fstrim-done"),
        trim_serial: std::sync::Arc::new(tokio::sync::Mutex::new(())),
    }
}

fn parse_credentials(
    mut arguments: impl Iterator<Item = String>,
) -> Result<HashSet<CredentialId>, &'static str> {
    let mut result = HashSet::new();
    while let Some(argument) = arguments.next() {
        if argument != "--credential" {
            return Err("unsupported argument");
        }
        let id = CredentialId::from_str(&arguments.next().ok_or("missing credential id")?)
            .map_err(|_| "invalid credential id")?;
        if !result.insert(id) {
            return Err("duplicate credential id");
        }
    }
    Ok(result)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn command_line_is_closed() {
        assert_eq!(
            parse_credentials(
                ["--credential", "ssh", "--credential", "gpg"]
                    .into_iter()
                    .map(str::to_owned)
            )
            .unwrap()
            .len(),
            2
        );
        assert!(parse_credentials(["--listen", "1"].into_iter().map(str::to_owned)).is_err());
        assert!(
            parse_credentials(
                ["--credential", "ssh", "--credential", "ssh"]
                    .into_iter()
                    .map(str::to_owned)
            )
            .is_err()
        );
    }
}
