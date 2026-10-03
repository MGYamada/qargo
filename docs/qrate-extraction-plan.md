# Prepare Qrates and Their Rust Environments for Independent Repositories

Status: implementation contract for the 0.1.7 candidate, revised 2026-10-03 under
[QRATEBOUNDARY.md](../QRATEBOUNDARY.md). This policy supersedes the earlier
proposal for independently advancing standard-tool versions and mixed-version
bundles. The independent Cargo environments, extraction tooling, identity records
and CI checks implement this contract. See [development instructions](qrate-development.md).

## Objective

Make each bundled qrate independently extractable together with its complete Rust
development environment. The extracted project must build and verify its Rust
implementation and manage its Qleisli sources without access to the original
Qargo repository. Cargo configuration, locked dependencies, CLI targets, tests,
licenses, toolchain configuration, and verification instructions must move with
the component.

Actual repository creation and publication are separate future operations.

## Ownership and version boundaries

- Each `Qargo.toml` owns its qrate name, version, Qleisli edition, and inputs.
- Each component's `Cargo.toml` owns its Rust package identity, version,
  dependencies, targets, and Rust edition. Neither manifest inherits from the
  other, and shared sources do not implicitly require matching versions.
- Qlippy, qlifmt, and qlidoc are three independent, ordinary qrates. Their qrate
  and Rust release versions remain aligned with Qargo for bundle convenience;
  alignment grants no privileged qrate status.
- Only `std` is privileged; it is bundled with and versioned alongside Qleisli.
- The root `qargo` package version identifies both qargo and the bundled release.
  Its `Cargo.toml` `[package].version` is the release-orchestration source of
  truth. Component manifests retain literal versions; release validation rejects
  disagreements rather than inferring values or repairing declarations.
- Manifest and result schema versions remain independent of product versions.

Changing captured Rust bytes changes the qrate input identity, without implicitly
changing its version or establishing a change to its Qleisli interface.

## Independent Rust environments

1. Give each development package its own Cargo workspace, explicit dependencies,
   lockfile, and toolchain configuration; exclude it from the root workspace.
   Preserve Rust edition 2024, Rust 1.85 support, exact Qleisli 0.2.1, and the
   unsafe-code prohibition.
2. Register component-owned CLI and integration-test targets. Canonical engines,
   CLI sources, and tests must not depend on the root `qargo_tools` facade. Thin
   root adapters reuse those sources for the public bundle.
3. Keep shared support in qlippy. Qlifmt and qlidoc explicitly depend on it in
   Cargo through the internal support surface defined below. Extraction includes
   its exact source/build configuration snapshot and records version and content
   identity, without reaching into the original checkout or introducing Qargo
   dependencies or a new published crate.
4. Extract the complete declared inventory, preserving relative directory
   relationships and omitting the root Qargo Cargo manifest. Cargo configuration
   stays outside qrate input roots. Run Cargo from the extracted component's own
   development directory. Include the boundary policy and licenses with the
   extracted project documentation.
5. Exercise each environment on Linux/macOS with Rust 1.85 and stable. Prepare
   dependencies explicitly, then verify with frozen dependency resolution.

## Shared support, adapters, and dependency direction

Keep shared-library ownership in qlippy while separating it logically from the
qlippy linter product. Audit the support API independently of the linter's CLI
and rule implementation. Qlifmt and qlidoc may consume capture/snapshot,
diagnostic/transport, identity, syntax, executable, and publication support.
They may not consume lint rules, catalogs, severity/promotion policies, lint
entry points, CLI defaults, or product-version globals. Shared checker support
remains available to Qargo and qlippy; formatting/documentation remain
syntax-only and must not acquire a checker or lint acceptance gate.

Support APIs accept each caller's identity and policy explicitly. A linter
refactor must not require downstream changes unless it also deliberately changes
the shared contract. Every shared API change must compile and pass behavior
checks for all consumers; matching release numbers alone do not establish API
compatibility. No new public support crate or Qleisli dependency is introduced.

The permitted dependency direction is Qargo to tools/support, and tool products
to support. Support must not depend on product layers. Extractable components
must have no normal, build, or development dependency path back to the root
Qargo package or facade. Include conditional target/feature edges and source
inclusions in this audit, not only direct Cargo dependencies. Build scripts and
tests must not read root-only services, configuration, or fixtures. An external
verifier may use an explicitly supplied installed Qargo to manage the qrate;
that does not authorize a component dependency on the original repository.

Root component adapters are wiring only: module assembly, explicit identity,
argument forwarding/representation conversion, streams, and exit status.
Canonical component entry points own CLI interpretation/defaults and engine
behavior. Adapters cannot inject semantic options or duplicate checking, lint,
formatting, or documentation algorithms. This restriction does not remove
Qargo's separately owned orchestration and response validation.

## Exact extraction and build identity

The following identities have different scopes. These govern the
developer-side extraction records; they do not add fields to Qargo manifests or
public result schemas.

| Identity | Scope |
| --- | --- |
| Existing `source_id` | Captured local `.qli` bytes under the existing Qleisli source contract. |
| Existing `input_id` | Raw `Qargo.toml` and the three declared qrate input roots, unchanged from the current specification. |
| Rust project snapshot identity | The complete portable extraction inventory, its dependency closure, and explicit project build configuration described below. |
| Rust build environment binding | The project snapshot plus the actual tools, platform, build choices, admitted external inputs, and generated compilation inputs used for one build. |

### Portable project inventory

Capture original bytes in one immutable, explicitly enumerated inventory. Hash
a canonical sorted UTF-8 inventory of project-relative paths, entry kinds,
relevant executable modes, byte lengths and file-content SHA-256 digests, using
a versioned domain and unambiguous length framing. Include
required empty directories and explicit absence/presence of optional build
configuration. Exclude absolute checkout locations, timestamps, and the record's
own digest. Relocation alone must preserve this identity; added, removed, or
changed admitted entries must change it. Reject symlinks, nonregular files, and
local references escaping the declared extraction/dependency closure.

The inventory must include:

- The complete declared qrate contents, Rust source and test files, and required
  documentation, licenses, fixtures, and build instructions.
- Every participating `Cargo.toml`, the component environment's `Cargo.lock`, and
  local dependency manifests and sources. Preserve literal dependency settings;
  no inherited setting may require the original workspace.
- The effective `rust-toolchain.toml` or `rust-toolchain` declaration, plus all
  effective project `.cargo/config.toml` or legacy `.cargo/config` files. Record
  absence explicitly. Resolve supported precedence explicitly; do not silently
  consume ancestor or user configuration outside the captured project.
- Rust build scripts, included helper files, generator programs/templates, and
  all declared data/configuration inputs they consume. This covers host-side
  Cargo builds only; it does not authorize Qargo to execute qrate build hooks.
- Generated sources already supplied as project inputs. Preserve the exact
  bytes actually supplied, even when a generator is also included.
- Exact dependency identities and sources needed for the admitted build graph.
  Local/vendored sources enter the inventory; registry dependencies bind to
  locked package identities and archive checksums, and Git dependencies to fixed
  revisions and captured content identities. Include required transitive build
  and development dependencies, including generators/procedural macros. Labels,
  paths, or version strings alone are not content identities.

Ordinary `target/` contents, Cargo/Rustup caches, `.git`, logs, timestamps, and
previous binaries are not project inputs. A required generated input must be
explicitly captured or regenerated under the build binding; it must not be
accidentally supplied from a previous build's output directory.

### Actual build environment and generated sources

Use isolated Cargo/Rustup configuration and an explicit build-environment
allowlist for extraction verification. No original-repository paths or ambient
configuration may influence dependency resolution or compilation. Record the
effective values of admitted build settings; keep credentials and unrelated
process environment out of captured records.

Bind the project snapshot to the actual Rust compiler, Cargo, and invoked
generators/build helpers, using implementation versions and content identities.
A toolchain channel declaration alone does not identify the resolved compiler.
Also record host/target, selected packages/targets, profile, feature selections,
compiler/linker flags, admitted build-affecting environment, and the identities
of any native compiler, linker, SDK/sysroot, headers, or libraries actually used.
Path names alone are insufficient for these external inputs. Configured wrappers
and other external tools must be explicitly admitted and identified or rejected.

Record the bytes/digests of generated sources actually consumed by compilation
alongside their generator and input binding. This includes generated inputs from
dependency build steps. A changed generated source changes the build binding
even if the generator source and project snapshot are unchanged. Ordinary final
binaries retain their separate artifact/executable digests. Unsupported or
unrecorded build-affecting inputs must fail extraction verification rather than
receive a claim of complete environment capture.

Project identity, build-environment binding, and artifact identity are separate.
The same Qleisli source identity may accompany different Rust environments; those
environments must be distinguished in the extraction record. These records do
not promise reproducible binary bytes, authenticate publishers, or establish
mathematical correctness.

## Bundle integration and public contracts

- Keep standard qrate, private Rust package, and executable versions aligned
  with Qargo through explicit release preparation and validation. Do not turn
  this distribution policy into manifest inheritance or a rule for other qrates.
- Standalone engines obtain product identity from their own Rust packages;
  bundle adapters reuse canonical sources and preserve that identity. Extracted
  projects must not require root-only build machinery.
- Retain current same-release tool transport validation, checker/profile and
  executable identities, input binding, and exit consistency. Extraction does
  not itself change the auxiliary-tool compatibility contract.
- Retain manifest schema 2 and result/build-record schema 1. Introduce no Qargo
  dependency resolution or new Qargo manifest fields.
- Preserve the native archive format, atomic bundle installation, and Cargo-free
  installer. Mixed-version bundle metadata is not required for extraction.
- Keep the public `.crate` self-contained and independent of private manifests.

## Acceptance and scope

- Audit the logical shared API and all dependency directions. Reject lint-policy
  imports from formatter/documenter, product-layer imports from support, and any
  direct/transitive component dependency on root Qargo, including test/build and
  conditional edges. Review root adapters for wiring-only ownership.
- Compare standalone and embedded component entry points on identical inputs,
  options/defaults, diagnostics, candidate outputs, effects, and exit codes.
  Include unknown/invalid arguments, empty input, and parse/check failures where
  applicable. Account explicitly for expected executable-digest differences;
  do not normalize away product identity or semantic differences.
- Extract each component and its explicit dependency closure; reject Cargo
  metadata containing local package or target paths outside the extraction.
- Build library/CLI and run engine/snapshot tests, doctests, Clippy, and Rustdoc
  without the original checkout. Validate the relocated qrate using supplied
  installed tools; relocation preserves input and source identities.
- Verify project-identity changes for each admitted Cargo manifest, lockfile,
  toolchain/Cargo configuration, helper script, dependency input, and supplied
  generated source; configuration creation/deletion also changes the identity.
  These changes must not silently broaden existing `source_id`/`input_id` scope.
- Verify that identical project snapshots built with different actual toolchain,
  flags, targets, features, admitted native inputs, or generated compilation
  bytes have different build bindings. Reject ambient configuration and escapes;
  relocation and changes to excluded caches must not change project identity.
- Exercise unrelated qrate/Cargo fixtures with independent names, versions, and
  editions, including absent or invalid unrelated Cargo metadata during Qargo
  operations. These fixtures do not change the standard bundle release policy.
- Verify aligned standard-tool versions in standalone builds, bundled builds,
  packaged installation, and native installation. Bundling must not bypass
  ordinary input, identity, or checking validation for any qrate.
- Deliberately mismatch each standard manifest, executable version, or
  distribution expectation against root Cargo `[package].version`; release
  validation must fail without rewriting or implicitly inheriting values.
- Reject tool schema, version, checker/profile, executable, and input mismatches
  under the current transport contract.
- Preserve empty inputs, syntax-only results, unavailable QLT, Cargo/Rustdoc
  traps, source/package validation, and all supported native distribution checks.

Keep standard versions aligned during release preparation. Do not create remote
repositories, publish packages, add a registry, change the Qleisli pin, or add
qrate-controlled build hooks as part of this preparation.

## Implemented verification profile

`scripts/qrate_project.py` captures the complete portable project, including
vendored registry sources and archive/file checksums for inactive platforms as
well as the selected graph. `scripts/verify_qrate.py` runs from that captured
project with fresh Cargo/Rustup homes, a fresh target, frozen resolution and an
explicit environment. `scripts/capture_build.py` binds actual native inputs and
each Cargo stage's generated build outputs/dep-info. Final executable digests are
recorded separately. Native inventories are checked again after verification.

The initial supported development hosts are native Linux x86_64 GNU with system
GCC and macOS with Command Line Tools. macOS denies unrecorded file reads using
Seatbelt; Linux audits successful file opens and executable launches with strace.
OS process/device interfaces and fresh build/test state are explicit runtime
inputs, not portable project inputs. Records are observations under this fixed
profile, not a sandbox attestation or bit-reproducibility claim. Unsupported
Cargo configuration, source kinds, cross compilation and external tools fail
until their capture profiles are specified. Existing native distribution builds
retain their separate verification, including Linux musl.

The independent CI job moves the original checkout away and removes read access
during verification. Each component is checked separately on Linux/macOS with
Rust 1.85 and stable. Standard-bundle version alignment and all existing public
transport, package and native-installation contracts remain unchanged.
