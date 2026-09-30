//! Pure Rust qlippy engine. Its source belongs to the qlippy qrate.

pub mod adapter;
pub mod qlippy;
pub mod report;
pub mod rules;
pub mod snapshot;
pub mod source;

pub const QLEISLI_VERSION: &str = "0.2.1";
pub const PROFILE: &str = "finite-v0";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
