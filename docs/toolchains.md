# Qrate semantics and acceptance toolchains

Status: design direction for a future version, not an implemented toolchain selector or manifest extension. Qargo 0.1.2 continues to link exactly Qleisli 0.2.1 and supports manifest/result schema 1.

## Current acceptance authority

Qargo delegates ordinary checking to its linked Qleisli library. qlippy uses the same linked checker before advisory linting. This fixes the accepted language, profile, and embedded stdlib at Rust build time. The user's PATH `qleisli` may be a different version and has no effect on either checker call. Records bind the linked version and profile to the actual executable digest. qlifmt and qlidoc use the linked syntax parser only and explicitly skip ordinary checking.

The sibling discovery and explicit-path rules for qlippy, qlifmt, and qlidoc select auxiliary processing tools. They do not select the acceptance implementation. Qargo owns snapshot collection and transport validation; Qleisli owns acceptance. This distinction must remain visible when a proof backend becomes available.

## Direction: qrate-selected Qleisli toolchains

A future toolchain declaration should select a Qleisli distribution independently of the Qargo product version. It belongs to the qrate's captured inputs, either in a separately specified file or a new manifest schema. Its exact syntax and discovery precedence remain undecided; schema 1 rejects unknown fields rather than silently accepting a nonfunctional selector.

Before an external checker is supported, its public protocol must specify ordinary checking of captured modules, closed result schemas, supported language/profile versions, and diagnostics bound to the exact source bytes. Selection must resolve one declared implementation and validate its identity before accepting a result. Missing or incompatible toolchains must fail explicitly, without falling back to Qargo's linked checker or an unrelated PATH executable. Selection must not implicitly download, install, or build a toolchain. The current auxiliary-tool requirement that all four executables share a product release is separate from future acceptance-toolchain compatibility.

Toolchain records must bind the requested declaration and resolved checker version/digest, language profile, bundled stdlib identity, and any proof backend, kernel, or verification environment required by the acceptance protocol. The stdlib is a constituent of the acceptance distribution; it must not vary independently without changing that toolchain identity. Local source identity remains portable and distinct from this toolchain binding. In the current release, the linked version and host digest provide the available implementation binding; there is no independent stdlib certificate or proof-backend identity.

Proof-bearing output requires a separately specified evidence format. It must state which frozen source snapshot, contracts, stdlib, backend, and verification environment the evidence covers, and how it is checked. A successful process, valid JSON response, lint result, or build record cannot substitute for checked evidence. Syntax-only steps and empty source roots must preserve their current explicit skipped-check status. Implementing toolchain selection will not by itself establish Realizability or Resource Safety.

## Qrates and host implementation inputs

The semantic package surface is the local `.qli` interface and its Qleisli contracts. A raw qrate identity also captures declared source, test, and documentation bytes for provenance. These scopes are distinct: changing captured Rust changes the input ID while leaving the Qleisli source ID and semantic declarations unchanged.

The standard tool qrates deliberately capture their Rust engines and a minimal `smoke.qli` management sample. Checking that sample establishes only ordinary Qleisli acceptance of its identity operation. It says nothing about the Rust engine's behavior. Cargo compilation, Rust tests, and Rustdoc remain developer operations outside Qargo, and their success does not become Qleisli evidence.

A future semantic registry needs a contract for exported Qleisli modules, dependencies and their identities, toolchain constraints, and the scope of any attached evidence. Host-language build inputs must be identified separately and remain outside that semantic guarantee. Neither crates.io distribution of the Rust executables nor schema-1 raw snapshots supplies this registry contract. Host build hooks, dependency tables, a Qargo registry, and proof-backed acceptance are not implemented by this design note.

The adopted [ecosystem policy](ecosystem-policy.md) keeps the curated stdlib at the center, forbids arbitrary qrate build hooks, and defers a public registry and automatic semver resolution. Future external dependencies must use trusted sources, authenticated namespaces, and fixed identities; toolchain selection must not implicitly introduce a registry or feature-unification mechanism.
