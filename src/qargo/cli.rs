//! Parse Qargo command arguments without loading manifests or sources.

use std::ffi::OsString;
use std::path::PathBuf;

use crate::report::{Diagnostic, path_argument};

pub(super) const HELP: &str = "qargo check|build|test [--manifest-path=PATH] [--format=json]\nqargo lint [source-root] [--manifest-path=PATH] [--qlippy=PATH] [--deny-warnings] [--format=json]\nqargo fmt [source-root] [--manifest-path=PATH] [--qlifmt=PATH] [--check] [--format=json]\nqargo doc [--manifest-path=PATH] [--qlidoc=PATH] [--document-private-items] [--format=json]\nqargo --help|--version [--format=json]\nPath options also accept --option PATH.\nStandalone source roots and --manifest-path are mutually exclusive.\nUse qlippy --list-rules to inspect advisory rule policy.\nQargo manages Qleisli qrates. Install the Rust tools from a prebuilt GitHub release or with Cargo outside Qargo commands.";

#[derive(Default)]
pub(super) struct Options {
    pub(super) command: String,
    pub(super) manifest: Option<PathBuf>,
    pub(super) source: Option<PathBuf>,
    pub(super) qlippy: Option<PathBuf>,
    pub(super) qlifmt: Option<PathBuf>,
    pub(super) qlidoc: Option<PathBuf>,
    pub(super) deny_warnings: bool,
    pub(super) check: bool,
    pub(super) document_private_items: bool,
}

fn usage(message: impl Into<String>) -> Diagnostic {
    Diagnostic::error("invalid_arguments", "usage", message)
}

pub(super) fn parse(args: &[OsString]) -> Result<Options, Diagnostic> {
    let mut options = Options::default();
    let mut format_seen = false;
    let mut remaining = args.iter();
    while let Some(arg) = remaining.next() {
        let arg = arg
            .to_str()
            .ok_or_else(|| usage("Arguments must be UTF-8."))?;
        if arg == "--format=json" {
            if format_seen {
                return Err(usage("--format=json may only be supplied once."));
            }
            format_seen = true;
        } else if arg == "--help" || arg == "--version" {
            if !options.command.is_empty() {
                return Err(usage(
                    "--help and --version must be used without a command.",
                ));
            }
            options.command = arg[2..].into();
        } else if arg == "--manifest-path" || arg.starts_with("--manifest-path=") {
            if options.manifest.is_some() {
                return Err(usage("--manifest-path requires one nonempty path."));
            }
            options.manifest =
                Some(path_argument("--manifest-path", arg, &mut remaining).map_err(usage)?);
        } else if arg == "--qlippy" || arg.starts_with("--qlippy=") {
            if options.qlippy.is_some() {
                return Err(usage("--qlippy requires one nonempty path."));
            }
            options.qlippy = Some(path_argument("--qlippy", arg, &mut remaining).map_err(usage)?);
        } else if arg == "--qlifmt" || arg.starts_with("--qlifmt=") {
            if options.qlifmt.is_some() {
                return Err(usage("--qlifmt requires one nonempty path."));
            }
            options.qlifmt = Some(path_argument("--qlifmt", arg, &mut remaining).map_err(usage)?);
        } else if arg == "--qlidoc" || arg.starts_with("--qlidoc=") {
            if options.qlidoc.is_some() {
                return Err(usage("--qlidoc requires one nonempty path."));
            }
            options.qlidoc = Some(path_argument("--qlidoc", arg, &mut remaining).map_err(usage)?);
        } else if arg == "--check" {
            if options.check {
                return Err(usage("--check may only be supplied once."));
            }
            options.check = true;
        } else if arg == "--document-private-items" {
            if options.document_private_items {
                return Err(usage("--document-private-items may only be supplied once."));
            }
            options.document_private_items = true;
        } else if arg == "--deny-warnings" {
            if options.deny_warnings {
                return Err(usage("--deny-warnings may only be supplied once."));
            }
            options.deny_warnings = true;
        } else if arg.starts_with('-') {
            return Err(usage(format!("Unknown option: {arg}")));
        } else if options.command.is_empty() {
            if !["check", "build", "lint", "fmt", "test", "doc"].contains(&arg) {
                let mut diagnostic = usage(format!("Unknown command: {arg}"));
                diagnostic.suggestion = Some(if ["add", "remove", "update", "install", "publish"].contains(&arg) {
                    "Qargo has no qrate registry or dependency management yet. Use a local Qargo.toml with check, build, lint, fmt, doc, or test. Install the Rust tools from a prebuilt GitHub release or with Cargo outside Qargo operations. See https://github.com/MGYamada/qargo#installation."
                } else {
                    "Use qargo --help to see supported commands."
                }.into());
                return Err(diagnostic);
            }
            options.command = arg.into();
        } else if ["lint", "fmt"].contains(&options.command.as_str())
            && options.source.is_none()
            && !arg.is_empty()
        {
            options.source = Some(PathBuf::from(arg));
        } else {
            return Err(usage(format!("Unexpected argument: {arg}")));
        }
    }
    if options.command.is_empty() {
        return Err(usage("A command is required. Use qargo --help."));
    }
    if options.source.is_some() && options.manifest.is_some() {
        return Err(usage(
            "A standalone source root cannot be combined with --manifest-path; select either a source root or a qrate manifest.",
        ));
    }
    if options.command != "lint" && (options.qlippy.is_some() || options.deny_warnings) {
        return Err(usage("--qlippy and --deny-warnings apply only to lint."));
    }
    if options.command != "fmt" && (options.qlifmt.is_some() || options.check) {
        return Err(usage("--qlifmt and --check apply only to fmt."));
    }
    if options.command != "doc" && (options.qlidoc.is_some() || options.document_private_items) {
        return Err(usage(
            "--qlidoc and --document-private-items apply only to doc.",
        ));
    }
    if ["help", "version"].contains(&options.command.as_str()) && options.manifest.is_some() {
        return Err(usage("--manifest-path requires a qrate command."));
    }
    Ok(options)
}
