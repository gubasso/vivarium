//! Configuration resolution, secure launch construction, and supervision for vivarium's
//! Nix-built VM.

#[cfg(feature = "host")]
pub mod config;

#[cfg(feature = "host")]
pub mod diagnostic;
#[cfg(feature = "host")]
pub mod doctor;
#[cfg(feature = "host")]
pub mod exit;
#[cfg(feature = "host")]
pub mod launch;
pub mod protocol;
