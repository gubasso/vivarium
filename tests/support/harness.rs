//! Cross-binary primitives: `PATH` resolution, the deadline poll, and shell-safe
//! byte transport.
//!
//! Reached as `support::harness` from every lane binary. What a lane needs of its
//! host is `support::preflight`'s, not this module's: nothing here decides whether
//! anything runs.

use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Resolve a pinned tool from `PATH` — the dev shell carries the tools the
/// product pins — or say which one is missing.
pub fn tool_on_path(name: &str) -> Result<PathBuf, String> {
    let path = std::env::var_os("PATH").ok_or_else(|| "PATH is unset".to_owned())?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| format!("`{name}` is not on PATH; the trial needs the dev shell"))
}

/// Poll `condition` every 50ms for up to ten seconds, answering `what` on expiry.
pub fn wait_until(mut condition: impl FnMut() -> bool, what: &str) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if condition() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(what.to_owned())
}

/// Encode bytes for transport through a guest shell argument.
///
/// The guest has `base64`, so a frame is built here and decoded there rather than
/// escaping arbitrary bytes through a shell command line.
pub fn base64(input: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}
