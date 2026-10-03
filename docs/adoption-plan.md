# Qargo tooling adoption plan

Status: adopted implementation direction, 2026-09-30; qleisliup coordination updated 2026-10-02. This plan schedules future work; the [specification](specification.md) remains the implemented public contract for 0.1.7. No new command, manifest field, result field, audit format, or backend is implemented by this document.

## Responsibility and order

Qargo owns reproducible input capture, standard-tool coordination, transport validation, and package-management diagnostics. Qleisli owns language acceptance, feature stabilization, compiler diagnostics, stdlib semantics, and proof authority. The upstream institutional work is tracked in [Qleisli issue 97](https://github.com/MGYamada/Qleisli/issues/97), targeting 0.5.0.

Implement the following increments in order. Each increment needs its own concrete specification and appropriate validation before becoming a public feature. Product releases need not wait for upstream 0.5.0 when an increment can use the current supported checker.

| Order | Qargo-owned increment | Initial deliverable | Upstream dependency |
| --- | --- | --- | --- |
| 1 | Diagnostic explanations | A shared, source-free explanation catalog for Qargo and standard-tool diagnostics | None for locally owned diagnostics |
| 2 | Compatibility corpus | A pinned development/CI comparison of known baseline and candidate bundles | None for current supported sources |
| 3 | Documentation examples and exercises | Explicitly classified examples checked in development/CI, plus package/tooling repair exercises | QLT execution and quantum-language exercise criteria remain upstream |
| 4 | Scoped review records | Repository-reviewed metadata with fixed subjects, named criteria, and actual reviewers | Qleisli determines stdlib promotion and mathematical-review policy |

After these foundations, connect the [toolchain selector](toolchains.md), structured repair suggestions, and QLT runner when their respective upstream protocols are specified and supported. These are separate integration increments, not implied functionality of the first four.

## 1. Explain local diagnostics

Use the existing named diagnostic IDs. Keep Qargo-owned package/manifest errors, snapshot and transport failures, formatting/documentation failures, and advisory lint rules in a shared catalog. Common support remains in qlippy. Reuse the current lint rule catalog's rationale and promotion policy rather than create a conflicting rule inventory.

Adopt a source-free `--explain <id>` direction for each executable, with human and JSON presentations. The implementation specification must define argument combinations, unknown-ID behavior, catalog ownership, and independent catalog versioning. Catalog entries should describe the cause, relevant contract, a minimal failing example where useful, and a supported correction. Diagnostic wording remains explanatory prose; the ID supplies the stable join.

Qleisli compiler IDs and source locations retain their upstream meaning. Qargo may forward or link an upstream explanation through a supported interface; it must not manufacture an authoritative compiler explanation or renumber the diagnostic. Until that interface exists, identify compiler diagnostics as upstream-owned.

Completion requires coverage of the locally owned diagnostic inventory, unambiguous catalog entries, explanation access without loading a qrate or invoking a checker, and agreement with current exit/JSON conventions. Document new public behavior in the specification before implementing it.

Keep existing result-version-1 diagnostic objects unchanged. Adding structured edits to these closed objects requires an independently versioned result contract and updated producer/transport validation. In the meantime, suggestions remain prose and qlifmt remains the implemented automated source transformation.

## 2. Compare pinned bundles in development and CI

Start with all three bundled qrates and small generated positive/negative source cases under the supported linked checker. Cover explicit edition handling, empty roots, ordinary compiler rejection, lint matches/nonmatches, formatting preservation and idempotence, documentation generation, input identity, and invalid tool transport.

Generate engine/scenario sources and valid schema-2 manifests from Rust strings in temporary directories. Every valid manifest explicitly declares `edition = "2026"`. Preserve the minimal tracked smoke samples and empty-root coverage; do not add `.qlt` files to the standard tool qrates. Give scenarios stable IDs and explicit expected outcomes.

A comparison consumes explicitly selected, already prepared baseline and candidate bundles, recording versions and actual executable digests for all four tools. Baseline selection, retrieval, and Cargo builds are developer/CI operations outside Qargo commands. A qrate cannot supply a command, build hook, or dependency script for this harness to run.

Compare accepted/rejected outcomes, diagnostic IDs and source spans, exit conventions, closed protocol shapes, formatting invariants, and generated artifact inventories. Record input identities and tool bindings for each side; do not treat changed executable digests as a regression or suppress them in the report. Diagnostic prose need not match byte-for-byte.

Version migrations are explicit scenario transitions. For example, 0.1.2 uses manifest schema 1 and 0.1.3 requires schema 2; compare their declared supported inputs and test the migration/rejection separately rather than call the intentional boundary a checker regression. Unexplained previously valid rejection and unintended acceptance of a known-invalid input both fail the comparison.

Initial comparisons run on Linux and macOS with the existing Rust 1.85/stable support matrix. Admit external curated snapshots only with pinned identities, license/provenance, and declared edition/profile/namespace compatibility. Do not imply that a newer Qleisli stdlib works with Qargo's older linked checker merely because its manifest is valid.

Start [qleisliup compatibility](toolchains.md#qleisliup-compatibility-plan) with offline local-link/proxy comparisons of prepared bundles and explicit component identities. Coordinate a separately implemented exact Qleisli library update with the manager's first production language distribution, aligning the standalone compiler, stdlib, and linked Qargo checker after compatibility checks. This linked-library update does not require an external-checker protocol. Migration to externally selected acceptance remains a later integration with its own upstream and Qargo contracts.

Completion requires a deterministic report binding scenario inputs, both bundles, actual outcomes, intentional migrations, and exclusions. These are regression observations and metadata; they do not establish mathematical correctness. Keep the existing source-archive and packaged-installation checks as separate release requirements.

## 3. Check explicit documentation examples

Add a development/CI verifier for examples in handwritten and generated documentation. Only explicitly designated, self-contained examples have a checking expectation. The example specification must distinguish syntax-only examples, ordinary-check examples, expected compiler rejection with a diagnostic ID, and illustrative fragments. Include any required local modules and the explicit edition in captured test inputs.

Start with ordinary checking under the linked Qleisli version. Check examples against frozen source/document bytes, record the tool identity and expectation, and use the same documentation artifact that is being validated. Reject unsupported annotations instead of silently treating them as successful checks.

Keep `qargo doc` and qlidoc syntax-only. The development example verifier is an independent check; it does not change documentation generation into type, ownership, contract, or execution verification. Keep `qargo test` explicitly unavailable until a QLT protocol and backend are integrated. Future test/doctest execution needs an explicit command contract and source/evidence binding.

Use the same scenario foundation for a small set of package/tooling repair exercises: invalid or missing editions, unsafe input paths, source changes, lint warnings, and formatting differences. State the expected diagnostic and correction. Quantum algorithm lessons and physical/mathematical acceptance criteria belong to the Qleisli project; Qargo can check approved examples at their declared level.

Completion requires positive, expected-rejection, syntax-only, and illustrative examples with distinct recorded expectations, including multi-module and stale-input cases. Report execution as unavailable when it is unavailable, regardless of other successful checks.

## 4. Record scoped reviews before adding dependency convenience

Begin with repository-reviewed records, without a registry or automatic trust inheritance. Specify the record format and criteria before enforcement. Each record must identify the exact reviewed content, reviewer and identity-assurance basis, criteria and their version, scope, and review result. A content change needs a new review or an explicitly scoped reviewed delta; it does not inherit an unrelated prior approval.

Separate review subjects: host implementation security, intended Qleisli mathematical specification, ordinary compiler acceptance, and independently checked proof evidence. Use raw qrate input identity for the relevant captured input scope and identify narrower source/contract subjects explicitly. Attach toolchain/environment identities where a result depends on them. A hash identifies bytes; it authenticates neither a reviewer nor a theorem.

Record only reviews that occurred. AI-generated notes do not become a human audit, and compiler acceptance does not become human mathematical review. Qleisli maintainers retain control over stdlib admission; Qargo validates a record's declared binding without independently promoting a qrate.

Completion requires defined criteria, validation of subject identity and record structure, stale/mismatched-subject rejection, and clear separation of claimed review from checked evidence. Do not make every local qrate operation depend on a new audit requirement until a separately reviewed trust-policy contract exists.

## Upstream integration conditions

| Integration | Required upstream contract | Qargo responsibility |
| --- | --- | --- |
| Toolchain selection and experimental gates | Supported editions/profiles/channels/gates, checker/stdlib identity, captured-source checking protocol, and any required proof environment | Validate an explicitly selected implementation, preserve identities, and fail without fallback or implicit installation |
| Structured repairs | Source-bound edits, UTF-8 ranges, applicability, and semantics of the relevant diagnostics | Validate stale inputs, paths/ranges and edit conflicts; apply only declared supported edits and rerun appropriate checks |
| QLT and executable doctests | Test discovery/execution, outcomes, limits, source/tool binding, and any evidence protocol | Orchestrate that protocol without invoking Rust tests, Rustdoc, or package-supplied shell code |

The Qleisli dependency remains exactly `=0.2.1` until an explicit dependency-update decision and compatibility review. Preparing a future adapter does not authorize a dependency bump. Manifest schema 2 continues to require edition `"2026"` and reject unknown fields; no provisional toolchain, channel, feature, dependency, or audit fields belong in it.

## Release and extension policy

Keep the four executables and three tool qrates on one product release. Ship bounded, validated increments through the existing [release procedure](releasing.md), without adopting a fixed calendar or daily distribution requirement at this stage. Product versions, editions, catalog versions, and manifest/result schemas remain independent.

Published versions/tags/artifacts are immutable; corrections require a new version. A future dependency withdrawal mechanism must preserve fixed historical content and state its effect on new selection and existing resolutions. Registry/yank commands and automatic semver resolution remain deferred.

Preserve the [ecosystem policy](ecosystem-policy.md): curated stdlib, admitted/authenticated sources, fixed identities, no arbitrary qrate build code, and no global feature unification. Declarative expansion and trait coherence belong to Qleisli; Qargo neither implements a second language gate nor relaxes those checking obligations.

Reference designs: [rustc structured diagnostics](https://doc.rust-lang.org/rustc/json.html) distinguish replacement edits and applicability; [Cargo Vet](https://mozilla.github.io/cargo-vet/how-it-works.html) provides criteria-based review records. Their protocols are references, not adopted Qargo wire formats.
