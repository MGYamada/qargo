#!/usr/bin/env python3
"""Verify a captured qrate's independent Rust environment, with frozen resolution."""

import argparse
import json
from pathlib import Path
import shutil
import subprocess

from capture_build import (BuildGuard, build_binding, capture_native, file_digest,
                           generated_inputs, verify_native_unchanged)
from qrate_project import (audit, audit_metadata, audit_vendor, canonical,
                           check_snapshot, closure, entries, manifest, require)


def isolated_config(project, component, work, native_env):
    for ancestor in (project, *project.parents):
        for config in (".cargo/config", ".cargo/config.toml"):
            require(not (ancestor / config).exists(), "Ambient ancestor Cargo configuration: " + str(ancestor / config))
    allowed = {'source': {'crates-io': {'replace-with': 'captured'}, 'captured': {'directory': '../../vendor'}}}
    for name in closure(component):
        development = project / "rust" / name
        require(manifest(development / ".cargo/config.toml") == allowed, "Unsupported Cargo build configuration")
        require(not (development / ".cargo/config").exists(), "Conflicting legacy Cargo configuration")
        require(not (development.parent / ".cargo").exists(), "Uncaptured intermediate Cargo configuration")
        require(not (development / "rust-toolchain").exists(), "Conflicting legacy toolchain selection")
        require(manifest(development / "rust-toolchain.toml") == {
            "toolchain": {"channel": "1.85.0", "profile": "minimal", "components": ["clippy", "rustfmt"]}},
            "Unsupported toolchain declaration")
    environment = dict(native_env)
    for name, directory in (("HOME", "home"), ("CARGO_HOME", "cargo-home"), ("RUSTUP_HOME", "rustup-home"),
                            ("TMPDIR", "tmp"), ("CARGO_TARGET_DIR", "target")):
        path = work / directory
        path.mkdir()
        environment[name] = str(path)
    environment["CARGO_NET_OFFLINE"] = "true"
    return environment


def require_lint_case(name, fixture, args, json_mode, result, envelope):
    """Do not count two matching early refusals as source-check equivalence."""
    if name != "qlinty" or args not in (["src"], ["src", "--deny-warnings"]):
        return
    expected = 1 if fixture in ("syntax", "type") else 0
    require(result.returncode == expected, "Unexpected lint fixture exit: " + fixture)
    if not json_mode:
        return
    require(envelope.get("format") == "qlinty.result" and
            type(envelope.get("version")) is int and envelope["version"] == 1 and
            envelope.get("command") == "lint" and
            envelope.get("outcome") == ("error" if expected else "ok"),
            "Incorrect lint fixture envelope: " + fixture)
    payload = envelope.get("result")
    require(isinstance(payload, dict) and type(payload.get("source_count")) is int and
            payload["source_count"] == (0 if fixture == "empty" else 1),
            "Lint fixture did not retain its actual source input: " + fixture)
    if fixture in ("syntax", "type"):
        # The retained historical "type" fixture actually calls an absent name.
        # Require that compiler fault, not an environment/kernel/config refusal.
        require(payload.get("qleisli_check") == {"status": "failed", "reason": "compiler_error"},
                "Lint fixture did not reach source checking: " + fixture)
        diagnostics = envelope.get("diagnostics")
        require(isinstance(diagnostics, list) and len(diagnostics) == 1 and
                diagnostics[0].get("category") == "compiler" and
                diagnostics[0].get("severity") == "error" and
                diagnostics[0].get("id") == ("parse" if fixture == "syntax" else "unknown_name"),
                "Lint fixture reached the wrong compiler failure: " + fixture)
    else:
        check = {"status": "not_run", "reason": "no_sources"} if fixture == "empty" else {
            "status": "passed", "reason": None}
        require(payload.get("qleisli_check") == check and envelope.get("diagnostics") == [],
                "Lint positive/empty fixture did not reach its specified result: " + fixture)


def compare_entrypoints(name, standalone, bundled, work, environment):
    """Compare canonical behavior; normalize only validated executable digests."""
    fixtures = {"empty": None, "valid": "pub unitary fn identity(q:Q<Bit>)->Q<Bit>{q}",
                "gate": "pub unitary fn had(q:Q<Bit>)->Q<Bit>{h(q)}",
                "syntax": "not a module {{{", "type": "pub unitary fn invalid()->Q<Bit>{missing()}"}
    cases = [["--help"], ["--version"], ["--unknown"], [], ["--version", "--version"],
             ["--format=xml"], ["--help", "--version"]]
    if name == "qlinty":
        cases += [["--list-rules"], ["src"], ["src", "--deny-warnings"]]
    elif name == "qlifmt":
        cases += [["src"], ["src", "--check"], ["src", "--check", "--check"]]
    else:
        cases += [["src"], ["src", "--output=docs"], ["src", "--document-private-items"], ["src", "--output"]]
    directory = work / "equivalence"
    count = 0
    for fixture, content in fixtures.items():
        for args in cases:
            for json_mode in (False, True):
                observations = []
                for binary in (standalone, bundled):
                    if directory.exists():
                        shutil.rmtree(directory)
                    (directory / "src").mkdir(parents=True)
                    if content is not None:
                        (directory / "src/smoke.qli").write_text(content)
                        (directory / "Qlirgo.toml").write_text(
                            'schema-version = 2\n[qrate]\nname = "extraction-check"\n'
                            'version = "7.4.0"\nedition = "2026"\n[source]\nroot = "src"\n')
                    command = [str(binary), *args, *(["--format=json"] if json_mode else [])]
                    result = subprocess.run(command, cwd=directory, env=environment, capture_output=True, timeout=60)
                    digest = file_digest(binary)
                    stdout = result.stdout
                    envelope = None
                    if json_mode:
                        envelope = json.loads(stdout)
                        payload = envelope.get("result") or {}
                        tool = payload.get("tool", payload if envelope["command"] == "version" else None)
                        if tool:
                            require(tool["executable_sha256"] == digest, "Invalid actual executable binding")
                        stdout = canonical(envelope)
                    require_lint_case(name, fixture, args, json_mode, result, envelope)
                    # Default qlidoc output paths also contain the actual tool hash.
                    normalize = lambda data: data.replace(digest.encode(), b"<executable>").replace(digest[7:].encode(), b"<executable-hex>")
                    effects = [(entry["path"].replace(digest[7:], "<executable-hex>"),
                                entry.get("sha256"), entry["kind"])
                               for entry in entries(directory)]
                    # entries() excludes target caches; qlidoc's default outputs
                    # are effects and must be compared explicitly here.
                    if (directory / "target").exists():
                        effects += [("target/" + entry["path"].replace(digest[7:], "<executable-hex>"),
                                     entry.get("sha256"), entry["kind"]) for entry in entries(directory / "target")]
                    observations.append((result.returncode, normalize(stdout), normalize(result.stderr), effects))
                require(observations[0] == observations[1], f"Standalone/bundle behavior differs: {name} {fixture} {args} JSON={json_mode}")
                count += 1
    print(f"{name}: {count} standalone/bundle behavior comparisons passed.", flush=True)


def verify_relocated(project, component, bundle, work, environment):
    qlirgo = bundle / "qlirgo"
    qrate = project / "qrates" / component
    original = {}
    for label in ("original", "relocated"):
        if label == "relocated":
            qrate = work / "relocated"
            shutil.copytree(project / "qrates" / component, qrate, ignore=shutil.ignore_patterns("target"))
        for command in ("check", "build", "lint", "fmt", "doc"):
            args = [str(qlirgo), command, "--manifest-path", str(qrate / "Qlirgo.toml"), "--format=json"]
            if command == "fmt":
                args.append("--check")
            result = subprocess.run(args, env=environment, capture_output=True, check=True)
            require(not result.stderr, "Unexpected qrate validation stderr")
            payload = json.loads(result.stdout)["result"]
            identities = (payload["input_id"], payload["source_id"])
            if label == "original":
                original[command] = identities
            else:
                require(original[command] == identities, "Relocation changed qrate/source identity")


def verify(project, toolchain, bundle, output, kernel):
    project, toolchain, bundle, output = (path.resolve() for path in (project, toolchain, bundle, output))
    kernel = kernel.resolve(strict=True)
    require(not kernel.is_relative_to(project), "Native kernel belongs outside the captured project")
    require(Path(__file__).resolve() == project / "scripts/verify_qrate.py",
            "Run the verifier copied into the captured project")
    require(not output.exists(), "Verification output must not exist")
    require(not output.is_relative_to(project), "Build records belong outside the project snapshot")
    record = check_snapshot(project)
    component = record["component"]
    require(not (project / "Cargo.toml").exists(), "Extraction contains a root Cargo manifest")
    audit(project, closure(component), bundle=False)
    audit_vendor(project, component)
    native, native_env = capture_native(toolchain, kernel)
    output.mkdir(parents=True)
    (output / "native-inputs.json").write_bytes(canonical(native) + b"\n")
    # A fresh target and empty homes ensure prior caches/configuration cannot
    # silently become project inputs. Registry sources are entirely vendored.
    work = output / "work"
    work.mkdir()
    environment = isolated_config(project, component, work, native_env)
    guard = BuildGuard(project, work, toolchain, native)
    development = project / "rust" / component
    cargo = toolchain / "bin/cargo"
    metadata = guard.run([str(cargo), "metadata", "--frozen", "--format-version=1"],
                         cwd=development, env=environment, stdout=subprocess.PIPE, check=True).stdout
    audit_metadata(json.loads(metadata), project)
    (output / "cargo-metadata.json").write_bytes(metadata)
    commands = [["build", "--lib", "--bins"], ["test", "--all-targets"], ["test", "--doc"],
                ["clippy", "--all-targets", "--", "-D", "warnings"], ["doc", "--no-deps"]]
    stages = []
    for args in commands:
        command = [str(cargo), args[0], "--frozen", *args[1:]]
        print(f"{component}: {' '.join(command)}", flush=True)
        guard.run(command, cwd=development, env=environment, check=True)
        # Capture after every stage; subsequent Cargo builds may replace OUT_DIR.
        stages.append({"command": args, "generated_inputs": generated_inputs(work / "target")})
    standalone = work / "target/debug" / component
    expected = manifest(development / "Cargo.toml")["package"]["version"]
    version = subprocess.check_output([str(standalone), "--version", "--format=json"], env=environment)
    payload = json.loads(version)["result"]
    require(payload.get("tool", payload)["version"] == expected, "Standalone uses another product's version")
    compare_entrypoints(component, standalone, bundle / component, work, environment)
    verify_relocated(project, component, bundle, work, environment)
    check_snapshot(project)
    verify_native_unchanged(native, toolchain)
    choices = {"host": native["host"], "target": native["host"], "package": component + "-engine",
               "profiles": ["dev", "test", "doc"], "features": "default", "commands": commands,
               "environment": environment}
    binding = build_binding(record["project_id"], native, choices, stages,
                            [{"name": component, "sha256": file_digest(standalone)}])
    (output / "build-record.json").write_bytes(canonical(binding) + b"\n")
    print(f"Independent {component} verification passed: {binding['build_environment_id']}", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project", type=Path, required=True)
    parser.add_argument("--toolchain", type=Path, required=True, help="Resolved Rust sysroot, containing bin/cargo and bin/rustc")
    parser.add_argument("--bundle-bin", type=Path, required=True, help="Explicit installed bundle for external comparisons/qlirgo validation")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--qleisli-kernel", type=Path, required=True, help="Explicit already-built matching native executable outside the captured project")
    args = parser.parse_args()
    verify(args.project, args.toolchain, args.bundle_bin, args.output, args.qleisli_kernel)


if __name__ == "__main__":
    main()
