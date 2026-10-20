# Linked Qleisli 0.3 development migration

Status: unreleased development implementation of the authorized exact
`=0.3.0-beta` migration under
[Qleisli #171](https://github.com/MGYamada/Qleisli/issues/171). The scoped local
checks below use an explicit Qleisli source substitution and an already built
native checker. No new qargo product release or publication is selected.
Published 0.1.9 (2026-10-10 UTC, `0722fa78fe16dcf504b260103fe00f0d24d0cf88`)
and its immutable Qleisli 0.2.1 artifacts remain unchanged.

## Recorded environment

Check/build capture one private `effective_std` from the linked `bundled_std()`
after its existing immutable manifest gate. Its complete product/std versions
must agree, including prerelease/build text; std is named `std` with edition
`2026`. Record the exact manifest hash, every canonical module name/source hash
in strict name order, and a content ID. A user version, PATH compiler, support
constant or second registry cannot supply these facts. Ordinary std bodies gain
no semantic exception or proof from this metadata.

The content ID uses existing `digest_files` framing with domain
`qargo.bundled-std.v1` and a BTreeMap of `manifest:std` plus
`module:<canonical-name>` labels and exact bytes. These labels are not file paths.
Manifest/module hashes, this content ID and `source_configuration_id` are
exactly `sha256:` followed by 64 lowercase hexadecimal digits. Byte identity is
neither publisher authentication nor mathematical evidence.

The .qli-only `source_id` and original declared-qrate `input_id` are unchanged.
Configuration has a separate identity; neither field is widened or redefined.

## Source configuration

Managed check/build/lint use the captured owning manifest as root fallback.
A captured source-root `Qargo.toml` takes precedence; canonical nested source
manifests retain their exact bytes/relative labels. No intermediate ancestor
outside the declared source root is searched. Invalid source-root/nested
configuration rejects, never falls back. All managed configuration already
belongs to the raw qrate input and existing build snapshot. This rule does not
claim equivalence with every general Qleisli filesystem ancestor lookup.

Standalone lint captures sources and manifests in one bounded descriptor-
anchored traversal. Without a source-root manifest, capture the actual nearest
ancestor from the held root's validated parent chain, including on empty inputs.
Do not
recanonicalize a path to select a different hierarchy. The first found invalid
manifest rejects; older ancestors cannot replace it. Inherited bytes become
staged root `Qargo.toml`; nested labels remain canonical.

A nonempty source set or a nonempty configuration map requires staged root
`Qargo.toml`. Empty inputs may have no configuration if no ancestor is found;
nested-only configuration without a root is rejected. Every found manifest is
validated. An empty-input configuration failure or an early capture failure has
null result. For qargo check/build after environment/input capture, nonempty
loader/compiler failures retain a bound `failed/compiler_error` result with `effective_std`,
`source_configuration_id` and `native_decisions=null`. Only valid empty inputs
receive `not_run/no_sources` with no native selection.

`source_configuration_id` hashes the normalized staged map using
`qargo.source-configuration.v1`: root `Qargo.toml` when present and canonical
nested labels/raw bytes, without `..`. Do not serialize physical absolute paths
or selection origin into this ID, or claim relocation/alias authentication.
Original owning provenance remains in `input_id`/snapshot; standalone ancestor
bytes are not claimed to belong to a qrate snapshot. Captured capabilities and
freshness checks remain separate from digests. Inherited lookup revalidates
held child/parent identities and every recorded manifest entry/absence; unrelated
ancestor sibling changes are allowed. Existing owned source-tree inventory and
content-coherence checks remain unchanged; this is no atomic global snapshot.

Keep existing 4096-entry traversal, 4096-file and 16 MiB aggregate ceilings.
Depth 64 bounds filesystem traversal and inherited lookup, not arbitrary
in-memory Files staging. Staging validates canonical labels, combined source/
configuration file and byte limits, and each manifest's 64 KiB limit.
No second source traversal or racy manifest reread is permitted.
Source-context manifest failures retain their actual diagnostic without a
fabricated .qli offset; private stage paths are redacted and primary is null.

## Native checks and result schemas

Nonempty check/build/qlippy select one real native kernel using Qleisli's
existing explicit selection, including `QLEISLI_KERNEL`. Missing, incompatible
or failed checking returns failure without fallback, build, download or install.
This adds no manager selector. Formatter/documentation remain syntax-only.

Capture one `Project` from the materialized inputs before kernel selection.
Select the bound kernel using that project's actual `edition()`, check the same
retained project, and produce its module index without reloading source or
configuration after checking. Bundled std metadata and product versions do not
select the user's constitutional edition.

Parent `qargo.result` becomes version 2 for all commands, including early errors,
usage and version. `qargo.build-record` becomes version 2. The closed diagnostic
and ToolIdentity shapes remain unchanged; all child envelopes remain version 1.
Do not extend child schemas or accept a child version 2.

After input/environment capture, check/build retain `effective_std` and
`source_configuration_id` for success, compiler failure and valid empty inputs.
Early capture/environment errors may have null result. Parent lint records its
effective configuration ID, not an independently measured child std inventory.

Successful nonempty check/build record actual `ProjectCheckReport` observations:
observed checker paths, count, requests_checked, complete, optional entry request
flag and concrete local/bundled root counts. Record `checker_identity="unpinned"`
and `source_preservation_verified=false`. Empty/failure uses `native_decisions=null`,
not invented zero/partial success. Keep incomplete reports incomplete.
Contract requests do not make an enclosing entry request-checked; generic
declarations remain conditional until instantiated. The `finite-v0` tag remains
the source adapter compatibility label, not native identity or proof.

Exact build reuse compares these environment/configuration fields with the
actual expected artifact, refusing stale/mixed std/module/version records.
Historical version-1 artifacts are not rewritten or imported as new evidence.
Module-index source categories grant no semantic effect or inverse/control right.
The new metadata objects have exactly these keys and types (`uint` is a
nonnegative JSON integer, `Hash` is the prefixed SHA-256 string defined above):

```text
EffectiveStd = {
  qleisli_version: string,
  std: {name: "std", version: string, edition: "2026",
        manifest_sha256: Hash, modules: [{name: string, sha256: Hash}],
        content_id: Hash}
}
NativeDecisions = {
  checker_identity: "unpinned", checker_paths: [string],
  count: uint, requests_checked: uint, complete: bool,
  entry_request_checked: bool | null,
  concrete_ir_roots: {local: uint, bundled: uint},
  source_preservation_verified: false
}
```

Check/build add `effective_std: EffectiveStd`,
`source_configuration_id: Hash` and `native_decisions: NativeDecisions | null`
to their existing bound result; build-record version 2 carries the same metadata.
Parent lint adds only `source_configuration_id: Hash` to its existing result.
The existing envelope/diagnostic/tool fields are in the [specification](specification.md).

## Validation status

[Retained development observations](../tests/fixtures/qleisli-beta-compat/index.json)
preserve first sources, actual failures and the recorded local source windows.
The compressed archive is inert history, not an acceptance or execution API.

The recorded local source windows pass normal Rust 1.98.1 coverage of 172 tests
(composed from a full run and the two affected targets) and one complete MSRV
1.85.0 run of 172 tests. Formatting, strict all-target Clippy and warning-free
Rustdoc pass; both doctest commands succeed with zero examples. Each of the
three independent developer Cargo manifests passes its all-target tests and
strict Clippy on both toolchains: 60 qlippy, 17 qlifmt and 20 qlidoc tests.

The actual runtime verifier passes using all four development executables
copied to the Qleisli repository's root `bin/`, with explicit intended dependency
source and native checker. Actual standalone/bundle CLI comparisons pass 100
qlippy, 100 qlifmt and 110 qlidoc pairs; positive lint fixtures reach source
checking, and retained syntax/absent-name faults reach their expected diagnostics.
The associated 91 host tests also pass. These are recorded source-window results,
not exact final-commit, binary/source or isolated-build attestations.

The local development locks replace only Qleisli with the selected beta. Existing
other packages retain their versions and records; each component additionally
needs eight TOML-parse packages already pinned in the root development lock.
These path-substituted locks do not establish official registry resolution.

A separate 2026-10-19 follow-up validates the explicit-edition API migration.
The current normal Rust 1.98.1 and MSRV 1.85.0 source windows each pass all
173 bundle tests, including a real-native regression that changes only the
child-private staged manifest after Project capture. Strict all-target Clippy
passes. Each independent component passes the same 60/17/20 tests, doctest
commands and warning-free Rustdoc on both toolchains. All builds retain frozen
locks and an explicit local Qleisli source override; no registry fallback or
new product version is selected.

The four refreshed bundle binaries are deployed under the Qleisli repository's
root `bin/`. Actual runtime verification passes with the intended Qleisli/std
source and selected native checker. Fresh standalone/bundle comparisons pass
100 qlippy, 100 qlifmt and 110 qlidoc pairs. Source, checker and executable
identities remain unchanged within the recorded validation windows. These are
developer observations, not isolated-build or binary/source attestations; the
preceding 172-test and host-test records retain their original scope.

A separate diagnostic-only follow-up for #171 explains that the linked Qleisli
toolchain selects canonical std and another std version requires its matching
toolchain. The [inert first-source and before/after observations](../tests/fixtures/qleisli-beta-compat/std-selection-diagnostics.json.gz)
retain five complete projects and 25 CLI pairs: explicit matching/older/development
std and ordinary dependency tables still refuse in all six qrate operations;
the one-Bit no-dependency identity still reaches actual native checking. Only
rejection prose and the rebuilt qargo executable identity differ. Formatting and
the normal candidate build pass; the earlier full normal/MSRV matrix retains its
original source window. No acceptance rule or result schema changes.

Complete isolated extraction, actual beta registry availability with the linked
descriptor, package/native installation and official supported-host release
checks remain unvalidated. The explicit native preparation and caller wiring
have been statically reviewed and host-tested, not executed as hosted workflows.
Published 0.1.9 installation still requires its immutable Qleisli 0.2.1 pairing;
external native provisioning does not make a four-tool archive self-contained.
No publication or product-version decision is implied.

Required bounded checks cover std pairing with independent qrate versions,
manifest precedence/freshness, valid empty and invalid empty configuration,
native missing/failure, exact build reuse and stale records, existing phase/
ownership refusals, three lint rules and nonmatches, formatter tokens/comments
and syntax-only documentation. No maximum-size quantum case is required.
These are ordinary tool contracts, not source-preservation, execution/target
correctness or full QS/PR/RS/EXACT proofs. End-to-end edition/native transport
under [#255](https://github.com/MGYamada/Qleisli/issues/255) remains distinct.
