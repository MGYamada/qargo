# Release checklist

The crates.io package is named `qargo` and installs `qargo`, `qlippy`, `qlifmt`, and `qlidoc` together. The standard bundle and its three qrates share version 0.1.6. Keep Qleisli exactly at 0.2.1, Rust 1.85 as the MSRV, and unsafe code forbidden. Manifest schema 2 requires an explicit `edition = "2026"` under `[qrate]`; independent result schema versions remain 1. Keep every valid manifest, example, and fixture explicit about its edition. Cargo manifests retain Rust edition `"2024"`.

Cargo publication and GitHub source/binary releases are separate actions. Obtain explicit authorization for the requested publication channel; updating versions or publication configuration does not authorize uploading, tagging, or creating a GitHub release. Do not advertise a candidate's downloads as available until publication succeeds.

Version 0.1.5 was published on 2026-10-01 to [crates.io](https://crates.io/crates/qargo/0.1.5) and [GitHub](https://github.com/MGYamada/qargo/releases/tag/v0.1.5) from verified commit `f9b562474351327e1b72781e30bdb19d5621d5b4`. Its release assets contain the three supported native bundles, `install.sh`, and `SHA256SUMS`; Linux ARM64 remains deferred under issue #18. Subsequent documentation and verification-workflow corrections retain that published package and tag identity.

Version 0.1.6 is an unpublished candidate. Commands below prepare or publish that candidate only at the corresponding authorized step; public installation examples in README continue to use 0.1.5 until publication is verified.

## Package layout

The public `qargo` package compiles all three engines from their canonical `qrates/<tool>/src/` files as internal Rust modules. Its normalized manifest has registry dependencies only. All four CLI targets, engine sources, tests, smoke samples, handwritten qrate documentation, README, lockfile, license, and notices are included in the `.crate`. Generated output and caches are excluded.

The internal library name remains `qargo_tools`; it is not an executable or the crates.io package name. Private workspace packages under `rust/<tool>/` remain unpublished and support independent engine development. Their external source paths are not intended to form individual distributable crates. A qrate snapshot alone omits the developer Cargo configuration and is not a standalone Rust build package.

## Prepare and validate

1. Check the crates.io `qargo` namespace, publisher ownership, and whether the intended version already exists. Never replace an existing version.
2. Update the changelog with the release date and make the README's version and installation instructions match the intended package. Commit the complete candidate, including Cargo configuration and lockfile, all three qrates, documentation, CI, verifiers, license, and notices. Exclude generated `target/` directories and caches.
3. Run formatting, all Rust targets, Rust doctests, Clippy with warnings denied, and Rustdoc with warnings denied. These are Cargo operations outside Qargo commands.
4. Require Linux/macOS × Rust 1.85/stable CI for that candidate. Each combination builds an extracted Git source archive and the standalone `.crate`, installs the four packaged executables, checks package inventory against candidate sources, and runs the runtime verifier. Also require all three supported native binary distribution jobs and the complete asset-collection job. Linux ARM64 remains excluded from the supported release scope because pinned Qleisli 0.2.1 fails ordinary checking (issue #18); do not publish an ARM64 Linux asset. Use that exact verified commit for publication.

After dependency preparation, the source checks include:

```sh
cargo fetch --locked
cargo build --release --frozen --bins
python3 scripts/verify_release.py --source-root=. --bin-dir=target/release
```

The Git archive preserves root Cargo configuration, all three private development manifests, the complete qrates, and the ecosystem/toolchain policy documents. The source verifier checks product versions and actual executable identities, the advisory rule catalog, both path-option forms, exact qrate snapshots, smoke source checking, sibling lint/format/doc discovery, stable repeated builds, canonical formatting, deterministic Markdown, and unavailable QLT. Cargo and Rustdoc PATH traps verify that runtime commands do not start either implementation tool.

On the clean candidate, validate crates.io packaging without skipping its build verification:

```sh
cargo package --list -p qargo
cargo package --locked -p qargo
mkdir -p /path/to/temporary-package
tar -xzf target/package/qargo-0.1.6.crate -C /path/to/temporary-package
cargo install --locked --path /path/to/temporary-package/qargo-0.1.6 --bins --root /path/to/temporary-install
python3 scripts/verify_package.py --source-root=. --crate=target/package/qargo-0.1.6.crate --bin-dir=/path/to/temporary-install/bin
cargo publish --dry-run --locked -p qargo --registry crates-io
```

[Cargo package verification](https://doc.rust-lang.org/cargo/commands/cargo-package.html) builds the extracted package. The package verifier compares the archived source and qrate bytes with the candidate, checks the normalized manifest's registry-only dependencies and four CLI targets, and validates the installed bundle through the runtime verifier. Extract outside the candidate workspace so Cargo cannot discover its development manifests. Installation must work without private workspace members or the original checkout. The dry run must succeed before publication; `--no-verify` does not replace these checks.

## Prepare binary assets without publication

The `Cargo-free binary distribution` workflow builds native Rust 1.85.0 bundles for Linux x86_64 on Ubuntu 24.04 and macOS x86_64/ARM64 on macOS 15/14 respectively. Linux installs the distribution's musl-tools and links statically; macOS sets `MACOSX_DEPLOYMENT_TARGET=11.0`. The verifier checks executable architecture, rejects ELF dynamic loaders/libraries, and checks Mach-O deployment metadata and system-only library dependencies. The CI summary records the exact candidate commit, tested OS, and compiler. macOS 11.0 is a deployment target, not a CI-tested host.

For a native macOS ARM64 candidate, the equivalent local preparation is:

```sh
cargo +1.85.0 fetch --locked
MACOSX_DEPLOYMENT_TARGET=11.0 cargo +1.85.0 build --release --frozen --bins --target=aarch64-apple-darwin
python3 scripts/test_install.py
python3 scripts/distribution.py build --source-root=. --bin-dir=target/aarch64-apple-darwin/release --target=aarch64-apple-darwin --output-dir=target/binary-assets
python3 scripts/distribution.py verify --source-root=. --archive=target/binary-assets/qargo-0.1.6-aarch64-apple-darwin.tar.gz --target=aarch64-apple-darwin
```

Use an empty output directory; archive generation and collection refuse to replace files. The build verifies the candidate's runtime behavior before archiving. The archive verifier repeats runtime checks on extracted binaries and performs a real Cargo-free installation with local responses for the production HTTPS URLs, followed by installed-bundle runtime checks. Source, binary, and crate verification remain independent. No verification calls a publication endpoint.

After all three supported native jobs pass, the collection job validates the complete archive inventories and produces `qargo-0.1.6-release-assets`, containing exactly the three `.tar.gz` files, `install.sh`, and `SHA256SUMS`. This is an Actions artifact, not a public GitHub release. Download this artifact from the exact verified candidate workflow run. To collect already downloaded per-target artifacts locally:

```sh
python3 scripts/distribution.py collect --source-root=. --artifact-dir=/path/to/native-artifacts --output-dir=/path/to/empty-release-assets
```

The collector requires one archive for each supported target and checks license/notice bytes and platform linkage against the candidate. It hashes the three archives and the candidate installer into a sorted `SHA256SUMS` file. Confirm all source/package and native-distribution jobs refer to the same candidate commit before publication; do not rebuild different binaries after verification.

## Publish crates.io when requested

Recheck the package/version and publish the verified candidate:

```sh
cargo publish --locked -p qargo --registry crates-io
```

[Published crate versions cannot be replaced](https://doc.rust-lang.org/cargo/reference/publishing.html). Wait for registry availability and verify a fresh installation:

```sh
cargo install qargo --version=0.1.6 --locked --bins --root /path/to/fresh-install
```

Check all four version commands and run the package/runtime verifiers against that registry installation. All executables must report 0.1.6, and Qargo must discover its installed siblings. Record the publication result and candidate commit.

## Publish a GitHub source and binary release when requested

This step requires authorization for the GitHub release channel. Check the remote `v0.1.6` tag and release before creating either; do not force-push or replace existing release data.

Create and push an annotated `v0.1.6` tag for the verified candidate and create a formal GitHub release titled `Qargo v0.1.6`, using the changelog entry as release notes. Attach all five files from that candidate's verified `qargo-0.1.6-release-assets` artifact at creation, then publish with `--verify-tag --latest --prerelease=false --draft=false`. Never expose a formal/latest release with only some supported target assets, or overwrite an existing tag/release/asset. GitHub also supplies source archives. Confirm that the tag and source archive identify the verified candidate and include all three complete qrates without generated outputs. When the same version is also published on crates.io, retain that exact candidate commit for the tag; subsequent documentation updates must not move the tag or replace the package.

After publication, run the `Verify published release` workflow with version `0.1.6`. It re-downloads every release asset, checks its recorded SHA-256 against the tagged candidate, and repeats installer and installed-runtime verification on all three supported native environments using the actual release URLs, including default latest resolution and explicit version selection. Remove the README's unpublished-candidate notice only after both requested publication channels have been verified, and link the published package/release without moving their identities.

Reflect the release on the default branch through a pull request when repository rules require one. Preserve the verified candidate commits in the merge, satisfy all required checks, and update the README to link the published release and package.
