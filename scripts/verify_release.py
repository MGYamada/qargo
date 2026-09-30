#!/usr/bin/env python3
"""Verify source-release executables; never part of a Qargo runtime operation."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def files_under(root):
    files = {}
    for path in sorted(root.rglob("*")):
        require(not path.is_symlink(), "Unexpected symlink: " + str(path))
        if path.is_file():
            files[path.relative_to(root).as_posix()] = path.read_bytes()
    return files


def input_id(domain, files):
    digest = hashlib.sha256()

    def size(value):
        digest.update(value.to_bytes(8, "big"))

    encoded_domain = domain.encode("utf-8")
    size(len(encoded_domain))
    digest.update(encoded_domain)
    size(len(files))
    for label, content in sorted(files.items()):
        encoded_label = label.encode("utf-8")
        size(len(encoded_label))
        digest.update(encoded_label)
        size(len(content))
        digest.update(content)
    return "sha256:" + digest.hexdigest()


def invoke(binary, args, environment, expected_exit=0):
    output = subprocess.run(
        [str(binary), *args, "--format=json"],
        env=environment,
        capture_output=True,
        timeout=60,
    )
    require(output.returncode == expected_exit, "Unexpected exit for " + str(args) + ": " + repr(output.stdout) + repr(output.stderr))
    require(not output.stderr, "Unexpected stderr: " + repr(output.stderr))
    require(output.stdout.endswith(b"\n") and output.stdout.count(b"\n") == 1, "Expected one JSON envelope")
    report = json.loads(output.stdout)
    require(report["version"] == 1, "Incorrect result schema")
    require(report["outcome"] == ("ok" if expected_exit == 0 else "error"), "Incorrect outcome")
    if expected_exit == 0:
        require(report["diagnostics"] == [], "Unexpected diagnostic")
    return report


def verify(source_root, bin_dir):
    source_root = source_root.resolve()
    bin_dir = bin_dir.resolve()
    for label in ["Cargo.toml", "Cargo.lock", "rust/qlippy/Cargo.toml", "LICENSE", "NOTICE", "README.md", "CHANGELOG.md", "docs/specification.md"]:
        require((source_root / label).is_file(), "Missing source-release input: " + label)
    qrate = source_root / "qrates/qlippy"
    expected = {"Qargo.toml": (qrate / "Qargo.toml").read_bytes()}
    for root_name in ["src", "tests", "docs"]:
        for label, content in files_under(qrate / root_name).items():
            expected[root_name + "/" + label] = content
    rust_sources = [label for label in expected if label.endswith(".rs")]
    require(len(rust_sources) == 8, "Incomplete qlippy Rust sources")
    language_sources = {label[len("src/"):]: content for label, content in expected.items() if label.startswith("src/") and label.endswith(".qli")}
    require(list(language_sources) == ["smoke.qli"], "Unexpected smoke sources")
    require(not any(label.endswith(".qlt") for label in expected), "Unexpected QLT inputs")
    expected_input_id = input_id("qargo.qrate.v1", expected)
    expected_source_id = input_id("qleisli.source.v1", language_sources)
    qargo, qlippy = bin_dir / "qargo", bin_dir / "qlippy"

    with tempfile.TemporaryDirectory(prefix="qargo-release-cargo-trap-") as trap_directory:
        trap_root = Path(trap_directory)
        marker = trap_root / "cargo-was-started"
        trap = trap_root / "cargo"
        trap.write_text('#!/bin/sh\nprintf started > "$QARGO_CARGO_TRAP"\nexit 99\n', encoding="utf-8")
        trap.chmod(0o755)
        environment = dict(os.environ, PATH=str(trap_root), CARGO=str(trap), QARGO_CARGO_TRAP=str(marker))
        tools = {}
        for name, binary in [("qargo", qargo), ("qlippy", qlippy)]:
            report = invoke(binary, ["--version"], environment)
            require(report["format"] == name + ".result" and report["command"] == "version", "Incorrect version envelope")
            tool = report["result"].get("tool", report["result"])
            require(tool["name"] == name and tool["version"] == "0.1.0", "Incorrect product version")
            require(tool["qleisli_version"] == "0.2.1" and tool["profile"] == "finite-v0", "Incorrect compiler/profile")
            require(tool["executable_sha256"] == "sha256:" + hashlib.sha256(binary.read_bytes()).hexdigest(), "Incorrect executable identity")
            tools[name] = tool

        manifest = "--manifest-path=" + str(qrate / "Qargo.toml")
        reports = {}
        for command in ["check", "build", "lint"]:
            args = [command, manifest]
            if command == "lint":
                args.append("--deny-warnings")
            report = invoke(qargo, args, environment)
            require(report["format"] == "qargo.result" and report["command"] == command, "Incorrect command envelope")
            result = report["result"]
            require(result["source_count"] == 1 and result["qleisli_check"] == {"status": "passed", "reason": None}, "Smoke source was not checked")
            require(result["input_id"] == expected_input_id and result["source_id"] == expected_source_id, "Input/source identity mismatch")
            require("verified" not in result, "Unexpected proof claim")
            require(result["tool"] == tools["qlippy" if command == "lint" else "qargo"], "Tool identity mismatch")
            if command == "lint":
                require(result["orchestrator"] == tools["qargo"], "Orchestrator identity mismatch")
            reports[command] = report

        artifact = (qrate / reports["build"]["result"]["artifact_path"]).resolve()
        require((qrate / "target/qargo").resolve() in artifact.parents, "Artifact escaped output root")
        snapshot = artifact / "snapshot"
        require(files_under(snapshot) == expected, "Snapshot differs from declared original inputs")
        for root_name in ["src", "tests", "docs"]:
            require((snapshot / root_name).is_dir(), "Missing snapshot root")
        module_index = json.loads((artifact / "module-index.json").read_bytes())
        require(module_index == [{"name": "smoke", "path": "smoke.qli", "declarations": [{"name": "identity", "kind": "unitary"}]}], "Incorrect public module index")
        record = json.loads((artifact / "build-record.json").read_bytes())
        require(record["format"] == "qargo.build-record" and record["version"] == 1, "Incorrect build record schema")
        require(record["input_id"] == expected_input_id and record["source_count"] == 1, "Incorrect build binding")
        require(record["qleisli_check"] == {"status": "passed", "reason": None} and record["tool"] == tools["qargo"], "Incorrect build provenance")
        require(record["qrate"] == {"name": "qlippy", "version": "0.1.0"} and record["profile"] == "finite-v0", "Incorrect qrate/profile")
        for backend in ["QLT", "qlidoc"]:
            require({"name": backend, "status": "not_run", "reason": "backend_unavailable"} in record["steps"], "Unrun backend was not recorded")
        require(invoke(qargo, ["build", manifest], environment) == reports["build"], "Repeated build changed")
        require(files_under(snapshot) == expected, "Repeated build changed snapshot")

        for command, backend in [("test", "QLT"), ("doc", "qlidoc")]:
            report = invoke(qargo, [command, manifest], environment, expected_exit=1)
            require(report["diagnostics"][0]["id"] == "backend_unavailable", "Unavailable backend did not fail explicitly")
            require(report["result"]["input_id"] == expected_input_id, "Unavailable backend lost input binding")
            require(report["result"]["backend"] == {"name": backend, "status": "unavailable", "reason": "not_implemented"}, "Incorrect unavailable backend record")
        require(not marker.exists(), "A Qargo command invoked developer Cargo")

    print("Release verification passed: versions 0.1.0, one checked Qleisli source, eight Rust sources, stable snapshot/build, no warnings, explicit unavailable backends, and no Cargo invocation.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--bin-dir", type=Path, required=True)
    args = parser.parse_args()
    verify(args.source_root, args.bin_dir)


if __name__ == "__main__":
    main()
