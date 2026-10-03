# Qargo first principles and architecture

**Exact. Immutable. Bound. Explicit. Independent.**

Qargo coordinates operations on one local Qleisli qrate. Its fundamental subject
is a declared set of input bytes. Its fundamental result is an observation tied
to those bytes and the implementation that processed them. A successful command
has only the meaning assigned by its public contract.

These principles take inspiration from qleisliup's Exact, Immutable,
Authenticated, Explicit, Independent. Qargo uses **Bound** for its own trust
boundary: it validates correspondence between inputs, executables, responses,
and effects. Publisher authentication belongs to distribution policy and
qleisliup. A digest is neither authentication nor mathematical evidence.

## Principles and consequences

The [qrate boundary policy](../QRATEBOUNDARY.md) applies the Independent principle
to every qrate and its host development environment, regardless of repository or
bundle membership. Standard-tool version alignment is a distribution policy;
only Qleisli's bundled, version-aligned std qrate has privileged status.

| Principle | Requirement | Architectural consequence |
| --- | --- | --- |
| Exact | Process the declared bytes with an identified implementation. | Capture once; distinguish qrate input identity, Qleisli source identity, and executable identity. Check explicit editions and exact tool compatibility. |
| Immutable | An observed subject and a published artifact retain their identity. | Keep captured bytes and their digest private and read-only. Give each checker/tool a separate materialized working copy. Publish complete artifacts without replacement; reuse requires exact agreement. |
| Bound | A result applies only to the input and tool that produced it. | Bind checked qrates in one type. Hold executable snapshots and child cleanup guards through response and artifact validation. Only accepted, captured output bytes may reach the effect phase. |
| Explicit | An operation's authority and outcome are visible. | Represent commands and input selection as closed variants. Preserve source-free help/version, skipped empty checks, syntax-only processing, unavailable QLT, selected-tool failures, and partial formatting progress. |
| Independent | Each component owns one authority. | Qleisli owns acceptance; Qargo owns qrate capture and orchestration; qlippy owns shared support and advisory linting; qlifmt/qlidoc own syntax transformations; qleisliup owns lifecycle/authentication; Cargo builds the Rust implementation outside Qargo operations. |

Immutability does not prohibit an explicit formatting operation. Formatting
derives a candidate from an immutable subject, validates it, then checks that the
original inputs are still current before applying changes. The captured subject
keeps its old identity. Existing multi-file application failures report the exact
updated prefix; they are not presented as atomic success.

## Operation lifecycle

```text
arguments
  -> typed request
  -> captured qrate or standalone sources
  -> linked checking OR selected-tool execution
  -> accepted result and captured candidate bytes
  -> explicit source update or artifact publication
  -> versioned report
```

Help/version stop before input capture. QLT stops with its explicit unavailable
result after qrate validation. Check/build use ordinary linked checking; lint
validates the linked checker result returned by qlippy. Formatting/documentation
use syntax only. Empty inputs retain their explicit skipped-check status.

The phases constrain dependencies:

- `qargo::request` contains legal requests and mutually exclusive input choices;
  `qargo::cli` only parses and validates arguments.
- `qargo::manifest` discovers, validates, and captures a qrate through the shared
  descriptor-based input kernel. Captured qrate fields have read-only accessors.
- `qargo::subject` binds a captured subject to reporting context and binds a
  checked qrate to the exact snapshot checked. Standalone operations cannot
  acquire an unrelated qrate input identity.
- `qargo::operations` selects the required operation and its effects;
  `qargo::build` assembles build artifacts only from a checked qrate.
- `tools` owns auxiliary selection, captured executable launch, bounded process
  transport, and command-specific response validation. A single execution
  boundary accepts a response only after all validation succeeds. Its format
  and document plans keep validated bytes until the effect phase.
- Shared `snapshot`, `adapter`, `report`, `executable`, and `publication` support
  remains inside the qlippy qrate. The ordinary checker gets its own working
  copy, and syntax engines read immutable captured bytes.

Qlippy's `support` module is logically separate
from its linter product. Formatter/documenter consumers cannot depend on lint
rules, product defaults, or ordinary-checker execution. Root component adapters
are wiring only; Qargo retains its own orchestration and response validation.
The [boundary policy](../QRATEBOUNDARY.md#dependency-direction-and-adapter-ownership)
forbids reverse component-to-Qargo dependencies, including build/test edges and
source inclusions. Component CLIs and tests use their own packages; the public
bundle includes those canonical sources through audited alias/include adapters.
Each private Cargo environment has its own workspace, lockfile and toolchain
declaration. Extraction checks build the relocated dependency closure.

The [extraction identity contract](qrate-extraction-plan.md#exact-extraction-and-build-identity)
separates portable Rust project inputs from the actual build environment and
generated compilation inputs. It does not extend current `source_id`, `input_id`,
or public result schemas to cover host builds.

The public CLI and wire formats are defined by the [specification](specification.md).
Internal Rust types are implementation boundaries, not a new external protocol.
Requests, captured subjects, and checking state stay typed; JSON remains the existing
versioned transport, whose closed schemas reject malformed and mismatched data.

## Effects and authority

Capture and materialization may create private temporary data. They do not
authorize a source write or published output. The tool execution layer has no
user source destination. Its acceptance callback validates the complete response
and output while process-group cleanup remains armed; rejection drops the guard.
An accepted tool error remains an error, but is a valid transport result.

Only the effect phase can apply a format plan or publish a document/build plan.
It retains captured source/qrate directory capabilities across the capture/effect
boundary, revalidates their names and ancestors, and passes them to the source
replacement and publication kernels. Reopening an equivalent pathname must never
select a new subject for an effect. Child-owned paths and bytes are never reread after acceptance for
publication. The orchestrator's identity is captured before child execution and
effects, then revalidated while child cleanup is still armed. A rejected host
identity therefore prevents source writes and artifact publication.
Ordinary check/build retain and revalidate their captured host identity as well;
build verifies immediately before publication. Failed child responses expose no
unvalidated formatting changes or document inventory. A document artifact path is
assigned only after successful publication or exact reuse.

The qleisliup proxy may choose the executable bundle. It does not replace the
linked Qleisli acceptance implementation. The [toolchain plan](toolchains.md)
governs a coordinated dependency update and eventual external-checker contract.
No manager-state parser, network operation, dependency hook, or Cargo invocation
is part of this operation lifecycle.

## Validation obligations

Exercise the boundaries rather than merely test module layout: mutation or
deletion after capture, modification of a tool working copy, source/tool identity
mismatches, malformed and duplicate transport fields, descendant cleanup on
rejection, stale formatting inputs, exact artifact reuse, empty roots, and
Cargo/Rustdoc traps. Compare representative direct and orchestrated outcomes
with the pre-refactor bundle, recording executable changes as expected identity
changes. Preserve independent workspace, source, package-installation, and
native distribution validation. Public behavior changes require specification
changes and independently versioned schemas where applicable.
