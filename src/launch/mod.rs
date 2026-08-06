//! Closed launch policy and lifetime ownership.

mod command;
mod console;
mod error;
mod policy;
mod readiness;
pub mod secure_fs;
mod spec;
mod supervisor;
mod systemd;

pub use command::CommandSpec;
pub use console::{ConsoleReader, ConsoleSink};
pub use error::LaunchError;
pub use policy::ConfinementProfile;
pub use readiness::{ReadinessError, ReadinessReport, ReadinessStatus};
pub use spec::{
    BackendPrograms, DescriptorBudget, IdentityTranslation, LaunchSpec, ResourceSpec, RuntimePaths,
    ShareSpec, SocketLegs, VIRTIOFSD_RLIMIT_NOFILE, VolumeSpec,
};
pub use supervisor::{
    ChildExit, ChildKind, GuestReadiness, LaunchReady, ShutdownReason, Supervisor,
};
pub use systemd::TransientUnitSpec;
