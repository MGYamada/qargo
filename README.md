# qlirgo

**Qleisli and its tools (qlirgo, qlinty, qlifmt, qlidoc, qleisliup, and qleisliup-init) are independent projects and are not affiliated with, endorsed by, or sponsored by the Rust Project or the Rust Foundation.**

**Rust and Cargo are trademarks of the Rust Foundation.**

Qlirgo captures declared inputs, binds results to those bytes and the actual tools,
and applies only validated effects. The [first principles and architecture](docs/architecture.md)
define these responsibilities and their implementation boundaries.

Local Qleisli qrate management with qlinty, qlifmt, and qlidoc. The four Rust executables use product version 0.2.0-beta; the three bundled qrates retain base version 0.2.0. This explicit bundle-version mapping does not merge their independent environments. The three tool qrates retain independent boundaries and have no privileged qrate status; see [QRATEBOUNDARY.md](QRATEBOUNDARY.md).

Since version 0.1.7, the tools have independent Rust environments and extraction verification; see the [development instructions](docs/qrate-development.md) and [extraction contract](docs/qrate-extraction-plan.md).

**Beta candidate:** qlirgo 0.2.0-beta is being prepared and is not yet published.
The existing crates.io qlirgo 0.1.0 is a namespace reservation. Published qargo
0.2.0 and 0.2.0-final are terminal migration notices; they do not run qrate commands.
The previous functional qargo 0.1.9 and its Qleisli 0.2.1 artifacts remain unchanged.

This development version pins Qleisli `=0.3.0-beta`, retains parent result/build-record
schema 2 and child schema 1, and requires explicit Qleisli edition `"2026"`.
The beta dependency is not yet on crates.io; local source-substitution checks do
not establish package, installer or public-release readiness.

`Qlirgo.toml` is the preferred qlirgo manifest name during this transition.
`Qargo.toml` remains accepted when the new name is absent. Both may coexist only
with exactly identical bytes. Bundled qrates retain identical copies for direct
Qleisli beta use. Qleisli itself completes its naming/manifest migration at its
0.3.0 stable release; this repository does not change that dependency's std or loader.
See the [naming contract](docs/naming-migration.md) and [linked development history](docs/linked-qleisli-030.md).

Linux ARM64 remains outside the supported distribution targets. Published qargo
0.1.9 retains its Qleisli 0.2.1 loader limitation ([issue #18](https://github.com/MGYamada/qlirgo/issues/18));
new platform support requires fresh validation.

**Qlirgo manages Qleisli packages. Cargo builds, packages, and installs the Rust implementation outside qlirgo operations.** Qlirgo commands never invoke Cargo. They do not translate mathematical tests into Rust tests or Qleisli documentation into Rustdoc.

Each standard qrate ([qlinty](qrates/qlinty/Qlirgo.toml), [qlifmt](qrates/qlifmt/Qlirgo.toml), and [qlidoc](qrates/qlidoc/Qlirgo.toml)) contains its complete Rust engine, CLI, tests, and a short `src/smoke.qli` identity operation, with no `.qlt` files. Shared support belongs to qlinty. Qlirgo snapshots and identifies both `.rs` and `.qli` inputs, checks the `.qli` source, and leaves Rust compilation to developer Cargo configuration outside the qrates. Each engine also accepts external Qleisli sources. The candidate `qlirgo` crate compiles these same engine sources as internal modules; the three private packages each have an independent Cargo workspace for development.

A qrate's semantic surface is its Qleisli modules and contracts; its raw input identity also tracks host implementation inputs for provenance. Checking a bundled smoke sample does not certify the Rust engine. The development target links Qleisli `=0.3.0-beta` and its actual embedded std descriptor. Nonempty checking selects one native kernel through Qleisli's existing explicit selection contract, including `QLEISLI_KERNEL`; a PATH `qleisli` does not select the linked frontend. Observed native paths and `finite-v0` are diagnostic/adapter metadata, not binary attestation or source-preservation proof. The [toolchain design](docs/toolchains.md) distinguishes future acceptance selection.

The [ecosystem policy](docs/ecosystem-policy.md) puts a curated stdlib at the center. Future extensions require trusted sources, authenticated publisher namespaces, and fixed identities. Public registry and automatic semver resolution are deferred; arbitrary qrate build hooks, native procedural macros, dependency-install scripts, and global feature unification will not be introduced. Cargo's command structure does not determine Qleisli's governance.

The [tooling adoption plan](docs/adoption-plan.md) sets the implementation order for local diagnostic explanations, compatibility comparisons, documentation examples, and scoped review records. Upstream toolchain, structured-repair, and QLT protocols determine their later integration.

The [qleisliup compatibility plan](docs/toolchains.md#qleisliup-compatibility-plan) starts with offline proxy execution of a complete qlirgo bundle. The named linked-library migration can precede an external acceptance adapter; it adds no manager-state dependency or new selector. Qlirgo still does not interpret `qleisli-toolchain.toml`. Actual standalone/linked/std distribution alignment remains a separate validation gate.

## Installation

The commands below select the planned beta explicitly. They become available
only after Qleisli 0.3.0-beta is published to crates.io, registry/package and all
supported-host checks pass, and this beta is published. Local source-override
checks do not establish those release gates. Beta releases do not become latest.

### Install the four Rust tools

Use a fresh prefix during migration: qlifmt and qlidoc keep their command names,
and an old qargo installation must not be overwritten automatically.

```sh
cargo install qlirgo --version '=0.2.0-beta' --locked --bins --root "$HOME/.local/qlirgo-beta"
export PATH="$HOME/.local/qlirgo-beta/bin:$PATH"
```

The crate builds the canonical engine sources as modules; private Rust workspace
packages are not required. Source builds require Rust 1.85 or newer; macOS also
requires the Xcode command line tools, including Clang/libclang.

### Install a binary bundle without Cargo

```sh
curl --proto '=https' --tlsv1.2 -fsSL https://github.com/MGYamada/qlirgo/releases/download/v0.2.0-beta/install.sh -o install.sh
sh install.sh --version 0.2.0-beta --prefix "$HOME/.local/qlirgo-beta"
export PATH="$HOME/.local/qlirgo-beta/bin:$PATH"
```

The installer verifies checksums, inventory and the four exact executable
versions. Supported archives are Linux x86_64 and macOS x86_64/ARM64; Linux ARM64
remains unsupported. Installation needs POSIX shell, curl, tar, SHA-256 tooling
and standard Unix utilities, with neither Rust/Cargo nor Python/jq. State is
stored under lib/qlirgo in the selected prefix. Existing command collisions stop
with a separate-prefix instruction. No sudo or shell configuration is used.

### Select the matching Qleisli native checker

The qlirgo archive contains four tools, LICENSE and NOTICE. Obtain the matching
native checker from the Qleisli 0.3.0-beta distribution separately and select its
actual absolute executable path:

```sh
export QLEISLI_KERNEL=/absolute/path/to/matching/qleisli-kernel
qlirgo --version
```

Nonempty check/build/lint require that checker. Formatting and documentation
remain syntax-only. Valid empty source inputs make no native call; this is not
a successful Qleisli check. `qlirgo test` continues to fail explicitly because
QLT execution is unavailable.

## Build from source

Rust 1.85 or newer is required. The development source targets exact Qleisli `=0.3.0-beta`; published legacy qargo 0.1.9 retains its exact 0.2.1 dependency. The commands below require a reviewed available dependency and locked closure. A development-only local link must be labeled explicitly and does not establish registry/package readiness; follow the [migration gates](docs/linked-qleisli-030.md#validation-status).

```sh
git clone https://github.com/MGYamada/qlirgo.git
cd qlirgo
# Explicit local-development source override; requires the matching beta checkout.
cargo --config 'patch.crates-io.qleisli.path="/absolute/path/to/Qleisli"' build --offline --locked --bins
```

This builds `target/release/qlirgo`, `qlinty`, `qlifmt`, and `qlidoc`. Keep the executables together for default sibling discovery, or supply `--qlinty=PATH`, `--qlifmt=PATH`, or `--qlidoc=PATH`. Build all four from the same product release. An explicitly selected tool is authoritative; a selected tool's failure is returned without falling back to another executable. Cargo fetches Rust dependencies during development; qlirgo does not fetch or build engines.

The public bundle builds from the root Cargo manifest and canonical qrate sources without private development manifests. To work on one tool, use its independent `rust/<tool>/` workspace. The [extraction workflow](docs/qrate-development.md) transfers that Rust environment, the corresponding `qrates/<tool>/` tree and its explicit dependency closure together, and verifies them without the original checkout. A qlirgo snapshot alone preserves declared qrate inputs and is not a complete Rust build project. Full extraction verification is available through the manually dispatched `Verify independent qrate extraction` workflow; ordinary push/PR CI retains the lightweight boundary audits.

Keep the root and all three component `Cargo.lock` files in version control so development, CI, Rust 1.85 checks, and `cargo install --locked` use the recorded dependency versions. Build outputs, macOS `.DS_Store` files, and Python caches and bytecode are ignored.

Development verification scripts require Python 3.11 or newer. The installed
tools and Cargo-free installer do not require Python.

## Manage a bundled qrate

Qlirgo 0.2.0 requires manifest schema 2 and an explicit Qleisli edition in every `Qlirgo.toml`. The only supported edition is the string `"2026"`; omission has no default and is an error. A minimal `Qlirgo.toml` is:

```toml
schema-version = 2
[qrate]
name = "example"
version = "0.2.0"
edition = "2026"
[source]
root = "src"
[tests]
root = "tests"
[docs]
root = "docs"
```

All three root directories must exist, even when empty. During the beta transition, copy the exact manifest bytes to `Qargo.toml` as well when direct Qleisli beta use is needed; qlirgo never creates that user file automatically. Migrate older manifests by changing `schema-version` to 2 and adding `edition = "2026"` under `[qrate]`. Include the edition in every valid manifest, example, and fixture. Rust's `Cargo.toml` files independently retain edition `"2024"`.

```sh
target/release/qlirgo check --manifest-path=qrates/qlinty/Qlirgo.toml
target/release/qlirgo build --manifest-path=qrates/qlinty/Qlirgo.toml
target/release/qlirgo lint --manifest-path=qrates/qlinty/Qlirgo.toml --deny-warnings
target/release/qlirgo fmt --manifest-path=qrates/qlinty/Qlirgo.toml --check
target/release/qlirgo doc --manifest-path=qrates/qlinty/Qlirgo.toml
```

Use either of the other qrate manifests with the same commands. Qlirgo operates on one qrate at a time. For `lint` and `fmt`, an explicit standalone source root cannot be combined with `--manifest-path`; use one input mode per command. `check` uses ordinary Qleisli source/IR checking. `build` writes an exact input snapshot, public module index, and build record under the qrate's `target/qlirgo/`; it does not run document generation. The smoke sources export `identity`; their migrated checking/linting is a required validation gate. Valid empty source roots report `source_count=0` and `not_run/no_sources` without native selection. Invalid empty-input configuration and early capture failures have null result; nonempty qlirgo check/build compiler/loader failures after capture retain a bound failed result and environment/configuration metadata. Child qlinty retains its existing source/tool-bound failure fields; parent lint adds its configuration ID.

For development nonempty check/build/lint, provide a matching native kernel
explicitly; qlirgo does not build, install or download one. Managed and standalone
configuration precedence differ as specified in the [migration contract](docs/linked-qleisli-030.md#source-configuration).

`qlirgo test` still fails with an explicit `backend_unavailable` diagnostic because QLT execution is not implemented. No fallback test runner is used.

## Lint, format, and document Qleisli

```sh
target/release/qlinty /path/to/project --format=json
target/release/qlinty --list-rules --format=json
target/release/qlirgo lint /path/to/project --deny-warnings
target/release/qlifmt /path/to/project --check
target/release/qlirgo fmt /path/to/project
target/release/qlidoc /path/to/project --output=/path/to/documentation
target/release/qlirgo doc --manifest-path=/path/to/project/Qlirgo.toml --document-private-items
```

qlinty freezes original bytes, runs ordinary Qleisli source/IR checks, and then inspects local syntax. Compiler rejection prevents advisory linting. Its three warning rules remain:

| Rule | Group | Diagnostic |
| --- | --- | --- |
| `unused_import` | `idiom` | No possible import use in bodies, meanings, static arguments, or capability requirements |
| `redundant_repeat_one` | `complexity` | `power(op, 1)` in an applied or static-operation position |
| `double_inverse` | `complexity` | `adjoint(adjoint(op))` as a static operation |

Ambiguous shadowing conservatively retains imports. Suggestions are prose. `--deny-warnings` makes a warning-bearing result fail with exit 1; otherwise warnings succeed.

`qlinty --list-rules` exposes an independently versioned catalog with `idiom`, `complexity`, and `resource` groups. All three existing rules have promotion policy `advisory`; the catalog also defines `checker_candidate` and `theorem_candidate` for future rules that might be absorbed by ordinary checking or theorem obligations. The resource group is currently empty. Classification and candidate status make no safety or proof claim, and groups do not enable CLI filtering. Diagnostic IDs join to catalog entries without changing lint result schemas.

qlifmt accepts a `.qli` file or source directory. It preserves syntax tokens, comment content and order, doc-comment attachment, and LF/CRLF while using four-space indentation and a target width of 100 columns. Indivisible tokens and comments may exceed that width. Normal execution writes formatting changes; `--check` leaves sources unchanged, displays a diff, and exits 1 when changes are needed. It parses every input before writing and validates candidates before applying them. Qlirgo additionally uses frozen copies and rejects a source change detected before applying candidates. An I/O failure during application reports files already updated. Formatting configuration and import reordering are deferred.

qlidoc also accepts a `.qli` file or source directory. It generates `index.md` and `modules/<relative-source-path>.md` from module documentation, declaration signatures, and doc comments. Public declarations appear by default; `--document-private-items` includes private declarations. Handwritten qrate docs remain captured inputs. The default standalone output is `target/qlidoc/<source-id>/<tool-id>/<public-or-all>/` under the current directory; qlirgo uses the qrate's `target/qlidoc/<input-id>/<tool-id>/<public-or-all>/`. Complete output directories are installed atomically; identical contents are reused, and conflicting or symlink output fails. Documentation examples are not executed. Future HTML may use qlidoc's own CSS, colors, and layout; HTML generation is deferred.

Formatting and documentation require syntax parsing only, so they accept syntactically valid sources with type or ownership errors. They mark ordinary Qleisli checking as `not_run` with reason `syntax_only` (or `no_sources`). They do not certify types, ownership, contracts, or mathematical correctness.

All four tools support `--help`, `--version`, and `--format=json`. The development parent `qlirgo.result` envelope is version 2 for every command; `qlinty.result`, `qlifmt.result` and `qlidoc.result` remain version 1. Shapes, UTF-8 spans, Unicode scalar coordinates and exit codes 0/1/2 retain their specified meanings. Existing child version-1 transport remains closed; accepting a parent version-2 envelope does not accept a child version 2.

## 0.1.x support and compatibility

Published 0.2.0 was validated on Linux x86_64 and macOS with Rust 1.85 and stable; Linux ARM64, Windows and Android remain outside that release's support. Those results do not validate the linked migration. Source/package and native distribution validation must be repeated before a future release; no product version or new installer is selected here. All Rust packages retain the unsafe-code prohibition.

The public contract covers CLI commands, manifest schema 2, the independently versioned JSON result formats, and exit codes 0/1/2. Path options accept both equality and space-separated syntax, such as `--manifest-path=PATH` and `--manifest-path PATH`, including explicit tool paths and qlidoc's `--output`. Empty, missing, and repeated values are usage errors. Incompatible manifest or result changes require a new schema version independently of the product version. Diagnostic prose and internal Rust library APIs are not stable interfaces. Build records and lint results are metadata, not mathematical evidence; the ordinary Qleisli trust boundary is preserved.

Cargo-style qrate dependency commands such as `qlirgo add` and manifest tables such as `[dependencies]` are still unsupported. Their diagnostics suggest supported local operations and keeping Rust dependencies in external developer Cargo configuration. Qlirgo does not resolve them through Cargo.

QLT execution and HTML generation remain deferred. Qlirgo's own qrate registry, dependency resolution, installation, publication, and host-language build steps also remain deferred; this is independent of distributing the Rust tools through crates.io. See the [changelog](CHANGELOG.md) and [specification](docs/specification.md) for supported behavior and limits.

## Development checks

```sh
cargo fmt --check
cargo test --frozen --all-targets
cargo test --frozen --doc
cargo clippy --frozen --all-targets -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --frozen --no-deps
cargo build --release --frozen --bins
QLEISLI_KERNEL=/absolute/matching/qleisli-kernel python3 scripts/verify_release.py --source-root=. --bin-dir=target/release --qleisli-source-root=/absolute/intended/Qleisli
```

Engine tests generate temporary sources from Rust strings. The source/archive, independent-component, package, native-install and Cargo/Rustdoc-trap checks remain required. Local development results, including explicit dependency/checker scope, are recorded in [naming-validation.json](docs/naming-validation.json); they do not establish registry or release readiness. The developer verifier requires the explicitly intended Qleisli dependency source and its matching, already built native checker; it never resolves or builds either. This source association must come from the recorded developer build, such as its actual Cargo dependency metadata, rather than qlirgo self-report alone. The comparison is not binary-source attestation. Registry availability, official lockfiles, package validation and native installation remain pending; the historical qargo 0.1.9 installer still requires its original Qleisli 0.2.1 pair. Distinguish an explicit local-source experiment from registry-resolved/package validation. See the [release checklist](docs/releasing.md).

## License

Copyright 2026 Masahiko G. Yamada. Apache-2.0; see [LICENSE](LICENSE) and [NOTICE](NOTICE). Dependencies retain their respective licenses.
