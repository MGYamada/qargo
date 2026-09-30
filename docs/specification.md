# Qargo and qlippy: initial specification

Status: public contract for the experimental source release 0.1.0; manifest and result schema version 1. Derived from [Qleisli issue 53](https://github.com/MGYamada/Qleisli/issues/53).

Supported release platforms are Linux and macOS, validated with Rust 1.85 and stable. Windows and Android are outside the v0.1.0 support contract. Distribution is the complete GitHub source archive or tagged checkout; no executable assets or crates.io packages are published. Rebuilding the Rust engines requires the repository layout and developer Cargo manifests/lockfile outside the qrate snapshot.

The documented CLI, manifest schema 1, independent qargo.result/qlippy.result version 1 formats and exit codes remain compatible within 0.1.x. Incompatible manifest/result changes require a new schema version, independently of product versions. Diagnostic prose and the unpublished Rust library API are not stable interfaces. Qargo and qlippy must come from the same product release.

## Boundaries

Qargo manages one local Qleisli qrate. Cargo is solely a development tool for these Rust executables; no Qargo command invokes it. Qargo has no Rust target, dependency solver, registry, downloads, publishing, Qargo.lock, or shell hooks. Zero .qli or .qlt files are supported; the qlippy qrate currently includes src/smoke.qli, a minimal unitary identity operation used to exercise qrate management, and no .qlt files. Its complete Rust engine, CLI, shared support and Rust test sources belong to the qrate and are captured as raw inputs. Developer Cargo configuration lives outside the qrate. Capturing .rs files does not authorize Qargo to compile or run them.

A qrate comprises Qargo.toml, a source root, a QLT test root, a documentation root, and deterministic local identity/build metadata. All three roots must exist as directories, may be empty, are UTF-8 qrate-relative paths, and must not escape the qrate, traverse symlinks, overlap each other, or include the reserved target directory. Unsupported schema versions and unknown fields are rejected.

```toml
schema-version = 1
[qrate]
name = "qlippy"
version = "0.1.0"
[source]
root = "src"
[tests]
root = "tests"
[docs]
root = "docs"
```

Names use ASCII letters/digits, hyphens and underscores, beginning with a letter. Versions are nonnegative canonical decimal MAJOR.MINOR.PATCH. There is no edition field until Qleisli defines editions. Explicit --manifest-path takes precedence; otherwise search current and ancestor directories for Qargo.toml. A malformed found manifest is an error, never grounds for continuing upward.

## Commands

- qargo check [--manifest-path=PATH]: validate and freeze inputs, then check the .qli source root using Qleisli 0.2.1's ordinary public check API. Library roots do not require main. All local definitions receive normal source/IR checks.
- qargo build [--manifest-path=PATH]: check, then emit the exact declared input snapshot, a public local module/declaration index, and a build record under target/qargo/<input-id>/<tool-id>/ (SHA-256 hex components). It does not compile the Rust engine or claim to produce executable quantum code.
- qargo lint [source-root] [--manifest-path=PATH] [--qlippy=PATH] [--deny-warnings]: with an explicit source-root, lint that standalone root; otherwise resolve the qrate's source root. Invoke the qlippy executable on a frozen source snapshot. Explicit tool paths are authoritative; otherwise use an executable sibling of qargo, then PATH. Never automatically build, install, download, or silently fall back after a selected tool fails.
- qargo test/doc [--manifest-path=PATH]: validate the qrate, then fail explicitly with backend_unavailable because QLT/qlidoc are not implemented. Do not substitute a host-language test runner or renderer, even for an empty package.
- qlippy <source-root> [--deny-warnings]: freeze .qli sources, check their actual source/IR through Qleisli 0.2.1, then lint local ASTs. Compiler failures prevent lint evaluation.
- qlippy --version: report engine version, linked Qleisli version, and finite-v0 profile.

All commands accept --format=json once, anywhere. --help and qargo --version are supported. Unknown/repeated options, invalid option combinations and missing arguments are usage errors. Equality-style path options are used as shown; empty values are rejected. Handled JSON-mode results emit exactly one UTF-8 object followed by LF on stdout, with no progress/log prose. Human diagnostics use stderr. Exit codes: 0 success (including warnings), 1 execution/validation/compiler/backend failure or denied warnings, 2 usage. --deny-warnings applies only to linting.

For zero .qli files, check/build/lint succeed with source_count=0 and a Qleisli step of status=not_run, reason=no_sources. They do not call the Qleisli checker and do not report verified=true. Bundled std modules are not local package declarations. No test or documentation status is inferred from a source check.

## Result version 1

Each envelope has exactly format (qargo.result or qlippy.result), version (1), command, outcome (ok/error), diagnostics (array), and result (object or null). Diagnostics have exactly id, category (qargo/compiler/lint/tool/usage), severity (error/warning), primary (location or null), message, and suggestion (string or null). Locations have path, start, end, line, column. Spans are half-open original UTF-8 byte offsets, and line/column are one-based Unicode scalar coordinates; CRLF counts as one newline. Local paths use relative forward slashes. Bundled locations use std:// labels. No lossy encoding or absolute temporary paths are permitted.

Success has no error diagnostics. Failure has at least one error diagnostic except denied-warning failure, which retains the warning diagnostics. Explanatory message and suggestion text are not stable identifiers. Lint results include source_count, source_id, a qleisli_check step, and tool information (engine/version, linked Qleisli version, profile). Compiler rejection retains that input/tool binding with qleisli_check.status=failed; input capture or usage failure may have a null result. Steps have status passed/not_run/unavailable/failed and an optional reason. A denied-warning result still includes the actual analysis result. Qargo lint also records its orchestrator identity and execution steps, and includes input_id when a qrate was captured. Build-output failures retain the captured input and source-check outcome.

Qargo treats qlippy output as untrusted transport: validate the closed envelope and diagnostic/location shapes, format/version/command/outcome, rule and compiler categories, tool/profile compatibility, source_count/source_id binding, and exit/outcome consistency. Reject invalid JSON, unknown formats, malformed locations, truncated responses and process failures. Remap locations only against the captured source bytes. Child stderr is handled as tool output, never mixed into JSON stdout.

## Lint rules

All three rules default to warning; --deny-warnings makes warnings fail without rewriting source. Suggestions are prose only.

- unused_import: flag an import only when there is no possible use in declaration bodies, meaning references, static parameters/arguments or capability requirements. Conservative name matching intentionally retains imports under ambiguous shadowing.
- redundant_repeat_one: repeat_static(1, f, input) and repeat_op(1, op). Point to the complete redundant constructor.
- double_inverse: inverse_op(inverse_op(op)). Point to the complete outer constructor.

Do not cancel arbitrary gates, use floating-point equivalence, infer clean release, or claim algorithm correctness. Syntax and all normal compiler checks apply before these advisory rules.

## Snapshots, identity and artifacts

Capture original bytes once, using sorted UTF-8 relative paths; reject symlinks and nonregular input files. Enforce bounded input collection (1 MiB/file, 16 MiB total, at most 4096 files and directory depth 64). Qleisli's own default source budget also remains in force. Identity uses SHA-256 with domain separation and length framing of paths/bytes. Qrate identity covers raw manifest bytes and all files in the three declared roots. Standalone source identity covers .qli files relative to their source root, independent of qrate location. Hidden directory markers are ordinary qrate inputs, never .qli sources.

Checks, linting and artifacts must consume the captured bytes, not a later read of the user's files. Source IDs exclude absolute paths, timestamps, generated files and tool versions; records bind tool identities and profiles separately. Record the actual host executable digest, embedded Qleisli version, selected steps and outcomes. Metadata and success are not proof. QLT and qlidoc are unavailable until their public contracts are separately implemented.

Build artifact directories contain snapshot/Qargo.toml and declared inputs (including .rs files), all three declared root directories even when empty, module-index.json (public declarations in local modules, sorted), and build-record.json. The record explicitly marks QLT and qlidoc as not_run with reason=backend_unavailable. Output uses relative paths, contains no timestamps, and is installed atomically without replacement from a complete sibling staging directory. Atomic publication is supported on Linux, Android and Apple platforms; other platforms fail explicitly until a safe implementation is added. A repeated identical build validates and reuses identical artifacts; inconsistent or symlink outputs fail rather than overwriting unrelated data. Distinct host executables keep separate records for the same input ID, allowing engine rebuilds without erasing older provenance. Generated output cannot feed back into inputs. Different toolchains/operating systems have no promised Rust-binary byte equivalence.

## Development and validation

Rust-only development commands (Cargo build/test/doc/Clippy) build and check the engines; they are outside Qargo operations. Required integration checks include zero-source qrate behavior, all rules and nonmatches, compiler rejection, shadowing/static references, UTF-8/CRLF/multiple modules, relocation/content-change identities, frozen-input behavior, unavailable backends, tool absence/failure/bad JSON, and a PATH trap proving that no Qargo command starts Cargo. Lint-rule inputs are generated from Rust strings; the tracked qlippy smoke sample also exercises mixed .rs/.qli input snapshots and public module indexing.
