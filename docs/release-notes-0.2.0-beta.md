# qlirgo 0.2.0-beta

Candidate release notes; publication is pending the exact Qleisli beta registry
package and all release gates. This beta is not the latest formal release.

- Rename the functional qargo and qlippy commands to qlirgo and qlinty. The bundle
  installs qlirgo, qlinty, qlifmt and qlidoc at 0.2.0-beta. Bundled qrates remain
  ordinary independent qrates at base version 0.2.0.
- JSON uses qlirgo.result and qlirgo.build-record version 2, qlinty.result version 1;
  qlifmt/qlidoc stay version 1. Artifact/install namespaces use qlirgo. Old command
  options, selected-tool responses and JSON names are not aliases.
- Qlirgo prefers Qlirgo.toml and accepts Qargo.toml fallback. Dual manifests must
  be byte-identical; explicit schema 2 and edition "2026" remain mandatory. Both
  files are retained identically in bundled qrates during the beta transition.
- Qrate versions can include prerelease identifiers, for example 0.2.0-beta,
  0.3.0-alpha or 1.0.0-rc.1. Exact declared bytes remain in snapshots, identities
  and build records. The bundle still declares qrate base version 0.2.0.
- Qleisli remains exactly 0.3.0-beta. Its whole naming/manifest migration belongs
  to 0.3.0 stable. Nonempty check/build/lint require the matching native checker
  from Qleisli, explicitly selected by absolute QLEISLI_KERNEL. The qlirgo archive
  does not install that checker. fmt/doc remain syntax-only; QLT is unavailable.

After publication, install this exact beta into a separate prefix:

```sh
cargo install qlirgo --version '=0.2.0-beta' --locked --bins --root "$HOME/.local/qlirgo-beta"
# Or use the installer from the v0.2.0-beta release:
sh install.sh --version 0.2.0-beta --prefix "$HOME/.local/qlirgo-beta"
export PATH="$HOME/.local/qlirgo-beta/bin:$PATH"
export QLEISLI_KERNEL=/absolute/path/to/matching/qleisli-kernel
```

Shared qlifmt/qlidoc command collisions stop rather than modifying old qargo
installation links. The previously published qargo 0.2.0 and 0.2.0-final remain
migration-only wrappers: one notice on stderr, no operation, exit status 1.
The README retains prominent bold non-affiliation and Rust/Cargo trademark
notices. Documentation metadata selects this beta's exact version page.
