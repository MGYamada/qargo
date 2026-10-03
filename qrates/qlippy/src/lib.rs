//! Qlippy product and its separately audited internal Rust support surface.

pub mod qlippy;
pub mod rules;
pub mod support;

pub use support::{
    PROFILE, QLEISLI_VERSION, adapter, executable, publication, report, snapshot, source,
};
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
