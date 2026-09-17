//! Development package assembly and verification; no engine execution.
pub mod build;
mod json;
mod keys;
mod types;
mod verify;

pub use build::{BuildOptions, build};
pub use json::decode_strict_json;
pub use keys::{load_private_key, load_public_key};
pub use types::*;
pub(crate) use verify::{PayloadSink, verify_to_sink};
pub use verify::{VerifyOptions, verify};

#[cfg(test)]
mod verify_tests;
