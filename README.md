# Qargo and qlippy

Local Qleisli qrate management and a Rust lint engine. Version 0.1.0 is an experimental GitHub source release for Linux and macOS.

**Qargo manages Qleisli packages. Cargo builds the Rust implementation during development.** Qargo commands never invoke Cargo. They do not translate mathematical tests into Rust tests or Qleisli documentation into Rustdoc.

The [qlippy qrate](qrates/qlippy/Qargo.toml) includes its complete Rust engine, CLI, shared support and Rust test sources, plus a short [Qleisli identity operation](qrates/qlippy/src/smoke.qli) that exercises qrate management. It contains zero `.qlt` files. Qargo snapshots and identifies both `.rs` and `.qli` inputs, checks the `.qli` source, and leaves Rust compilation to developer Cargo configuration outside the qrate. The engine can also lint external Qleisli source roots.

## Install from source

Rust 1.85 or newer is required. The implementation links exactly Qleisli 0.2.1. Clone the release tag, or extract the complete GitHub source archive and start in its top-level directory:

```sh
git clone --branch v0.1.0 https://github.com/MGYamada/qargo.git
cd qargo
cargo fetch --locked
cargo build --release --frozen --bins
```

These commands build `target/release/qargo` and `target/release/qlippy`. Keep the executables together so Qargo can find qlippy beside it, or supply `--qlippy=PATH`. Build both from the same release. Cargo fetches Rust dependencies during this developer operation; Qargo commands do not fetch or build the Rust engines.

The complete repository layout, root `Cargo.toml` and `Cargo.lock`, and `rust/qlippy/Cargo.toml` are required to rebuild the executables. A Qargo qrate snapshot preserves declared inputs, including `.rs` files, but omits that external developer configuration. It does not by itself constitute a Rust build package.

## Manage the initial qrate

```sh
target/release/qargo check --manifest-path=qrates/qlippy/Qargo.toml
target/release/qargo build --manifest-path=qrates/qlippy/Qargo.toml
target/release/qargo lint --manifest-path=qrates/qlippy/Qargo.toml --deny-warnings
```

`check` validates and checks available Qleisli sources. `build` writes an exact input snapshot, public module index and build record under the qrate's `target/qargo/`. The sample qrate reports `source_count=1` and `qleisli_check.status="passed"`, with public module `smoke` exporting `identity`; linting emits no warnings. Empty source roots remain supported with `source_count=0` and `qleisli_check.status="not_run"`.

`qargo test` requires QLT and `qargo doc` requires qlidoc. Both currently fail with an explicit `backend_unavailable` diagnostic. Neither tool is implemented by this project, and no fallback runner/renderer is used.

## Lint external source

For an existing directory of `.qli` files:

```sh
target/release/qlippy /path/to/project --format=json
target/release/qargo lint /path/to/project --format=json
target/release/qargo lint /path/to/project --qlippy=/path/to/qlippy --deny-warnings
```

qlippy freezes the original bytes, runs ordinary Qleisli source/IR checks, and then inspects local syntax. Compiler rejection prevents advisory linting. The initial warnings are:

| Rule | Diagnostic |
| --- | --- |
| `unused_import` | No possible import use in bodies, meanings, static arguments or capability requirements |
| `redundant_repeat_one` | `repeat_static(1, …)` or `repeat_op(1, …)` |
| `double_inverse` | `inverse_op(inverse_op(…))` |

Ambiguous shadowing conservatively retains imports. Suggestions are explanatory text; files are never automatically changed. `--deny-warnings` turns warning-bearing results into exit 1. Otherwise warnings succeed. Invalid CLI usage exits 2, other failures exit 1.

Both tools accept `--format=json` and `--help`; `--version` reports the implementation and linked compiler profile. JSON results are separate version-1 `qargo.result` and `qlippy.result` formats. Source locations retain UTF-8 byte spans and Unicode scalar line/column coordinates. Qargo validates child results against the captured input rather than trusting a claimed source identity.

Input IDs omit absolute paths and timestamps and retain original file bytes. Build records and lint success are metadata, not proof of mathematical correctness. The ordinary Qleisli trust boundary is preserved.

## v0.1.0 support and compatibility

Linux and macOS are validated with Rust 1.85 and stable. Windows and Android are outside this release's supported platforms. GitHub distributes source only; prebuilt executables and crates.io publication are not provided. Both developer Rust packages retain `publish = false`.

The public v0.1.0 contract covers the CLI, manifest schema 1, independent JSON result version 1 formats, and exit codes 0/1/2. Path options use `--manifest-path=PATH` and `--qlippy=PATH`. Keep these documented interfaces compatible within 0.1.x; incompatible manifest/result changes require a new schema version. Diagnostic prose and the unpublished Rust library API are not stable interfaces.

QLT/qlidoc execution, registry access, dependency resolution, installation, publication, automatic fixes and host-language build steps remain outside this release. See the [changelog](CHANGELOG.md) and [specification](docs/specification.md) for the supported behavior and limits.

## Development checks

```sh
cargo fmt --check
cargo test --frozen --all-targets
cargo test --frozen --doc
cargo clippy --frozen --all-targets -- -D warnings
cargo build --release --frozen --bins
python3 scripts/verify_release.py --source-root=. --bin-dir=target/release
```

Lint-rule tests generate temporary source files from Rust strings. The tracked `smoke.qli` sample exercises a qrate containing both Rust and Qleisli inputs, including source checking, linting, module indexing and snapshot identity. CI also builds an extracted Git source archive and runs the release smoke verifier on all four OS/toolchain combinations. Python 3 is only needed for this developer verification script. See the [release checklist](docs/releasing.md) for publication steps.

## License

Copyright 2026 Masahiko G. Yamada. Apache-2.0; see [LICENSE](LICENSE) and [NOTICE](NOTICE). Dependencies retain their respective licenses.
