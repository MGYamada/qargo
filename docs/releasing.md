# Source release checklist

v0.1.0 is an experimental GitHub source release for Linux and macOS. Keep both Rust packages unpublished, the product and qlippy qrate versions at 0.1.0, the Qleisli dependency exactly at 0.2.1, and Rust 1.85 as the MSRV.

## Prepare and validate

1. Commit the complete source tree, developer Cargo manifests and lockfile, qlippy `.rs`/`.qli` sources, tests, documentation, CI, verification script, license and notices. Exclude generated `target/` directories.
2. Run formatting, all Rust targets, Rust doctests, Clippy with warnings denied, and Rustdoc with warnings denied. These are developer Cargo operations.
3. Push the candidate commit and require all four CI combinations: Linux/macOS and Rust 1.85/stable. Each must build an extracted `git archive HEAD` source tree and pass the release verifier. The final release tag must name this exact successful commit.

The source archive must preserve the complete repository layout. After extraction and dependency preparation, its build and smoke verification commands are:

```sh
cargo fetch --locked
cargo build --release --frozen --bins
python3 scripts/verify_release.py --source-root=. --bin-dir=target/release
```

The verifier checks both versions, actual executable identities, source checking, default sibling lint discovery, snapshot contents, module indexing, stable repeated builds, input/source identity binding and unavailable QLT/qlidoc outcomes. A Cargo trap proves that the release executables do not invoke the developer build tool. Existing Rust tests retain empty-source and failure coverage.

## Publish

Check the remote tags and GitHub releases before creating `v0.1.0`. Stop if that tag or release already exists; do not force-push, replace or delete it.

Create an annotated `v0.1.0` tag for the verified commit, push that tag, and create a GitHub prerelease titled `Qargo and qlippy v0.1.0`. Use the 0.1.0 changelog section as the release notes. GitHub's source archives are the only distributed assets; do not attach Rust binaries or publish Cargo packages.

Confirm that the release is public, marked prerelease, and names the verified commit. The source archive must include Cargo configuration/lockfile, license/notice, documentation, all qlippy Rust sources and `smoke.qli`, with no generated build outputs.
