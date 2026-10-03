# Independent Rust development for standard qrates

Each `rust/<tool>/` directory is its own Cargo workspace, with a literal package
manifest, lockfile and Rust 1.85 toolchain declaration. Its engine, CLI, tests and
Qleisli sample live in `qrates/<tool>/`. Run these commands from that development
directory, including after extraction:

```sh
cargo fetch --locked
cargo build --frozen --lib --bins
cargo test --frozen --all-targets
cargo test --frozen --doc
cargo clippy --frozen --all-targets -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --frozen --no-deps
```

Qlifmt and qlidoc have an exact local Rust dependency on qlippy's support. They
import `qlippy_engine::support` without depending on lint rules, product defaults
or the ordinary checker. All three packages remain unpublished. The root bundle
uses alias/include adapters for the same canonical CLI and test files.

## Preparing an extraction

Python 3.11 or newer is required only for developer verification. First prepare
the chosen component's locked registry dependencies with `cargo fetch --locked
--manifest-path rust/<tool>/Cargo.toml`. The extraction command itself uses
offline, locked vendoring:

```sh
python3 scripts/qrate_project.py --component qlifmt --destination /tmp/qlifmt-project
```

The destination must not exist. It receives the component's `qrates/` and `rust/`
trees, the corresponding qlippy trees when required, all locked registry sources
and checksums in `vendor/`, license notices, the boundary policy, these
instructions, and the standalone verification scripts. There is no root Qargo
Cargo manifest. Relative source/dependency relationships are preserved. This
does not create a repository or publish anything.

`project-record.json` records a versioned, sorted inventory of file content
digests, sizes, executable modes, directories, and optional configuration
presence. Its framed SHA-256 identity excludes its own record, checkout paths,
timestamps, caches, Git state and build outputs. Qrate inputs and this complete
Rust project are separate artifacts. No Qargo result schema is extended.

The initial extraction supports the current explicit crates.io and local qlippy
dependency graph. Git dependencies and custom Cargo configuration fail until
their capture contract is implemented. Supplied helpers/generated sources in
the component's development or qrate trees enter the inventory. Project links,
special files, escaped local references, and inherited dependencies are rejected.

## Verifying without the original checkout

Supply a resolved Rust installation directory (the result of `rustc --print
sysroot`, not a rustup shim directory), with Cargo, Clippy, Rustfmt and Rustdoc,
and an installed four-executable Qargo bundle. Neither may require the original
checkout. Then run the copied verifier:

```sh
python3 /tmp/qlifmt-project/scripts/verify_qrate.py \
  --project /tmp/qlifmt-project \
  --toolchain /absolute/resolved/rust-toolchain \
  --bundle-bin /absolute/installed/bundle/bin \
  --output /tmp/qlifmt-verification
```

The output directory must not exist and must be outside the captured project.
Verification uses fresh Cargo/Rustup homes, a fresh target directory, an explicit
environment and frozen vendored dependencies. It rejects ancestor Cargo
configuration, custom compiler wrappers/configuration, and metadata paths outside
the extracted closure. It builds the library/CLI, runs engine and snapshot tests,
doctests, Clippy and Rustdoc, compares direct and embedded CLI behavior, and uses
the supplied Qargo to validate original/relocated qrate input and source IDs.

On macOS the Cargo commands run in a dedicated Seatbelt sandbox that permits
reads from captured inputs and writes only to fresh build/test state. On Linux,
`strace` is required to audit successful opens and executable launches against
the admitted paths. Directory-only descriptors permit path traversal; subsequent
file-content opens remain audited. Failed executable probes supply no executable
bytes, and interleaved syscall entries/returns are matched by process identity.
Unrecorded native reads fail verification. CI also makes the
original checkout unreadable while running the copied verifier.

Native build records bind the actual Rust installation, compiler/helper binaries,
native SDK/header/library inventories, selected commands, admitted environment,
each stage's dependency build outputs and Rust dep-info, and separate executable
digests. Linux records also capture Cargo's system CA and Git configuration inputs,
transparent-hugepage setting and applicable cgroup CPU limits. Fresh homes and
explicit Git configuration variables isolate user configuration; Cargo's linked
Git library can still inspect the captured system file. They are developer
observations, not publisher authentication,
mathematical evidence or promises of reproducible binary bytes. Supported native
verification hosts are Linux x86_64 with system GCC and macOS with Command Line
Tools; custom native toolchains and cross builds require separate capture rules.
The copied toolchain declaration remains Rust 1.85; an explicitly supplied stable
installation exercises compatibility without changing that project identity.

For development in the original checkout, `python3 scripts/qrate_project.py`
audits component dependency direction and checks that root CLI/test adapters
remain wiring only. Release validation compares literal standard-qrate and Rust
versions to root Cargo `[package].version`. Qargo commands do not run these
developer scripts or Cargo.
