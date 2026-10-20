# qlirgo / qlinty naming migration

Status: functional qlirgo 0.2.0-beta preparation, not a publication. The GitHub
repository is MGYamada/qlirgo. The root package and private Rust component versions
are 0.2.0-beta, while bundled qrate manifests retain base version 0.2.0; the four tools are qlirgo, qlinty, qlifmt and qlidoc. qrate remains
unchanged. The root library is qlirgo_tools and shared support belongs to the
independent qlinty qrate. Root CLI/test adapters remain wiring only.

## Transitional manifests

Qlirgo.toml is preferred by qlirgo. Qargo.toml is a supported legacy fallback.
At the nearest directory containing either name, capture both entries through
one held directory, reject nonregular/symlinked entries, invalid UTF-8 and
manifests over 65536 bytes, and compare exact bytes when both exist. Differences
produce manifest_conflict without choosing an older manifest or ancestor.
All selected manifests still require schema-version = 2 and explicit string
[qrate].edition = "2026". There is no inferred/default edition.

Explicit --manifest-path names a real file and retains support for arbitrary
filenames. Missing explicit paths fail. The parent directory's standard-name
pair is validated even for explicit selection; neither spelling bypasses a
conflict. Standard-name selection prefers Qlirgo.toml after equality checking.
Managed source-root manifests override the owning fallback; nested manifests
retain nearest-directory precedence. Standalone lint inherits only the nearest
actual ancestor pair when there is no source-root manifest. Ancestor entries,
including absence of either name, and descriptor ancestry are revalidated.
Invalid nearer configuration never falls back. Raw and normalized captures
retain the existing aggregate budgets, including duplicate manifest bytes.

Bundled qrates carry identical Qlirgo.toml and Qargo.toml files; developer audits
and CI require both ordinary files and their byte equality. There is no automatic
user-manifest rewrite. User files, root and nested, remain byte-exact raw inputs.

## Separate identities and materializations

input_id hashes the complete raw qrate inventory with actual manifest filenames,
including both names when present, using qlirgo.qrate.v1. Build snapshots preserve
that inventory. An explicitly selected custom filename remains its actual name.

Effective source configuration selects each validated pair and normalizes its
label to Qlirgo.toml. Its source_configuration_id uses
qlirgo.source-configuration.v1 and this canonical relative map, including nested
paths and exact selected bytes. Identical effective bytes/configuration locations
have the same configuration ID independent of old/new/dual spelling, while their
raw qrate input IDs differ. The .qli-only qleisli.source.v1 identity is unchanged.
The actual embedded std content domain is qlirgo.bundled-std.v1.

Parent-to-child lint transport uses only the canonical configuration map. Only
the private stage passed to the fixed beta compiler substitutes Qargo.toml file
labels, preserving selected bytes and directory relationships. Derived compiler
aliases do not enter input_id, raw snapshots or configuration identities.
Manifest diagnostics refer to logical canonical configuration, without private
stage paths. Ordinary Qleisli checking is neither replaced nor weakened.
Standalone format/document operations remain syntax-only.

Qleisli stays exactly =0.3.0-beta and its source, loader and bundled std are not
edited. Its whole ecosystem filename/default migration belongs to 0.3.0 stable.
The beta registry package is unavailable; explicit local source/checker checks
cannot establish registry or release readiness.

## Public names and distribution

The CLI option is --qlinty. There are no old qargo/qlippy command aliases,
--qlippy option, old-tool search fallback or old JSON acceptance. Shell aliases
are a user choice. Parent envelopes/build records are qlirgo.result and
qlirgo.build-record version 2; the linter uses qlinty.result version 1. qlifmt and
qlidoc envelopes remain version 1. Other fields/contracts retain their shapes.
Product versions, manifest versions and protocol versions remain independent.

Artifacts use target/qlirgo. Archives use qlirgo-<version>-<target>, install state
uses lib/qlirgo, and all four binaries must have one release and linked beta
identity. Old qargo installations are not automatically modified; shared
qlifmt/qlidoc or other command collisions refuse with a separate-prefix hint.
The package requires documentation = "https://docs.rs/qlirgo/0.2.0-beta".

Published qargo 0.2.0-final and 0.2.0 remain immutable failure-only migration
wrappers. The reserved qlirgo 0.1.0 does not supply this functional bundle.
Historical records and fixed fixtures keep their original names and bytes.

## Validation

Completed local checks are recorded in [naming-validation.json](naming-validation.json):
189 root tests and all component quality checks pass on Rust 1.85 and stable;
the fresh developer installation passes actual-checker runtime verification. Registry packaging/install/publish validation, full
independent extraction Linux/macOS × Rust 1.85/stable and supported-platform
binary release checks remain release gates. Publication requires the separate beta gates in releasing.md. The previous local
verification record retains its original 0.2.0 source window; it does not certify
this version-preparation change or registry availability.

The current 0.2.0-beta preparation and its remaining publication gates are recorded
in [beta-release-validation.json](beta-release-validation.json).
