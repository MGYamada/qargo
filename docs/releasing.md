# Release checklist

The crates.io package is named `qargo` and installs `qargo`, `qlippy`, `qlifmt`, and `qlidoc` together. The standard bundle and its three qrates share version 0.1.1. Keep Qleisli exactly at 0.2.1, Rust 1.85 as the MSRV, and unsafe code forbidden. Manifest and independent result schema versions remain 1.

Cargo publication and GitHub source releases are separate actions. Obtain explicit authorization for the requested publication channel; updating versions or publication configuration does not authorize uploading, tagging, or creating a GitHub release.

## Package layout

The public `qargo` package compiles all three engines from their canonical `qrates/<tool>/src/` files as internal Rust modules. Its normalized manifest has registry dependencies only. All four CLI targets, engine sources, tests, smoke samples, handwritten qrate documentation, README, lockfile, license, and notices are included in the `.crate`. Generated output and caches are excluded.

The internal library name remains `qargo_tools`; it is not an executable or the crates.io package name. Private workspace packages under `rust/<tool>/` remain unpublished and support independent engine development. Their external source paths are not intended to form individual distributable crates. A qrate snapshot alone omits the developer Cargo configuration and is not a standalone Rust build package.

## Prepare and validate

1. Check the crates.io `qargo` namespace, publisher ownership, and whether the intended version already exists. Never replace an existing version.
2. Update the changelog with the release date and make the README's version and installation instructions match the intended package. Commit the complete candidate, including Cargo configuration and lockfile, all three qrates, documentation, CI, verifiers, license, and notices. Exclude generated `target/` directories and caches.
3. Run formatting, all Rust targets, Rust doctests, Clippy with warnings denied, and Rustdoc with warnings denied. These are Cargo operations outside Qargo commands.
4. Require Linux/macOS × Rust 1.85/stable CI for that candidate. Each combination builds an extracted Git source archive and the standalone `.crate`, installs the four packaged executables, checks package inventory against candidate sources, and runs the runtime verifier. Use that exact verified commit for publication.

After dependency preparation, the source checks include:

```sh
cargo fetch --locked
cargo build --release --frozen --bins
python3 scripts/verify_release.py --source-root=. --bin-dir=target/release
```

The Git archive preserves root Cargo configuration, all three private development manifests, and the complete qrates. The source verifier checks product versions and actual executable identities, exact qrate snapshots, smoke source checking, sibling lint/format/doc discovery, stable repeated builds, canonical formatting, deterministic Markdown, and unavailable QLT. Cargo and Rustdoc PATH traps verify that runtime commands do not start either implementation tool.

On the clean candidate, validate crates.io packaging without skipping its build verification:

```sh
cargo package --list -p qargo
cargo package --locked -p qargo
mkdir -p /path/to/temporary-package
tar -xzf target/package/qargo-0.1.1.crate -C /path/to/temporary-package
cargo install --locked --path /path/to/temporary-package/qargo-0.1.1 --bins --root /path/to/temporary-install
python3 scripts/verify_package.py --source-root=. --crate=target/package/qargo-0.1.1.crate --bin-dir=/path/to/temporary-install/bin
cargo publish --dry-run --locked -p qargo --registry crates-io
```

[Cargo package verification](https://doc.rust-lang.org/cargo/commands/cargo-package.html) builds the extracted package. The package verifier compares the archived source and qrate bytes with the candidate, checks the normalized manifest's registry-only dependencies and four CLI targets, and validates the installed bundle through the runtime verifier. Extract outside the candidate workspace so Cargo cannot discover its development manifests. Installation must work without private workspace members or the original checkout. The dry run must succeed before publication; `--no-verify` does not replace these checks.

## Publish crates.io when requested

Recheck the package/version and publish the verified candidate:

```sh
cargo publish --locked -p qargo --registry crates-io
```

[Published crate versions cannot be replaced](https://doc.rust-lang.org/cargo/reference/publishing.html). Wait for registry availability and verify a fresh installation:

```sh
cargo install qargo --version=0.1.1 --locked --bins --root /path/to/fresh-install
```

Check all four version commands and run the package/runtime verifiers against that registry installation. All executables must report 0.1.1, and Qargo must discover its installed siblings. Record the publication result and candidate commit.

## Publish a GitHub source release when requested

This step requires authorization for the GitHub release channel. Check the remote `v0.1.1` tag and release before creating either; do not force-push or replace existing release data.

Create and push an annotated `v0.1.1` tag for the verified candidate and create a GitHub prerelease titled `Qargo v0.1.1`, using the changelog entry as release notes. GitHub supplies source archives; do not attach prebuilt Rust executable assets. Confirm that the tag and source archive identify the verified candidate and include all three complete qrates without generated outputs.
