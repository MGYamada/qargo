# Qargo

**Exact. Immutable. Bound. Explicit. Independent.**

Qargo captures declared inputs, binds results to those bytes and the actual tools,
and applies only validated effects. The [first principles and architecture](docs/architecture.md)
define these responsibilities and their implementation boundaries.

Local Qleisli qrate management with qlippy, qlifmt, and qlidoc. All four executables and three bundled qrates share version 0.1.6 in this checkout.

**0.1.6 is published and verified.** Install it through [crates.io](https://crates.io/crates/qargo/0.1.6) or the [GitHub source and binary release](https://github.com/MGYamada/qargo/releases/tag/v0.1.6). The download and registry installation examples below use this release. Binary installation requires no Cargo on Linux x86_64 and macOS x86_64/ARM64. Since version 0.1.3, Qargo requires manifest schema 2 and an explicit Qleisli edition; migrate schema-1 manifests as described below.

Linux ARM64 is unsupported in 0.1.6 because the pinned Qleisli 0.2.1 file loader fails during ordinary checking ([issue #18](https://github.com/MGYamada/qargo/issues/18)). No ARM64 Linux binary is distributed; installing from source or crates.io on that architecture does not resolve the checker defect. The installer reports this limitation before downloading or changing files.

**Qargo manages Qleisli packages. Cargo builds, packages, and installs the Rust implementation outside Qargo operations.** Qargo commands never invoke Cargo. They do not translate mathematical tests into Rust tests or Qleisli documentation into Rustdoc.

Each standard qrate ([qlippy](qrates/qlippy/Qargo.toml), [qlifmt](qrates/qlifmt/Qargo.toml), and [qlidoc](qrates/qlidoc/Qargo.toml)) contains its complete Rust engine, CLI, tests, and a short `src/smoke.qli` identity operation, with no `.qlt` files. Shared support belongs to qlippy. Qargo snapshots and identifies both `.rs` and `.qli` inputs, checks the `.qli` source, and leaves Rust compilation to developer Cargo configuration outside the qrates. Each engine also accepts external Qleisli sources. The published `qargo` crate compiles these same engine sources as internal modules; the three private workspace packages remain available for independent development.

A qrate's semantic surface is its Qleisli modules and contracts; its raw input identity also tracks host implementation inputs for provenance. Checking a bundled smoke sample does not certify the Rust engine. The acceptance implementation is the linked Qleisli 0.2.1 checker and its embedded stdlib, regardless of any `qleisli` on PATH. Results record that linked version, profile, and host executable digest. Future qrate-selected acceptance toolchains and checker/stdlib/proof-backend bindings are described in the [toolchain design note](docs/toolchains.md).

The [ecosystem policy](docs/ecosystem-policy.md) puts a curated stdlib at the center. Future extensions require trusted sources, authenticated publisher namespaces, and fixed identities. Public registry and automatic semver resolution are deferred; arbitrary qrate build hooks, native procedural macros, dependency-install scripts, and global feature unification will not be introduced. Cargo's command structure does not determine Qleisli's governance.

The [tooling adoption plan](docs/adoption-plan.md) sets the implementation order for local diagnostic explanations, compatibility comparisons, documentation examples, and scoped review records. Upstream toolchain, structured-repair, and QLT protocols determine their later integration.

The [qleisliup compatibility plan](docs/toolchains.md#qleisliup-compatibility-plan) starts with offline proxy execution of a complete Qargo bundle, then coordinates an exact linked-Qleisli update with the manager's initial production language distribution. Qargo and qlippy currently use their linked Qleisli 0.2.1 checker. That library update can precede external-checker integration, which requires a separate upstream protocol and Qargo contract. Qargo does not read `qleisli-toolchain.toml` or manager state in this release.

## Installation

### Install without Cargo

Install all four executables from the GitHub binary release. Rust and Cargo are unnecessary on the installing machine:

```sh
curl --proto '=https' --tlsv1.2 -fsSL https://github.com/MGYamada/qargo/releases/latest/download/install.sh | sh
```

The installer selects Linux x86_64 or macOS x86_64/ARM64, resolves the latest formal release once, verifies the archive's SHA-256 and inventory, and checks all four executable versions. Linux binaries link statically with musl. macOS binaries have deployment target 11.0 and use system libraries; native CI executes the Intel bundle on macOS 15 and the ARM64 bundle on macOS 14. Earlier macOS versions are not runtime-tested by CI.

The default prefix is `$HOME/.local`, with no sudo or shell configuration changes. If needed, add its bin directory to PATH:

```sh
export PATH="$HOME/.local/bin:$PATH"
qargo --version
```

Run the installer again to update all four tools together. Download the script for inspection, select a version, or choose a different absolute prefix:

```sh
curl --proto '=https' --tlsv1.2 -fsSL https://github.com/MGYamada/qargo/releases/download/v0.1.6/install.sh -o install.sh
sh install.sh --version 0.1.6 --prefix "$HOME/.local"
```

POSIX shell, curl, tar, `sha256sum` or `shasum`, and standard Unix utilities are required. Python/jq are unnecessary. Unsupported environments and missing release assets fail explicitly. Existing unrelated commands are preserved; choose another prefix if its `bin/qargo`, `bin/qlippy`, `bin/qlifmt`, or `bin/qlidoc` already belongs to another installation.

Bundles are stored under `lib/qargo/releases/`; the common `lib/qargo/current` link selects one version for all four command links. Updates preserve older bundles and switch only after validation. An interrupted install can leave a `.install-lock` directory under `lib/qargo`; after confirming that no installer is running, remove that empty lock with `rmdir` and retry.

For manual installation, download the matching `qargo-0.1.6-<target>.tar.gz` and `SHA256SUMS` from the same release. For example, verify and extract the Apple Silicon archive:

```sh
awk '$2 == "qargo-0.1.6-aarch64-apple-darwin.tar.gz"' SHA256SUMS | shasum -a 256 -c -
tar -xzf qargo-0.1.6-aarch64-apple-darwin.tar.gz
export PATH="$PWD/qargo-0.1.6-aarch64-apple-darwin/bin:$PATH"
```

On Linux use `sha256sum -c -` instead. Keep all four executables together. To uninstall an installer-managed default-prefix bundle, remove its four command links and managed directory:

```sh
rm "$HOME/.local/bin/qargo" "$HOME/.local/bin/qlippy" "$HOME/.local/bin/qlifmt" "$HOME/.local/bin/qlidoc"
rm -rf "$HOME/.local/lib/qargo"
```

### Install from crates.io

Rust 1.85 or newer is required. Install all four executables from version 0.1.6 together:

```sh
cargo install qargo --version=0.1.6 --locked --bins
```

Cargo compiles the Rust tools during installation. The installed Qargo executable manages Qleisli qrates without invoking Cargo. The package includes all three engines and requires no private engine crates or repository checkout.

Source builds on macOS also require the Xcode command line developer tools, including Clang/libclang, for the safe running-image identity adapter. Runtime executable identity requirements and change handling are documented in the [executable identity contract](docs/executable-identity.md).

## Build from source

Rust 1.85 or newer is required. The implementation links exactly Qleisli 0.2.1. Clone the repository, or extract a complete source archive, and start in its top-level directory:

```sh
git clone https://github.com/MGYamada/qargo.git
cd qargo
cargo fetch --locked
cargo build --release --frozen --bins
```

This builds `target/release/qargo`, `qlippy`, `qlifmt`, and `qlidoc`. Keep the executables together for default sibling discovery, or supply `--qlippy=PATH`, `--qlifmt=PATH`, or `--qlidoc=PATH`. Build all four from the same product release. An explicitly selected tool is authoritative; a selected tool's failure is returned without falling back to another executable. Cargo fetches Rust dependencies during development; Qargo does not fetch or build engines.

Rebuilding requires the complete repository layout, root `Cargo.toml` and `Cargo.lock`, and the three `rust/<tool>/Cargo.toml` developer manifests. A Qargo snapshot preserves declared qrate inputs but omits this external Cargo configuration; it is not a standalone Rust build package.

Keep `Cargo.lock` in version control so development, CI, Rust 1.85 checks, and `cargo install --locked` use the recorded dependency versions. Build outputs, macOS `.DS_Store` files, and Python caches and bytecode are ignored.

## Manage a bundled qrate

Qargo 0.1.6 requires manifest schema 2 and an explicit Qleisli edition in every `Qargo.toml`. The only supported edition is the string `"2026"`; omission has no default and is an error. A minimal manifest is:

```toml
schema-version = 2
[qrate]
name = "example"
version = "0.1.6"
edition = "2026"
[source]
root = "src"
[tests]
root = "tests"
[docs]
root = "docs"
```

All three root directories must exist, even when empty. Migrate older manifests by changing `schema-version` to 2 and adding `edition = "2026"` under `[qrate]`. Include the edition in every valid manifest, example, and fixture. Rust's `Cargo.toml` files independently retain edition `"2024"`.

```sh
target/release/qargo check --manifest-path=qrates/qlippy/Qargo.toml
target/release/qargo build --manifest-path=qrates/qlippy/Qargo.toml
target/release/qargo lint --manifest-path=qrates/qlippy/Qargo.toml --deny-warnings
target/release/qargo fmt --manifest-path=qrates/qlippy/Qargo.toml --check
target/release/qargo doc --manifest-path=qrates/qlippy/Qargo.toml
```

Use either of the other qrate manifests with the same commands. Qargo operates on one qrate at a time. For `lint` and `fmt`, an explicit standalone source root cannot be combined with `--manifest-path`; use one input mode per command. `check` uses ordinary Qleisli source/IR checking. `build` writes an exact input snapshot, public module index, and build record under the qrate's `target/qargo/`; it does not run document generation. The smoke sources export `identity` and pass checking and linting. Empty source roots succeed with `source_count=0` and a Qleisli check marked `not_run` with reason `no_sources`.

`qargo test` still fails with an explicit `backend_unavailable` diagnostic because QLT execution is not implemented. No fallback test runner is used.

## Lint, format, and document Qleisli

```sh
target/release/qlippy /path/to/project --format=json
target/release/qlippy --list-rules --format=json
target/release/qargo lint /path/to/project --deny-warnings
target/release/qlifmt /path/to/project --check
target/release/qargo fmt /path/to/project
target/release/qlidoc /path/to/project --output=/path/to/documentation
target/release/qargo doc --manifest-path=/path/to/project/Qargo.toml --document-private-items
```

qlippy freezes original bytes, runs ordinary Qleisli source/IR checks, and then inspects local syntax. Compiler rejection prevents advisory linting. Its three warning rules remain:

| Rule | Group | Diagnostic |
| --- | --- | --- |
| `unused_import` | `idiom` | No possible import use in bodies, meanings, static arguments, or capability requirements |
| `redundant_repeat_one` | `complexity` | `repeat_static(1, …)` or `repeat_op(1, …)` |
| `double_inverse` | `complexity` | `inverse_op(inverse_op(…))` |

Ambiguous shadowing conservatively retains imports. Suggestions are prose. `--deny-warnings` makes a warning-bearing result fail with exit 1; otherwise warnings succeed.

`qlippy --list-rules` exposes an independently versioned catalog with `idiom`, `complexity`, and `resource` groups. All three existing rules have promotion policy `advisory`; the catalog also defines `checker_candidate` and `theorem_candidate` for future rules that might be absorbed by ordinary checking or theorem obligations. The resource group is currently empty. Classification and candidate status make no safety or proof claim, and groups do not enable CLI filtering. Diagnostic IDs join to catalog entries without changing lint result schemas.

qlifmt accepts a `.qli` file or source directory. It preserves syntax tokens, comment content and order, doc-comment attachment, and LF/CRLF while using four-space indentation and a target width of 100 columns. Indivisible tokens and comments may exceed that width. Normal execution writes formatting changes; `--check` leaves sources unchanged, displays a diff, and exits 1 when changes are needed. It parses every input before writing and validates candidates before applying them. Qargo additionally uses frozen copies and rejects a source change detected before applying candidates. An I/O failure during application reports files already updated. Formatting configuration and import reordering are deferred.

qlidoc also accepts a `.qli` file or source directory. It generates `index.md` and `modules/<relative-source-path>.md` from module documentation, declaration signatures, and doc comments. Public declarations appear by default; `--document-private-items` includes private declarations. Handwritten qrate docs remain captured inputs. The default standalone output is `target/qlidoc/<source-id>/<tool-id>/<public-or-all>/` under the current directory; Qargo uses the qrate's `target/qlidoc/<input-id>/<tool-id>/<public-or-all>/`. Complete output directories are installed atomically; identical contents are reused, and conflicting or symlink output fails. Documentation examples are not executed. Future HTML may use qlidoc's own CSS, colors, and layout; HTML generation is deferred.

Formatting and documentation require syntax parsing only, so they accept syntactically valid sources with type or ownership errors. They mark ordinary Qleisli checking as `not_run` with reason `syntax_only` (or `no_sources`). They do not certify types, ownership, contracts, or mathematical correctness.

All four tools support `--help`, `--version`, and `--format=json`. Independent version-1 `qargo.result`, `qlippy.result`, `qlifmt.result`, and `qlidoc.result` envelopes preserve original UTF-8 byte spans and Unicode scalar line/column coordinates. Exit codes are 0 for success, 1 for execution/validation failure or required formatting, and 2 for usage errors. Qargo validates child responses and artifacts against captured inputs and the actual executable identity.

## 0.1.x support and compatibility

Linux x86_64 and macOS are validated with Rust 1.85 and stable. Linux ARM64, Windows and Android are outside the supported 0.1.6 release platforms. Distribution from 0.1.1 includes GitHub source archives and the `qargo` crates.io package; version 0.1.5 adds prebuilt bundles for Linux x86_64 and both macOS architectures. All Rust packages forbid unsafe code.

The public contract covers CLI commands, manifest schema 2, the independently versioned JSON result formats, and exit codes 0/1/2. Path options accept both equality and space-separated syntax, such as `--manifest-path=PATH` and `--manifest-path PATH`, including explicit tool paths and qlidoc's `--output`. Empty, missing, and repeated values are usage errors. Incompatible manifest or result changes require a new schema version independently of the product version. Diagnostic prose and internal Rust library APIs are not stable interfaces. Build records and lint results are metadata, not mathematical evidence; the ordinary Qleisli trust boundary is preserved.

Cargo-style qrate dependency commands such as `qargo add` and manifest tables such as `[dependencies]` are still unsupported. Their diagnostics suggest supported local operations and keeping Rust dependencies in external developer Cargo configuration. Qargo does not resolve them through Cargo.

QLT execution and HTML generation remain deferred. Qargo's own qrate registry, dependency resolution, installation, publication, and host-language build steps also remain deferred; this is independent of distributing the Rust tools through crates.io. See the [changelog](CHANGELOG.md) and [specification](docs/specification.md) for supported behavior and limits.

## Development checks

```sh
cargo fmt --check
cargo test --frozen --all-targets
cargo test --frozen --doc
cargo clippy --frozen --all-targets -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --frozen --no-deps
cargo build --release --frozen --bins
python3 scripts/verify_release.py --source-root=. --bin-dir=target/release
```

Engine tests generate temporary sources from Rust strings. Bundled smoke samples exercise mixed Rust/Qleisli snapshots, checking, linting, formatting, documentation, and input identity. CI builds an extracted Git source archive and runs the release verifier on all four OS/toolchain combinations. Cargo and Rustdoc traps prove that runtime operations do not start either developer tool. Python 3 is only required for the verifier. CI also builds the standalone `.crate`, installs its four executables, compares packaged sources with the candidate, and runs the runtime verifier on the installation. See the [release checklist](docs/releasing.md) for publication steps.

## License

Copyright 2026 Masahiko G. Yamada. Apache-2.0; see [LICENSE](LICENSE) and [NOTICE](NOTICE). Dependencies retain their respective licenses.
