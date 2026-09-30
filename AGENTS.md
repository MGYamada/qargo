# Qargo working guidelines

- Qargo manages Qleisli qrates; Cargo builds the Rust implementation during development. Never invoke Cargo from a Qargo command or translate Qargo test/doc into Rust tests/Rustdoc.
- The qlippy qrate includes all of its Rust engine, CLI, shared support, and Rust test sources, plus a minimal src/smoke.qli management sample. It has no .qlt files. Developer Cargo configuration lives outside the qrate. Use source strings and temporary directories for lint-rule tests; preserve coverage of empty qrate roots.
- Read docs/specification.md before changing public behavior. Manifest and result schema versions are independent of product versions.
- Preserve ordinary Qleisli checking and its trust boundary. Empty source roots have no Qleisli checking result. Lint results and build records are metadata, not mathematical evidence.
- Specifications, public documentation, diagnostics, and code comments are English. User conversation can follow the user's language.
- Rust 1.85 is the MSRV; forbid unsafe code. Keep the Qleisli dependency exactly at 0.2.1.
