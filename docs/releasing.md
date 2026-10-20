# Release checklist

## Selected beta and formal releases

Prepare qlirgo 0.2.0-beta first, followed by 0.2.0 only after the approved exact
Qleisli 0.3.0 stable migration and complete revalidation. Follow-up betas use
0.2.0-beta.1, beta.2, and so on; published versions/tags/assets are immutable.

Root Cargo [package].version is the release authority. The root package,
independent private Rust engines and their exact support dependency use the
complete version 0.2.0-beta. The three bundled qrates keep literal base version
0.2.0 by the selected bundle policy. Schema-2 qrate manifests also support
prerelease versions for independently versioned qrates; this does not implicitly
change bundled declarations. scripts/release_version.py
validates stable, -beta and -beta.N spellings (canonical decimal N >= 1) and
derives only the bundled-qrate base. Runtime tool compatibility always compares
the full prerelease identity, never the base. No edition is inferred.

The fixed beta dependency stays exactly Qleisli =0.3.0-beta. Its registry version
is a prerequisite for release. Path-substituted development locks/checks cannot
satisfy registry/source/package/native release verification. Do not replace it
with alpha, another version, a local/Git fallback or an unverified package.
Rust edition 2024, MSRV 1.85 and unsafe prohibition remain required.

## Beta publication gates

1. Repair worktree references after the local repository folder move and preserve
   the primary uncommitted diff and legacy release branch. Prepare the version,
   lockfiles, release notes and docs, then commit the complete candidate.
2. Require the official exact registry dependency and registry-resolved root and
   component lockfiles. Bind its actual source and associated native checker.
   Pass formatting, tests/doctests, strict Clippy and Rustdoc for the bundle and
   independent workspaces. Require the ordinary Linux/macOS x Rust 1.85/stable CI
   and three-component extraction matrix for the exact publication candidate.
3. Build-verify cargo package, compare archived bytes/normalized metadata,
   extract outside the checkout and install all four tools into a fresh root.
   Require cargo publish --dry-run --locked. The metadata URL must be the exact
   version page https://docs.rs/qlirgo/0.2.0-beta. Retain both opening bold README
   notices, complete qrate inputs and the independent boundaries.
4. Require all three supported native archive/installer jobs and collection.
   The five manual assets are the Linux x86_64 and two macOS x86_64/ARM64 archives,
   install.sh and SHA256SUMS. Each archive contains only the four executable
   tools, LICENSE and NOTICE. Record the exact candidate and tested hosts.
   The matching native checker is supplied by Qleisli and selected explicitly
   with QLEISLI_KERNEL; it is not silently bundled or downloaded by qlirgo.
5. Recheck ownership and version absence, freeze the candidate and checksums,
   then publish crates.io first. Verify downloaded archive SHA-256, exact-version
   fresh installation, versioned docs.rs page and README notices/metadata.
6. Use that same candidate for tag v0.2.0-beta and a GitHub prerelease. Attach all
   five verified assets before publishing; set prerelease=true and latest=false.
   Run published-release checks on every supported host. Beta installation always
   supplies --version 0.2.0-beta; do not compare/install a different formal latest
   release as if it were this beta. Verify beta is not latest instead.
7. Record publication URLs, commit, checksums, workflow runs, install observations
   and scope. After a partial publication failure, resume only remaining steps
   for the same candidate; do not overwrite/move artifacts or automatically yank.

## User migration and later formal release

Use the pinned examples in README.md and release-notes-0.2.0-beta.md. Existing
qargo/qlippy scripts, --qlippy and old JSON names must migrate explicitly. Prefer
Qlirgo.toml, retain Qargo.toml fallback and reject nonidentical dual files. During
beta both names remain identical in bundled qrates. Choose a fresh prefix for
old-install collisions. QLT remains unavailable and fmt/doc remain syntax-only.

After Qleisli 0.3.0 stable is published, update the exact dependency and beta-only
compiler materialization in a separate approved candidate. Keep explicit edition
and legacy fallback contracts. Repeat all package, extraction and native gates,
then publish qlirgo 0.2.0 as a formal latest release. Product/protocol/manifest
versions remain independent. Published qargo terminal wrappers stay unchanged.

## Historical 0.1.9 preparation and validation procedure

1. Check the crates.io `qargo` namespace, publisher ownership, and whether the intended version already exists. Never replace an existing version.
2. Update the changelog with the release date and make the README's version and installation instructions match the intended package. Commit the complete candidate, including Cargo configuration and lockfile, all three qrates, documentation, CI, verifiers, license, and notices. Exclude generated `target/` directories and caches.
3. Run formatting, all Rust targets, Rust doctests, Clippy with warnings denied, and Rustdoc with warnings denied. Run `scripts/test_qrate_project.py` and `scripts/qrate_project.py` for identity, release-version and architectural checks. Root workspace checks do not cover the independent component workspaces; require the [extraction checks](qrate-development.md) for all three. These are developer operations outside qargo commands.
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
tar -xzf target/package/qargo-0.1.9.crate -C /path/to/temporary-package
cargo install --locked --path /path/to/temporary-package/qargo-0.1.9 --bins --root /path/to/temporary-install
python3 scripts/verify_package.py --source-root=. --crate=target/package/qargo-0.1.9.crate --bin-dir=/path/to/temporary-install/bin
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
python3 scripts/distribution.py verify --source-root=. --archive=target/binary-assets/qargo-0.1.9-aarch64-apple-darwin.tar.gz --target=aarch64-apple-darwin
```

Use an empty output directory; archive generation and collection refuse to replace files. The build verifies the candidate's runtime behavior before archiving. The archive verifier repeats runtime checks on extracted binaries and performs a real Cargo-free installation with local responses for the production HTTPS URLs, followed by installed-bundle runtime checks. Source, binary, and crate verification remain independent. No verification calls a publication endpoint.

After all three supported native jobs pass, the collection job validates the complete archive inventories and produces `qargo-0.1.9-release-assets`, containing exactly the three `.tar.gz` files, `install.sh`, and `SHA256SUMS`. This is an Actions artifact, not a public GitHub release. Download this artifact from the exact verified candidate workflow run. To collect already downloaded per-target artifacts locally:

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
cargo install qargo --version=0.1.9 --locked --bins --root /path/to/fresh-install
```

Check all four version commands and run the package/runtime verifiers against that registry installation. All executables must report 0.1.9, and qargo must discover its installed siblings. Record the publication result and candidate commit.

## Publish a GitHub source and binary release when requested

The published-release workflow first uses its own revision's inventory verifier
with the requested tag version. It requires exactly the three supported binary
archives, `install.sh`, and `SHA256SUMS` at the download root, with no extra files,
nested directories, or symlinks. The tagged candidate's archive, checksum,
installer, and runtime verifiers then run as before. Keeping the inventory
verifier separate allows this check to cover older tags that predate it.
Local inventory-only inspection uses:

```sh
python3 scripts/distribution.py verify-published-assets --artifact-dir=/path/to/downloaded-assets --version=0.1.9
```

This inventory check does not replace content/checksum or runtime validation.
The `collect` command continues to accept nested Actions artifact directories.

This step requires authorization for the GitHub release channel. Check the remote `v0.1.9` tag and release before creating either; do not force-push or replace existing release data.

Create and push an annotated `v0.1.9` tag for the verified candidate and create a formal GitHub release titled `qargo v0.1.9`, using the changelog entry as release notes. Attach all five files from that candidate's verified `qargo-0.1.9-release-assets` artifact at creation, then publish with `--verify-tag --latest --prerelease=false --draft=false`. Never expose a formal/latest release with only some supported target assets, or overwrite an existing tag/release/asset. GitHub also supplies source archives. Confirm that the tag and source archive identify the verified candidate and include all three complete qrates without generated outputs. When the same version is also published on crates.io, retain that exact candidate commit for the tag; subsequent documentation updates must not move the tag or replace the package.

After publication, run the `Verify published release` workflow with version `0.1.9`. It re-downloads every release asset, checks its recorded SHA-256 against the tagged candidate, and repeats installer and installed-runtime verification on all three supported native environments using the actual release URLs, including default latest resolution and explicit version selection. Remove the README's unpublished-candidate notice only after both requested publication channels have been verified, and link the published package/release without moving their identities.

Reflect the release on the default branch through a pull request when repository rules require one. Preserve the verified candidate commits in the merge, satisfy all required checks, and update the README to link the published release and package.
