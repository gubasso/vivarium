//! Per-VM networking: the egress allowlist grammar, the nftables ruleset and its
//! application, the namespace pair's argv and lifecycle, the tap one-shots, and the
//! gating resolver with its serve loop.
//!
//! These are the four seams Q-005 selected; the slice 004 `Revisions` line carries the
//! selection evidence. `launch` composes them into processes; `config` calls the
//! allowlist grammar at validation time. What spawns here is bounded: `nft` applies
//! and the resolver's sockets — namespace creation and the uplink stay with the
//! supervisor, which owns child lifetimes.

pub mod allowlist;
pub mod netns;
pub mod nft;
pub mod resolver;
pub mod tap;
