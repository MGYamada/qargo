# qlidoc

qlidoc 0.1.8 generates descriptive Markdown from Qleisli source syntax. It is
one of the three standard Qargo qrates, alongside qlippy and qlifmt. Its Rust
engine, CLI and Rust tests are qrate inputs; Cargo configuration remains
outside this qrate. Qargo never runs Cargo or Rustdoc.

```text
qlidoc <file-or-source-root> [--output=PATH] [--document-private-items] [--format=json]
```

The default output is
`target/qlidoc/<source-id>/<executable-sha256>/<public-or-all>/` beneath the
working directory, with the `sha256:` prefix omitted from directory names.
Specify an output outside the selected source root. Each complete output
contains `index.md` and `modules/<source-relative-path-with-.md>`. Output is
published atomically without replacement. An identical existing directory is
reused; inconsistent directories, extra entries, symlinks and special files
are rejected.

Module documentation and public declaration signatures and comments are
included by default. `--document-private-items` also includes private
declarations. Both inner and outer documentation comments are supported.
Ordinary comments inside signatures are retained. Module pages exist even
when they have no visible declarations. Source filenames determine module
names and links; relative paths and file bytes determine source identity.

Only syntax is parsed. Generating documentation does not establish typing,
ownership, contract validity or mathematical correctness. Documentation
examples are never executed. Author-written qrate documentation is a separate
input and is not copied into these generated pages.

All handled JSON responses use the independent `qlidoc.result` version 1
envelope. Documentation results contain `source_count`, `source_id`,
`qleisli_check`, `tool`, `document_private_items`, `artifact_path` and a sorted
`files` array of relative paths and raw-byte SHA-256 digests. The ordinary
Qleisli checker is not run: its step has reason `syntax_only`, or `no_sources`
for an empty source root. Parse failures retain input and tool identity but
have no artifact path or files. Output failures retain the intended output
path and file inventory. Exit codes are 0 for success, 1 for execution errors
and 2 for usage errors.

Future HTML generation may use qlidoc's own CSS, colors and layout. There is
no requirement to reproduce Rustdoc's visual style. HTML generation and
style design are deferred beyond this Markdown implementation.

Rust development uses the independent `rust/qlidoc/` Cargo workspace with its
own manifest, lockfile and Rust toolchain declaration. Transfer that directory
and this qrate together; see [independent development](../../../docs/qrate-development.md)
for extraction, dependency capture and verification without the original checkout.
