# Changelog

## 0.1.7 — 2026-10-03

- Retain captured directory capabilities through formatting and build/document publication, rejecting byte-identical root/ancestor replacements before effects and preventing redirection to replacement paths (#24).
- Revalidate the captured Qargo running-image identity before check acceptance and build publication; derive the build record and output path from that same identity (#25).
- Clear parent-unverified change lists, formatted identity and document inventories on failed child execution while preserving transport, diagnostic and identity validation (#26).
- Report documentation `artifact_path` only after successful publication or exact reuse, leaving it null on conflicts and output failures in Qargo and standalone qlidoc (#27).
- Adopt `QRATEBOUNDARY.md`: every internal or external qrate has an independent environment boundary, including its Rust development environment. qlippy, qlifmt, and qlidoc remain three ordinary qrates; their aligned Qargo versions are a bundle convenience, not a privilege. Only std has privileged qrate status and is bundled with and versioned alongside Qleisli.
- Give each tool an independent Cargo workspace, explicit dependencies, lockfile, toolchain declaration, canonical CLI and Rust tests. Extract its qrate, Rust environment and exact vendored dependency closure without a root Qargo manifest. Verify standalone builds, tests, documentation, CLI equivalence and qrate relocation with no original-checkout access.
- Separate qlippy shared support from its lint product; require caller-owned report versions and wiring-only bundle adapters. Audit reverse/conditional dependencies and source inclusions. Record portable project identities separately from native build inputs, generated outputs and executable digests. Root Cargo package version is the release-orchestration authority; component manifests retain literal versions checked by release validation.
- Add independent component CI on Linux/macOS with Rust 1.85 and stable, frozen vendored resolution and isolated Cargo/Rustup configuration. Preserve Qargo/Cargo manifest independence, including invalid unrelated Cargo metadata.
- Update Qargo, the standard tools, qrate manifests, private Rust packages, lockfile, CI, and release/package verification to 0.1.7. Include the boundary policy and extraction plan in source/package validation.
- Preserve exact Qleisli 0.2.1, Rust 1.85, explicit Qleisli/Rust editions, manifest schema 2, result schema 1, current same-release tool compatibility, and existing native archive formats. Publication remains a separate action.

## 0.1.6 — 2026-10-03

- Require the exact top-level published release asset set before checksum, archive, installer, and runtime validation. Keep nested Actions artifact collection and verification of older release tags supported (#21).
- Reject directory-content changes during input capture by comparing two bounded descriptor-anchored inventories and directory metadata, including extension-filtered entries. Preserve the logical traversal budget and existing replacement checks (#20).
- Establish Qargo's first principles: Exact, Immutable, Bound, Explicit, Independent. Refactor orchestration into typed requests, immutable captured subjects, bound checking/tool validation, and explicit effects. Give each checker/tool its own materialized sources and accept child transport through one guarded execution boundary before source updates or artifact publication.
- Capture the orchestrator identity before child execution and revalidate it before response acceptance, so host-identity rejection prevents parent-side formatting writes and documentation publication.
- Update Qargo, all three standard tools, their qrate manifests, private development packages, lockfile, CI, documentation, and package/release verifiers together to 0.1.6.
- Adopt a qleisliup compatibility plan covering offline proxy execution, complete same-release bundles, executable and checker identities, explicit failure, and a coordinated exact linked-Qleisli update for the manager's initial production language distribution. Keep external-checker integration dependent on its own upstream and Qargo contracts.
- Preserve exact Qleisli 0.2.1, Rust 1.85, manifest schema 2 with explicit edition "2026", independent result schema version 1, syntax-only formatting/documentation, and unavailable QLT execution. Publication requires separate authorization.

## 0.1.5 — 2026-10-01

- Add Linux x86_64 and macOS x86_64/ARM64 binary bundles and a Cargo-free installer, with SHA-256 verification, version selection, user-prefix installation, and atomic bundle switching. Linux ARM64 is deferred because Qleisli 0.2.1's file loader uses incompatible flags on that architecture (#18); the installer reports this limitation before downloading or changing an installation.
- Add native Rust 1.85 distribution checks, archive and installation verification, and CI artifacts; retain independent source and crates.io package validation.
- Keep Qleisli exactly at 0.2.1, Rust 1.85, manifest schema 2 with explicit edition "2026", and independent result schema version 1. Publication requires separate authorization.
- Bind selected-tool launch to captured executable bytes and host identity to the operating system's running image object, rejecting installation-path A/B/A redirection and changed macOS host images (#14).
- Anchor qrate and standalone input capture to retained directory descriptors; reject root, ancestor, directory and file replacement during capture without following symlink redirection (#13).

## 0.1.4 — 2026-10-01

- Update Qargo, all three standard tools, their qrate manifests, private development packages, lockfile, CI, documentation, and package/release verifiers together to 0.1.4.
- Ignore macOS `.DS_Store` files and Python bytecode alongside existing build outputs and Python cache directories. Keep `Cargo.lock` tracked for reproducible development, MSRV checks, and locked installation.
- Separate Qargo argument parsing, manifest validation/input capture, and child-tool response validation. Share closed response schemas and tool-identity checks across linting, formatting, and documentation without changing CLI behavior or result formats.
- Reject declared-root overlap and reserved `target` capture through filesystem aliases, using directory identities while preserving distinct case-sensitive directories (#9).
- Keep selected-tool process-group cleanup armed through response and output validation, terminating detached-output descendants on rejected lint, format, or document transport (#10).
- Clarify the separate 4096-entry traversal and 4096-file capture budgets, and cover directory, filtered-file, and depth boundaries without changing the limits (#11).
- Preserve manifest schema 2, independent result schema version 1, Rust 1.85, and exact Qleisli 0.2.1.

## 0.1.3 — 2026-09-30

- Update Qargo, all three standard tools, their qrate manifests, private development packages, CI, and package/release verifiers together to 0.1.3.
- Introduce manifest schema 2 with mandatory `[qrate].edition = "2026"`. Require an explicit TOML string, reject omission and unsupported editions before any qrate operation, and provide a migration suggestion for schema-1 manifests. No edition default is provided.
- Add the edition to every bundled manifest and valid example/fixture, and require explicit editions in the specification and working guidelines. Qleisli edition `"2026"` and the Rust implementation's Cargo edition `"2024"` are independent.
- Reject standalone source roots combined with `--manifest-path` for lint and formatting, preventing unrelated qrate identities from being attached to standalone results (#3).
- Reuse one descriptor-anchored publication kernel for build and documentation, including exact empty-directory inventories, race-resistant staging/cleanup, no-replace installation, and byte-identical concurrent reuse (#4).
- Terminate selected-tool process groups on deadline or transport failure, and use cancellable nonblocking pipe reads to bound cleanup when descendants inherit output descriptors (#5).
- Require `compiler_error` as the only failed nonempty-source qlippy check reason, retaining existing status/outcome/diagnostic consistency checks (#6).
- Document the adopted tooling implementation order: local diagnostic explanations, pinned compatibility comparisons, explicit documentation-example checks, then scoped review records. Keep toolchain selection, structured repairs, and QLT execution dependent on separately specified upstream protocols; these remain future features.
- Preserve independent result schema version 1, Rust 1.85, exact Qleisli 0.2.1, syntax-only formatting/documentation, empty-source behavior, and explicitly unavailable QLT execution. Publication remains a separate authorized action.

## 0.1.2 — 2026-09-30

- Update Qargo, all three standard tools, their qrate manifests, private development packages, CI, and package/release verifiers together to 0.1.2.
- Accept both `--option=PATH` and `--option PATH` for Qargo's manifest/tool paths and qlidoc's output path. Retain rejection of empty, missing, repeated, and incompatible options, and selected-tool authority without fallback.
- Add actionable suggestions for unsupported Cargo-style qrate dependency commands and `[dependencies]`/`[dev-dependencies]` manifest tables. Preserve strict TOML string field types; unquoted date/time root values are rejected even when similarly named directories exist.
- Introduce `qlippy --list-rules`, with an independently versioned machine-readable catalog of idiom, complexity, and resource groups, plus advisory/checker-candidate/theorem-candidate policies. Existing rules remain advisory warnings; lint result envelopes are unchanged.
- Clarify qrate semantic scope versus raw input provenance, the host-language boundary, and checker-bound stdlib identity. Document the planned separation of qrate-selected Qleisli toolchains and Qargo orchestration, including the required checker/backend/stdlib bindings before proof-bearing results can be supported.
- Adopt a curated stdlib center and future extensions with trusted sources, authenticated namespaces, and fixed identities. Defer public registry and automatic semver resolution; forbid arbitrary qrate build hooks, native procedural macros, dependency-install scripts, and global feature unification. A future lockfile will serve reproducibility and provenance, independently of proof.
- Preserve manifest/result schema version 1, Rust 1.85, exact Qleisli 0.2.1, syntax-only formatting/documentation, empty-source behavior, and explicitly unavailable QLT execution. Publication remains a separate authorized action.

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
