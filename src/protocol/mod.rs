//! Shared host/guest control protocol and Cloud Hypervisor host transport.

pub mod frame;
pub mod hybrid;
pub mod message;

pub use frame::{
    FrameError, read_agent_frame, read_client_frame, write_agent_frame, write_client_frame,
};
pub use message::*;
