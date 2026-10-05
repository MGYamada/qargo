# Qrate boundaries

Status: adopted policy for Qargo 0.1.7, 2026-10-03. This document governs qrate
ownership and environment boundaries. The [specification](docs/specification.md)
defines implemented commands and schemas. Independent Rust environment extraction and verification are developer operations
described in [qrate development](docs/qrate-development.md).

## 1. Every qrate has an independent environment

Treat every qrate as a distinct package environment, analogous to a Rust crate,
whether internal to this repository or maintained externally. Repository
location, common ownership, source sharing, and distribution membership do not
erase its boundary. Each qrate owns its declared inputs, configuration, identity,
and checking results. Success for one qrate does not certify another.

`Qargo.toml` and `Cargo.toml` describe separate management domains. Qargo owns
Qleisli qrate operations; Cargo owns Rust implementation builds and dependencies.
Neither manifest implicitly supplies the other's name, version, or edition.
Qleisli edition `"2026"` and Rust edition `"2024"` remain explicit and independent.
Qargo never invokes Cargo or substitutes Rust tests/Rustdoc for Qleisli operations.

A qrate with a Rust implementation must be separable together with its complete
Rust environment: manifests, locked dependency closure, CLI targets, tests,
toolchain configuration, licenses, and build/verification instructions. The
extracted project must build and verify its Rust implementation and manage its
Qleisli sources without access to the original Qargo repository. A qrate snapshot
alone need not be a Rust build package; both boundaries must be transferred and
verified explicitly.

## 2. Standard tools are three ordinary qrates

`qlippy`, `qlifmt`, and `qlidoc` are bundled with the Qargo environment as standard
tools. They remain three independent qrates, subject to the same boundaries as
any internal or external qrate.

Shared Rust support belongs to qlippy and is an explicit implementation dependency
of the other tools. Reuse does not merge their qrate identities or checking
results. Default discovery and standard installation are distribution
conveniences, not privileged qrate semantics.

### Shared support is not the linter product

Keep a logical internal shared-library boundary within qlippy's ownership. Its
allowed surface consists of input capture, identities, diagnostic data and
transport, syntax utilities, executable handling, and validated publication.
The ordinary checker adapter may serve Qargo and qlippy, but qlifmt and qlidoc
must remain syntax-only. This does not require a new published support crate.

Other tools must not depend on qlippy's lint rules, rule catalog, promotion or
severity policies, lint command, CLI defaults, or product-version globals. They
must not gain Qleisli semantic dependencies or acceptance conditions through
that Rust dependency. Each caller supplies its own product identity and policy;
shared support never silently chooses the linter's identity or behavior for it.
Shared API changes require downstream compilation and behavior checks for all
consumers. Linter-only changes must remain behind the linter boundary.

### Dependency direction and adapter ownership

The dependency graph must remain acyclic: Qargo may depend on standard tools and
shared support; tool implementations may depend on shared support; shared
support must not depend on tool-product layers. No extractable component may
depend directly or transitively on the root Qargo package or `qargo_tools`
facade, including through build or development dependencies. Repository splits
must preserve this direction. A supplied installed Qargo executable may be used
by the external extraction verifier; it is not a component build dependency.

Root adapters for embedded components may only wire modules, explicitly supplied
identity, arguments, streams, and exit status to canonical component entry
points. CLI interpretation and defaults, checking, linting, formatting, and
documentation algorithms belong to their canonical owners. Adapters must not
duplicate or override that behavior, introduce hidden defaults, or call back
into root services from a component. Qargo's own capture, orchestration, and
response-validation responsibilities are separate from these thin adapters.
Isolation and adapter-equivalence checks are specified in the extraction plan.

## 3. Aligned tool versions are a distribution policy

Keep the standard qlippy, qlifmt, and qlidoc releases aligned with Qargo for bundle
convenience. For 0.1.8 this includes the three qrate manifests, their private Rust
development packages, and all four executables. Release preparation explicitly
updates and validates these declarations.

The root `Cargo.toml` `[package].version` is the single source of truth for
release orchestration. Every standard qrate and private Rust package keeps a
self-contained literal version in its own manifest. Release validation compares
those declarations, executable reports, and distribution expectations with the
root release version and rejects disagreement; it does not infer, silently
repair, or introduce manifest inheritance. Other qrates retain their own version
authority. This synchronization rule does not apply to Qleisli or std.

Alignment grants no privilege, checking exemption, implicit trust, or access to
another qrate's environment. It does not require unrelated qrates or coexisting
Qargo/Cargo manifests to share a version. Independent ownership and environments
do not require independently advancing release numbers.

The current same-release auxiliary-tool requirement is a tool transport contract,
not a general rule for qrate versions. Changing that contract requires a separate
specification and validation. Manifest and result schema versions remain
independent of product releases.

## 4. Only std has privileged qrate status

The Qleisli standard library, `std`, is the sole privileged qrate. It is bundled
with Qleisli itself and its version is aligned with Qleisli, not Qargo. Qleisli
owns its reserved namespace, distribution, and semantic integration. Neither the
three standard tools nor any other internal or external qrate acquires this
status by being bundled or adopting a similar name.

Qargo currently consumes the stdlib embedded in its exactly pinned Qleisli 0.2.1
implementation. Its identity is bound to that checker distribution and the actual
host executable. This policy adds no independently resolved std package or new
manifest field. Privileged distribution status, version metadata, and hashes are
not mathematical evidence and do not extend Qleisli checking to Rust code.

## Implementation boundary

The existing Qleisli `source_id` and qrate `input_id` retain their specified
meanings; neither identifies a complete Rust build environment. The extraction
plan separately defines the captured Rust project inventory and the recorded
build environment, including Cargo manifests/lockfiles, toolchain and Cargo
configuration, build scripts, dependency inputs, and generated sources. Equal
source identities must not be presented as equal build environments.

The [extraction contract](docs/qrate-extraction-plan.md) separates each qrate and
its Rust environment. Each `rust/<tool>/` is an independent Cargo workspace,
transferred with `qrates/<tool>/` and its explicit support closure. The public
bundle retains canonical-source embedding through wiring-only adapters. Version
alignment continues during extraction; independent tool release trains and
mixed-version bundle formats are not required by this policy.

The [ecosystem policy](docs/ecosystem-policy.md) continues to govern provenance,
future dependency sources, and stdlib governance. This decision introduces no
registry, automatic version resolution, qrate-controlled build hooks, or
publication authorization.
