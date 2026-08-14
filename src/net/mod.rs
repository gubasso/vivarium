//! Per-VM networking: the egress allowlist grammar, the nftables ruleset model, the
//! namespace wrapper argv, and the gating resolver core.
//!
//! These are the four seams Q-005 selected; the slice 004 `Revisions` line carries the
//! selection evidence. `launch` composes them into processes; `config` calls the
//! allowlist grammar at validation time. Nothing here spawns or binds anything yet —
//! behavior that needs a namespace or a socket enters with the wiring slice.

pub mod allowlist;
pub mod netns;
pub mod nft;
pub mod resolver;
