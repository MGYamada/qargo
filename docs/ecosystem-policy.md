# Ecosystem policy

Status: adopted design policy as of 0.1.2. Future dependency, workspace, toolchain-selection, and registry mechanisms described here are not implemented in manifest schema 1.

## Curated standard library first

Qargo borrows Cargo's familiar command structure. This does not commit Qleisli to Cargo's package governance, automatic version resolution, feature unification, or registry model.

The ecosystem's center is a curated Qleisli standard library: one reviewed source history and explicit release snapshots, with modular Qleisli interfaces and contracts. Its governance and semantic acceptance belong to the Qleisli project. Logical modules or qrates can remain useful units within that shared history; they do not need independent publication, ownership, or versions merely because the tool resembles Cargo.

Qargo consumes and records the selected Qleisli distribution and its stdlib identity. In 0.1.2 these are fixed by the linked Qleisli 0.2.1 library and bound to the host executable digest. Future selection must bind the checker, stdlib snapshot, profile, and any required proof environment without coupling their releases to Qargo's release cycle. The [toolchain design](toolchains.md) states the protocol prerequisites.

External qrates may become useful, but a public registry and automatic semver dependency resolution are deferred until semantic interfaces, compatibility, provenance, and evidence scope have explicit contracts. Cargo-style version strings in schema 1 remain package metadata; they are not a dependency compatibility judgment.

## No arbitrary qrate build code

Qargo will not introduce qrate-controlled shell hooks, `build.rs` equivalents, native procedural macros, or executable dependency-install scripts. Managing a qrate must not execute arbitrary host code supplied by its sources or dependencies. Host-language compilation and Rust development tests remain explicit developer operations outside Qargo and outside Qleisli semantic guarantees. This policy does not prohibit Cargo's own implementation-build machinery outside Qargo operations.

Invoking an explicitly selected checker or standard tool is part of Qargo's tool protocol. It must retain declared tool selection, exact input binding, result validation, and explicit failure without fallback. This is a distinct trust boundary from executing package-supplied hooks. Future QLT execution must follow a specified Qleisli test protocol; it cannot become a shell command runner. Any future declarative source expansion must remain inside specified Qleisli syntax and ordinary checking.

Qargo will not copy Cargo-style global feature unification. Different language, backend, capability, or stdlib choices must have explicit configuration identities and compatibility rules. A dependency cannot silently broaden another qrate's capabilities or change its acceptance profile by enabling a global boolean. Profile conflicts must be diagnosed until a defined composition rule exists.

## Provenance before dependency convenience

Future external dependency references must identify a trusted source and authenticated publisher namespace as well as the semantic package and fixed content identity. Official stdlib namespaces must be reserved and controlled by Qleisli's stdlib maintainers. Bare guessed names, resemblance to crates.io names, and first registration must not establish trust or ownership.

Namespace authentication establishes publisher identity, not that the publisher is trusted or its program correct. The qrate or its configured trust policy must explicitly admit the source/publisher. Content hashes bind bytes, not authenticity or mathematical validity. Failure to resolve or validate the admitted source must fail explicitly; it must not redirect to another registry or similarly named package. Diagnostic suggestions must not invent a registry dependency or silently add one.

When dependencies are supported, a separately specified lockfile should record exact resolved source/publisher identities, package content identities, dependency edges, and the selected checker/stdlib/profile binding. Normal checking and building should consume those pinned resolutions; changes to resolution should be an explicit operation. A lockfile provides reproducibility and provenance, never proof. Automatic semver solving is not a prerequisite for that first dependency model.

Local workspace convenience may be added independently of a registry once multi-qrate semantics are defined. A workspace must preserve each qrate's declared inputs, configuration, and check result; success for one member cannot certify the others. Qargo 0.1.2 still manages one local qrate per operation and has no Qargo.lock or dependency tables.

## Honest command contracts

The familiar `check`, `build`, `test`, and `doc` names keep explicit Qleisli meanings. Build records and lint results remain metadata. QLT is explicitly unavailable until implemented; documentation and formatting remain syntax-only. No Cargo tests, Rustdoc, or host-language executable substitute for an unavailable Qleisli operation.

Current unsupported dependency commands and tables must report their limits and point to supported local operations. Adding a registry or workspace later requires a separately reviewed specification and any necessary schema versions; the name Qargo does not itself authorize those features.
