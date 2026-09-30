# qlippy qrate

This local qrate contains one short Qleisli management sample, `src/smoke.qli`, and zero QLT test files. The sample exports the unitary `identity` function, which returns its input unchanged. Its Rust engine, CLI, shared support and Rust tests also belong to this qrate and are included in its input identity and build snapshot. Developer Cargo configuration remains outside the qrate; Qargo does not compile the Rust implementation. The engine can diagnose external Qleisli projects. Empty Qleisli-source checks remain supported and are explicitly reported as not run.

QLT and qlidoc are unavailable in this initial implementation. Qargo never substitutes Cargo tests or Rustdoc for those tools. Build records and lint results are metadata, not mathematical evidence.
