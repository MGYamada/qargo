# Release checklist

The crates.io package is named `qargo` and installs `qargo`, `qlippy`, `qlifmt`, and `qlidoc` together. The standard bundle and its three qrates share version 0.1.8 for distribution convenience under [QRATEBOUNDARY.md](../QRATEBOUNDARY.md); the tools remain ordinary independent qrates. Keep Qleisli exactly at 0.2.1, Rust 1.85 as the MSRV, and unsafe code forbidden. Manifest schema 2 requires an explicit `edition = "2026"` under `[qrate]`; independent result schema versions remain 1. Keep every valid manifest, example, and fixture explicit about its edition. Cargo manifests retain Rust edition `"2024"`.

**0.1.8 is an unpublished candidate.** It requires all release gates below for the exact candidate commit before publication. The records below describe earlier published releases.

**0.1.7 is published and verified.** It was published on 2026-10-03 (UTC) to [crates.io](https://crates.io/crates/qargo/0.1.7) and [GitHub](https://github.com/MGYamada/qargo/releases/tag/v0.1.7) from verified commit `cb9646a281fce4c6b5388c3e08f7ed3785922482`. The annotated tag and published crate retain that commit; subsequent workflow scheduling and documentation updates must not move or replace either.

The [source/package and full independent-extraction matrix](https://github.com/MGYamada/qargo/actions/runs/37131993827), [native distribution and complete asset collection](https://github.com/MGYamada/qargo/actions/runs/37131993854), and [published-release verification](https://github.com/MGYamada/qargo/actions/runs/37137055113) all passed. The independent matrix covered all three components on Linux/macOS with Rust 1.85 and stable before its scheduling was separated from routine CI. It remains a mandatory manual gate for future releases.

All three native archives, `install.sh`, and `SHA256SUMS` were attached before the GitHub release became formal/latest. Real versioned/latest installations passed on Linux x86_64 and macOS x86_64/ARM64. A fresh crates.io installation passed package/runtime verification, and the registry archive matched the dry-run package byte-for-byte (SHA-256 `b5ea8100cd28648822837e3248477c09579d931d0e5ff26d0fb58be287299cb1`). The GitHub source archive matched all 101 tracked files at the tagged commit.

Cargo publication and GitHub source/binary releases are separate actions. Obtain explicit authorization for the requested publication channel; updating versions or publication configuration does not authorize uploading, tagging, or creating a GitHub release. Do not advertise a candidate's downloads as available until publication succeeds.

Version 0.1.5 was published on 2026-10-01 to [crates.io](https://crates.io/crates/qargo/0.1.5) and [GitHub](https://github.com/MGYamada/qargo/releases/tag/v0.1.5) from verified commit `f9b562474351327e1b72781e30bdb19d5621d5b4`. Its release assets contain the three supported native bundles, `install.sh`, and `SHA256SUMS`; Linux ARM64 remains deferred under issue #18. Subsequent documentation and verification-workflow corrections retain that published package and tag identity.

Version 0.1.6 was published on 2026-10-03 to [crates.io](https://crates.io/crates/qargo/0.1.6) and [GitHub](https://github.com/MGYamada/qargo/releases/tag/v0.1.6) from verified commit `1c759a097c58f8fbd280edcaf8efac6d380ec1e9`. The annotated tag and published crate retain that commit; later documentation updates must not move or replace either. All three native archives, `install.sh`, and `SHA256SUMS` were attached before the GitHub release became formal/latest.

The [source/package matrix](https://github.com/MGYamada/qargo/actions/runs/37080281022), [native distribution and complete asset collection](https://github.com/MGYamada/qargo/actions/runs/37080280990), and [published-release verification](https://github.com/MGYamada/qargo/actions/runs/37081714657) all passed for that commit. Real versioned/latest installations passed on Linux x86_64 and macOS x86_64/ARM64. A fresh crates.io installation passed package/runtime verification; its archive matched the dry-run package byte-for-byte (SHA-256 `e985680e072d23f0402bad9ed133429a05a1a81940ec3ab277b0ae7af2a04ac0`). The GitHub source archive matched all 80 tracked files at the tagged commit.

## Package layout

The public `qargo` package compiles all three engines from their canonical `qrates/<tool>/src/` files as internal Rust modules. Its normalized manifest has registry dependencies only. All four CLI targets, engine sources, tests, smoke samples, handwritten qrate documentation, README, lockfile, license, and notices are included in the `.crate`. Generated output and caches are excluded.

The internal library name remains `qargo_tools`; it is not an executable or the crates.io package name. Independent Cargo workspaces under `rust/<tool>/` remain unpublished and support independent engine development. Their manifests, lockfiles, toolchain declarations and canonical qrate trees are transferred together for independent development, with a captured qlippy support dependency when needed. They are not separately published crates. A qrate snapshot alone omits the developer Cargo configuration and is not a standalone Rust build package.

## Prepare and validate

1. Check the crates.io `qargo` namespace, publisher ownership, and whether the intended version already exists. Never replace an existing version.
2. Update the changelog with the release date and make the README's version and installation instructions match the intended package. Commit the complete candidate, including Cargo configuration and lockfile, all three qrates, documentation, CI, verifiers, license, and notices. Exclude generated `target/` directories and caches.
3. Run formatting, all Rust targets, Rust doctests, Clippy with warnings denied, and Rustdoc with warnings denied. Run `scripts/test_qrate_project.py` and `scripts/qrate_project.py` for identity, release-version and architectural checks. Root workspace checks do not cover the independent component workspaces; require the [extraction checks](qrate-development.md) for all three. These are developer operations outside Qargo commands.
4. Require Linux/macOS × Rust 1.85/stable bundle CI for that candidate. Before every release, manually dispatch `Verify independent qrate extraction` (`qrate-extraction.yml`) for the exact candidate and require all three components on both OSes and both Rust toolchains to pass. This full extraction matrix is a release gate, not a push/PR trigger; lightweight boundary audits do not replace it. It verifies each extracted component while the original checkout is unreadable. Record its candidate commit and run URL. The bundle matrix builds an extracted Git source archive and the standalone `.crate`, installs the four packaged executables, checks package inventory against candidate sources, and runs the runtime verifier. Also require all three supported native binary distribution jobs and the complete asset-collection job. Linux ARM64 remains excluded from the supported release scope because pinned Qleisli 0.2.1 fails ordinary checking (issue #18); do not publish an ARM64 Linux asset. Use that exact verified commit for publication.

After dependency preparation, the source checks include:

```sh
cargo fetch --locked
cargo build --release --frozen --bins
python3 scripts/verify_release.py --source-root=. --bin-dir=target/release
```

The Git archive preserves root Cargo configuration, all three private development manifests, lockfiles and toolchain declarations, the complete qrates, QRATEBOUNDARY.md, the extraction plan, and the ecosystem/toolchain policy documents. The crate also includes the boundary policy and extraction plan. The source verifier checks product versions and actual executable identities, the advisory rule catalog, both path-option forms, exact qrate snapshots, smoke source checking, sibling lint/format/doc discovery, stable repeated builds, canonical formatting, deterministic Markdown, and unavailable QLT. Cargo and Rustdoc PATH traps verify that runtime commands do not start either implementation tool.

On the clean candidate, validate crates.io packaging without skipping its build verification:

```sh
cargo package --list -p qargo
cargo package --locked -p qargo
mkdir -p /path/to/temporary-package
tar -xzf target/package/qargo-0.1.8.crate -C /path/to/temporary-package
cargo install --locked --path /path/to/temporary-package/qargo-0.1.8 --bins --root /path/to/temporary-install
python3 scripts/verify_package.py --source-root=. --crate=target/package/qargo-0.1.8.crate --bin-dir=/path/to/temporary-install/bin
cargo publish --dry-run --locked -p qargo --registry crates-io
```

[Cargo package verification](https://doc.rust-lang.org/cargo/commands/cargo-package.html) builds the extracted package. The package verifier compares the archived source and qrate bytes with the candidate, checks the normalized manifest's registry-only dependencies and four CLI targets, and validates the installed bundle through the runtime verifier. Extract outside the candidate workspace so Cargo cannot discover its development manifests. Installation must work without private workspace members or the original checkout. The dry run must succeed before publication; `--no-verify` does not replace these checks.

## Prepare binary assets without publication

The `Cargo-free binary distribution` workflow builds native Rust 1.85.0 bundles for Linux x86_64 on Ubuntu 24.04 and macOS x86_64/ARM64 on macOS 15/14 respectively. Linux installs the distribution's musl-tools and links statically; macOS sets `MACOSX_DEPLOYMENT_TARGET=11.0`. The verifier checks executable architecture, rejects ELF dynamic loaders/libraries, and checks Mach-O deployment metadata and system-only library dependencies. The CI summary records the exact candidate commit, tested OS, and compiler. macOS 11.0 is a deployment target, not a CI-tested host.

The workflow runs for every pull request, pushes to `main` and `v*` tags, and manual dispatches. Feature-branch pushes do not start a duplicate native build. Configure the default branch's required status checks to include `Native distribution verified`, with GitHub Actions as its source, alongside the four bundle OS/toolchain checks. This final job always evaluates both the native build matrix and complete asset collection, and fails if either failed, was cancelled, or was skipped. Register it after its first successful run; the workflow name itself is not the status-check name. The manually dispatched extraction and published-release workflows are separate release requirements, not PR status checks.

For a native macOS ARM64 candidate, the equivalent local preparation is:

```sh
cargo +1.85.0 fetch --locked
MACOSX_DEPLOYMENT_TARGET=11.0 cargo +1.85.0 build --release --frozen --bins --target=aarch64-apple-darwin
python3 scripts/test_install.py
python3 scripts/distribution.py build --source-root=. --bin-dir=target/aarch64-apple-darwin/release --target=aarch64-apple-darwin --output-dir=target/binary-assets
python3 scripts/distribution.py verify --source-root=. --archive=target/binary-assets/qargo-0.1.8-aarch64-apple-darwin.tar.gz --target=aarch64-apple-darwin
```

Use an empty output directory; archive generation and collection refuse to replace files. The build verifies the candidate's runtime behavior before archiving. The archive verifier repeats runtime checks on extracted binaries and performs a real Cargo-free installation with local responses for the production HTTPS URLs, followed by installed-bundle runtime checks. Source, binary, and crate verification remain independent. No verification calls a publication endpoint.

After all three supported native jobs pass, the collection job validates the complete archive inventories and produces `qargo-0.1.8-release-assets`, containing exactly the three `.tar.gz` files, `install.sh`, and `SHA256SUMS`. This is an Actions artifact, not a public GitHub release. Download this artifact from the exact verified candidate workflow run. To collect already downloaded per-target artifacts locally:

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
cargo install qargo --version=0.1.8 --locked --bins --root /path/to/fresh-install
```

Check all four version commands and run the package/runtime verifiers against that registry installation. All executables must report 0.1.8, and Qargo must discover its installed siblings. Record the publication result and candidate commit.

## Publish a GitHub source and binary release when requested

The published-release workflow first uses its own revision's inventory verifier
with the requested tag version. It requires exactly the three supported binary
archives, `install.sh`, and `SHA256SUMS` at the download root, with no extra files,
nested directories, or symlinks. The tagged candidate's archive, checksum,
installer, and runtime verifiers then run as before. Keeping the inventory
verifier separate allows this check to cover older tags that predate it.
Local inventory-only inspection uses:

```sh
python3 scripts/distribution.py verify-published-assets --artifact-dir=/path/to/downloaded-assets --version=0.1.8
```

This inventory check does not replace content/checksum or runtime validation.
The `collect` command continues to accept nested Actions artifact directories.

This step requires authorization for the GitHub release channel. Check the remote `v0.1.8` tag and release before creating either; do not force-push or replace existing release data.

Create and push an annotated `v0.1.8` tag for the verified candidate and create a formal GitHub release titled `Qargo v0.1.8`, using the changelog entry as release notes. Attach all five files from that candidate's verified `qargo-0.1.8-release-assets` artifact at creation, then publish with `--verify-tag --latest --prerelease=false --draft=false`. Never expose a formal/latest release with only some supported target assets, or overwrite an existing tag/release/asset. GitHub also supplies source archives. Confirm that the tag and source archive identify the verified candidate and include all three complete qrates without generated outputs. When the same version is also published on crates.io, retain that exact candidate commit for the tag; subsequent documentation updates must not move the tag or replace the package.

After publication, run the `Verify published release` workflow with version `0.1.8`. It re-downloads every release asset, checks its recorded SHA-256 against the tagged candidate, and repeats installer and installed-runtime verification on all three supported native environments using the actual release URLs, including default latest resolution and explicit version selection. Remove the README's unpublished-candidate notice only after both requested publication channels have been verified, and link the published package/release without moving their identities.

Reflect the release on the default branch through a pull request when repository rules require one. Preserve the verified candidate commits in the merge, satisfy all required checks, and update the README to link the published release and package.
