# Changelog

## 0.1.1 — 2026-09-30

- Distribute the standard bundle as the self-contained crates.io package `qargo`, installing all four executables together. Compile the canonical qrate engine sources as internal modules and verify packaged sources and installation in CI; private engine packages remain development-only.
- Develop Qargo and the standard qlippy, qlifmt, and qlidoc bundle together at 0.1.1. Each qrate contains its Rust engine, CLI, tests, and a minimal Qleisli management sample; developer Cargo manifests remain outside the qrates.
- Add syntax-preserving Qleisli formatting through qlifmt and `qargo fmt`, including a non-writing `--check` mode, comment preservation, four-space indentation, and LF/CRLF preservation.
- Add deterministic Markdown documentation through qlidoc and `qargo doc`, with public declarations by default, optional private declarations, and atomic reuse of identical documentation artifacts. Future HTML may adopt independent CSS and layout.
- Bind new child-tool results to frozen sources and executable identities; validate formatting candidates and documentation artifacts before accepting them. Formatting and documentation use syntax parsing and make no mathematical verification claim.
- Extend source-archive verification and Linux/macOS CI to three qrates and four executables, including Cargo and Rustdoc invocation traps.
- Preserve the manifest schema and existing result versions at 1; introduce independent version-1 qlifmt.result and qlidoc.result formats. Retain Rust 1.85, exact Qleisli 0.2.1, existing qlippy rules, and explicitly unavailable QLT execution.

## 0.1.0 — 2026-09-30

Initial experimental GitHub source release for Linux and macOS. Requires Rust 1.85 or newer and links exactly Qleisli 0.2.1 with the finite-v0 profile.

- Manage one local qrate with a strict, versioned `Qargo.toml`, ancestor discovery and explicit manifest selection.
- Check frozen `.qli` inputs through ordinary Qleisli source/IR checks; empty sources explicitly skip checking.
- Snapshot declared Rust, Qleisli, test and document inputs; generate portable SHA-256 identities, public module indexes and deterministic build records without overwriting conflicting artifacts.
- Invoke the sibling, explicitly selected or PATH-discovered qlippy engine on the same source snapshot, with strict validation of child diagnostics and identities.
- Provide advisory `unused_import`, `redundant_repeat_one` and `double_inverse` warnings with original UTF-8/CRLF coordinates, suggestions and optional warning denial.
- Publish separate version-1 JSON envelopes for Qargo and qlippy and exit codes 0/1/2.
- Include the complete qlippy Rust sources and a minimal Qleisli identity operation in its qrate.
- Validate Linux/macOS with Rust 1.85/stable, including rebuilding and exercising the source archive.

QLT and qlidoc are unavailable: `qargo test` and `qargo doc` fail explicitly. No Cargo command is invoked by Qargo. Registry/dependency resolution, binary distribution, crates.io publication, Windows/Android support and automatic fixes are deferred. Build records and lint success are metadata, not mathematical evidence.
