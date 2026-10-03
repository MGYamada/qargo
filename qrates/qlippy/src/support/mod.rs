//! Internal shared contracts. This layer must not depend on tool products.

#[path = "../adapter.rs"]
pub mod adapter;
#[path = "../executable.rs"]
pub mod executable;
#[path = "../publication.rs"]
pub mod publication;
#[path = "../report.rs"]
pub mod report;
#[path = "../snapshot.rs"]
pub mod snapshot;
#[path = "../source.rs"]
pub mod source;

pub const QLEISLI_VERSION: &str = "0.2.1";
pub const PROFILE: &str = "finite-v0";
