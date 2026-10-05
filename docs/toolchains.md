# Qrate semantics and acceptance toolchains

Status: compatibility and toolchain evolution plan, updated 2026-10-04 for the unpublished Qargo 0.1.8 candidate. Qargo continues to link exactly Qleisli 0.2.1 and supports manifest schema 2 and independent result schema 1. The plan does not implement selection or authorize publication.

## Current acceptance authority

Qargo delegates ordinary checking to its linked Qleisli library. qlippy uses the same linked checker before advisory linting. This fixes the accepted language, profile, and embedded stdlib at Rust build time. The user's PATH `qleisli` may be a different version and has no effect on either checker call. Records bind the linked version and profile to the actual executable digest. qlifmt and qlidoc use the linked syntax parser only and explicitly skip ordinary checking.

The sibling discovery and explicit-path rules for qlippy, qlifmt, and qlidoc select auxiliary processing tools. They do not select the acceptance implementation. Qargo owns snapshot collection and transport validation; Qleisli owns acceptance. This distinction must remain visible when a proof backend becomes available.

Every qrate declares Qleisli `edition = "2026"` under `[qrate]`. Schema 2 requires this string without a default and rejects other editions. The edition is captured in the manifest and checked for compatibility with the linked implementation. A future toolchain selector must also validate that the selected checker supports the qrate's declared edition. Rust implementation manifests separately declare Cargo edition `"2024"`.

## qleisliup compatibility plan

[qleisliup](https://github.com/MGYamada/qleisliup) owns toolchain installation, selection, offline proxy dispatch, and distribution authentication. The local 0.1.0 development contract inspected on 2026-10-02 implements Stages 1–3: local selection, links/proxies, and authenticated transactional installation. Production endpoints and initial trust remain unconfigured; bootstrap and self-update remain planned. This is a development reference, not a published or jointly validated distribution.

Its repository declaration is `qleisli-toolchain.toml`, with an exact SemVer string in `[toolchain].version`. That version identifies the Qleisli distribution; qargo and qleisliup have independent product versions. Proxy selection uses a leading `+selector`, then `QLEISLIUP_TOOLCHAIN`, the nearest repository declaration, and the global default. The manager removes the leading selector and directly executes the selected tool, preserving arguments, streams, working directory, and exit/signal behavior. It prepends the selected bin directory to PATH and passes the resolved home/selection to nested calls. Qargo does not reimplement that resolution or parse the manager's human inspection output.

In 0.1.8 a proxy can select a Qargo executable, whose linked Qleisli version remains fixed at Rust build time. qlippy, qlifmt, and qlidoc must be regular executable siblings from the same Qargo product release. Qargo's explicit auxiliary paths remain authoritative, and existing sibling/PATH discovery and executable capture rules remain in effect. A manager pin is not currently Qargo acceptance configuration; declarations outside the declared qrate inputs do not enter Qargo's input identity or snapshot. No manager state or library dependency is added.

### Delivery order

| Increment | Concrete work | Completion condition |
| --- | --- | --- |
| 1. Proxy compatibility | Run already built baseline/candidate bundles through an explicitly selected, already built qleisliup in isolated temporary homes and local links. Record all component versions, actual executable digests, scenario inputs, and exits. | Direct and proxy execution agree on CLI/result behavior and linked checker identity; failure cases remain explicit. |
| 2. Linked Qleisli update | Before qleisliup's initial production distribution, name the exact Qleisli/std release to ship and update Qargo's exact library pin to that release in a separate implementation change. Adapt checker/parser APIs and review the locked dependency graph. | The standalone compiler, stdlib, and Qargo/qlippy linked checker agree on the intended Qleisli release; the compatibility corpus and existing source/package/native-install checks pass. |
| 3. Joint distribution validation | Validate the real compiler plus the complete same-release Qargo bundle under the manager's internal manifest and authenticated installation contract on all supported hosts. | Recorded versions match runtime identities; native inventory/linkage, offline execution, and real installation pass. Production trust and publication need their own authorization. |
| 4. External acceptance adapter | Specify captured-source checking, identity/diagnostics, edition/profile compatibility, declaration capture, and independently versioned Qargo records before dispatching to an external checker. | The upstream protocol and Qargo contract are supported and tested, with failure without fallback or implicit installation. |

Increment 2 is a linked-library upgrade and can proceed without an external-checker wire protocol. Coordinate it with qleisliup release preparation rather than retaining Qleisli 0.2.1 indefinitely or assuming that manager release alone selects a compatible compiler. Keep `=0.2.1` in the current candidate until the target release is named and that compatibility change is implemented. If the first manager release has no production language distribution yet, record that missing target explicitly rather than invent a Qleisli version. Each completed upgrade uses an exact pin, retains Rust 1.85 and the unsafe-code prohibition, and updates tests, diagnostics, documentation, and distribution declarations together. Edition changes require their own explicit support; they must not introduce a default.

The manager manifest's `qargo_checker_qleisli` must always describe the actual linked checker. A development bundle may contain a different standalone compiler if that difference is visible; it cannot count as completion of the release-alignment increment. Manager receipt/authentication records describe distribution history, not current executable integrity or mathematical evidence. Qargo continues to bind its own runtime executable identities.

### Initial development scenarios

| Scenario | Required observation |
| --- | --- |
| All four version commands, directly and through proxies | Same Qargo release, actual selected executable digests, linked Qleisli version, and finite-v0 profile. |
| Nonempty qrate check/build/lint/fmt/doc and empty roots | Same source binding and expected results; syntax-only and no-source skips remain explicit; `test` remains unavailable. |
| Multiple installed/local bundles and a leading override | The manager selects the requested bundle; Qargo invokes its captured same-release siblings. |
| Nearest declaration, environment override, and nested working directory | The manager's precedence applies before Qargo runs; Qargo's manifest discovery and source-relative diagnostics retain their existing meaning. |
| Compiler-only link or missing/incompatible auxiliary | Proxy-level missing tools fail without borrowing PATH tools; Qargo rejects incompatible selected auxiliaries under its existing transport contract. |
| Explicit Qargo auxiliary paths, arguments with spaces, JSON and exit 0/1/2 | Forwarded arguments retain their meaning; selected-tool identity and closed result validation remain enforced. |
| Invalid/missing selection, offline execution, Cargo/Rustdoc/network traps | No fallback, installation, download, Rust tests, or Rustdoc starts from Qargo operations or proxy dispatch. |

These scenarios extend the [pinned bundle comparison](adoption-plan.md#2-compare-pinned-bundles-in-development-and-ci). Keep retrieval/building outside Qargo commands, require explicit local tool paths, and bind upstream/candidate sources and manager executable digests in the development report. Synthetic compiler or distribution fixtures can establish process behavior only. Real compiler compatibility, authenticated production installation, and supported Linux x86_64 and macOS x86_64/ARM64 coverage must be recorded separately from any local smoke check.

### Local smoke validation, 2026-10-02

On macOS ARM64, a locally built qleisliup 0.1.0 and the Rust 1.98.1-built Qargo 0.1.6 bundle passed 28 comparisons/checks in an isolated temporary home. All four version responses and nonempty/empty qrate check/build/lint/fmt/doc/test/usage responses matched direct execution byte-for-byte, including actual executable digests and the linked Qleisli 0.2.1 identity. Standalone auxiliaries matched too. A leading selector overrode a missing environment selection; missing selections and a compiler-only link failed explicitly. Compiler, Cargo, Rustdoc, curl, and wget executable traps remained untouched.

The local link used an executable compiler fixture solely for inventory validation; Qargo's real linked checker processed the smoke source. This establishes local proxy behavior, not compatibility with a newer standalone compiler, production authentication, the complete precedence/failure matrix, or other hosts. The manager executable SHA-256 was `52a3314328b4be4af916208cbd2e8d408a22ea67172d8e27225ca62f42b5aeca`; the inspected manager specification SHA-256 was `5358e6adefe7d1623d9c2ce6f1b589e0b070a0420981930bffe1611a289ad9bb`. The full development corpus and release-alignment increments remain pending.

## Direction: qrate-selected Qleisli toolchains

qleisliup already specifies the repository declaration and executable-selection precedence above. A future Qargo acceptance contract should reuse that declaration rather than introduce a competing manager selector. Specify which declaration bytes enter the captured subject, how ancestor declarations and explicit manifest paths interact, how the resolved selection is bound, and how relocation or changed declarations are handled. Those Qargo capture/result semantics remain undecided; schema 2 rejects unknown fields rather than silently accepting a nonfunctional selector.

Before an external checker is supported, its public protocol must specify ordinary checking of captured modules, closed result schemas, supported language/profile versions, and diagnostics bound to the exact source bytes. Selection must resolve one declared implementation and validate its identity before accepting a result. Missing or incompatible toolchains must fail explicitly, without falling back to Qargo's linked checker or an unrelated PATH executable. Selection must not implicitly download, install, or build a toolchain. The current auxiliary-tool requirement that all four executables share a product release is separate from future acceptance-toolchain compatibility.

Toolchain records must bind the requested declaration and resolved checker version/digest, language profile, bundled stdlib identity, and any proof backend, kernel, or verification environment required by the acceptance protocol. The stdlib is a constituent of the acceptance distribution; it must not vary independently without changing that toolchain identity. Local source identity remains portable and distinct from this toolchain binding. In the current release, the linked version and host digest provide the available implementation binding; there is no independent stdlib certificate or proof-backend identity.

Proof-bearing output requires a separately specified evidence format. It must state which frozen source snapshot, contracts, stdlib, backend, and verification environment the evidence covers, and how it is checked. A successful process, valid JSON response, lint result, or build record cannot substitute for checked evidence. Syntax-only steps and empty source roots must preserve their current explicit skipped-check status. Implementing toolchain selection will not by itself establish Realizability or Resource Safety.

## Qrates and host implementation inputs

The semantic package surface is the local `.qli` interface and its Qleisli contracts. A raw qrate identity also captures declared source, test, and documentation bytes for provenance. These scopes are distinct: changing captured Rust changes the input ID while leaving the Qleisli source ID and semantic declarations unchanged.

The standard tool qrates deliberately capture their Rust engines and a minimal `smoke.qli` management sample. Checking that sample establishes only ordinary Qleisli acceptance of its identity operation. It says nothing about the Rust engine's behavior. Cargo compilation, Rust tests, and Rustdoc remain developer operations outside Qargo, and their success does not become Qleisli evidence.

A future semantic registry needs a contract for exported Qleisli modules, dependencies and their identities, toolchain constraints, and the scope of any attached evidence. Host-language build inputs must be identified separately and remain outside that semantic guarantee. Neither crates.io distribution of the Rust executables nor schema-2 raw snapshots supplies this registry contract. Host build hooks, dependency tables, a Qargo registry, and proof-backed acceptance are not implemented by this design note.

The adopted [ecosystem policy](ecosystem-policy.md) keeps the curated stdlib at the center, forbids arbitrary qrate build hooks, and defers a public registry and automatic semver resolution. Future external dependencies must use trusted sources, authenticated namespaces, and fixed identities; toolchain selection must not implicitly introduce a registry or feature-unification mechanism.
