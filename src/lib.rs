//! Local qrate management and standard source tools; no acceptance authority beyond Qleisli.

// Embedded engines share the same support as the private development crates.
extern crate self as qlippy_engine;

#[path = "../qrates/qlidoc/src/lib.rs"]
pub mod qlidoc_engine;
#[path = "../qrates/qlifmt/src/lib.rs"]
pub mod qlifmt_engine;
#[path = "../qrates/qlippy/src/lib.rs"]
mod qlippy_support;

mod bundled;
mod installation;
pub mod qargo;
pub use qlippy_support::{
    PROFILE, QLEISLI_VERSION, VERSION, adapter, qlippy, report, rules, snapshot, source,
};
