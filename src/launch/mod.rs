//! Closed launch policy and lifetime ownership.

mod command;
mod console;
// The guest control plane, which is both the supervisor's readiness handshake and the host end of
// every `exec` and `shell` session (spec/12).
pub mod control;
mod credentials;
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
    BACKEND, BackendPrograms, BootMetadata, CredentialSpec, DescriptorBudget, GuestSession,
    IdentityTranslation, LAUNCH_SCHEMA_VERSION, LaunchSpec, ResourceSpec, RuntimePaths, ShareSpec,
    SocketLegs, VIRTIOFSD_RLIMIT_NOFILE, VolumeSpec, WORKSPACE_SHARE_TAG, unmirrorable,
};
pub use supervisor::{
    ChildExit, ChildKind, GuestReadiness, LaunchReady, ShutdownReason, Supervisor,
};
pub use systemd::TransientUnitSpec;
