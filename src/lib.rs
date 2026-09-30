//! Local qrate management and linting; no acceptance authority beyond Qleisli.

mod installation;
pub mod qargo;
pub use qlippy_engine::{PROFILE, QLEISLI_VERSION, VERSION, adapter, qlippy, report, snapshot};
