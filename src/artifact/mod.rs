//! Development package content verification; no execution or extraction.
pub mod build;
mod json;
mod keys;
mod types;
mod verify;

pub use build::{BuildOptions, build};
pub use json::decode_strict_json;
pub use keys::{load_private_key, load_public_key};
pub use types::*;
pub use verify::{VerifyOptions, verify};

#[cfg(test)]
mod verify_tests;
