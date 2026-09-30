# qlifmt

qlifmt 0.1.3 is the Qleisli formatter in the standard Qargo bundle.

```text
qlifmt <file-or-source-root> [--check] [--format=json]
```

The default invocation formats all captured `.qli` files before changing any
source. `--check` prints a bounded, line-oriented diff without changing files and
returns 1 when formatting is required. Parsing failures also return 1; usage
errors return 2. Empty source roots succeed.

The initial style uses four spaces and a target line width of 100 Unicode scalar
characters. Long indivisible tokens and comments may exceed that target. All
syntax token spellings, comment bodies and comment order are preserved. LF and
CRLF are retained; files containing both retain their first line-ending style.
No import sorting or style configuration is provided in 0.1.3.

Formatting parses Qleisli 0.2.1 syntax and validates token/comment preservation
and documentation attachment. It does not require or imply type, ownership,
contract or quantum verification. Qargo freezes inputs and validates the child
formatter response before applying it. Direct qlifmt invocations likewise check
that every current input still matches the captured bytes before the first
write. Changes use a same-directory atomic rename per file, preserving file
permissions. An I/O failure can leave a reported prefix of files updated; no
multi-file transaction is claimed. Symlinks and nonregular sources are rejected.

The qrate contains the formatter engine, CLI and Rust tests, plus a small
`smoke.qli` management sample. Developer Cargo manifests are outside this qrate.
Qargo never invokes Cargo to build or test these sources.
