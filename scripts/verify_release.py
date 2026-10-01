#!/usr/bin/env python3
"""Verify source-release executables; never part of a Qargo runtime operation."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile


QRATES = ("qlippy", "qlifmt", "qlidoc")
TOOLS = ("qargo", *QRATES)
PRODUCT_VERSION = "0.1.4"


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def files_under(root):
    files = {}
    require(root.is_dir() and not root.is_symlink(), "Invalid directory: " + str(root))
    for path in sorted(root.rglob("*")):
        require(not path.is_symlink(), "Unexpected symlink: " + str(path))
        if path.is_file():
            files[path.relative_to(root).as_posix()] = path.read_bytes()
    return files


def sha256(content):
    return "sha256:" + hashlib.sha256(content).hexdigest()


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


def qrate_inputs(qrate):
    expected = {"Qargo.toml": (qrate / "Qargo.toml").read_bytes()}
    for root_name in ("src", "tests", "docs"):
        for label, content in files_under(qrate / root_name).items():
            expected[root_name + "/" + label] = content
    name = qrate.name
    manifest = expected["Qargo.toml"].decode("utf-8")
    require(re.search(r'^schema-version\s*=\s*2\s*$', manifest, re.MULTILINE), "Incorrect qrate manifest schema")
    require(re.search(r'^name\s*=\s*"' + name + r'"\s*$', manifest, re.MULTILINE), "Incorrect qrate name")
    require(re.search(r'^version\s*=\s*"' + re.escape(PRODUCT_VERSION) + r'"\s*$', manifest, re.MULTILINE), "Incorrect qrate version")
    require(re.search(r'^edition\s*=\s*"2026"\s*$', manifest, re.MULTILINE), "Qleisli edition must be explicit: " + name)
    for label in ("src/lib.rs", "src/bin/" + name + ".rs", "src/smoke.qli"):
        require(label in expected, "Missing qrate input: " + name + "/" + label)
    require(any(label.startswith("tests/") and label.endswith(".rs") for label in expected), "Missing Rust tests: " + name)
    require(not any(label.endswith(".qlt") for label in expected), "Unexpected QLT input: " + name)
    require(not (qrate / "Cargo.toml").exists(), "Developer Cargo configuration belongs outside the qrate")
    sources = {label[len("src/"):]: content for label, content in expected.items() if label.startswith("src/") and label.endswith(".qli")}
    require(list(sources) == ["smoke.qli"], "Unexpected management smoke sources: " + name)
    return expected, sources


def verify_doc(qrate, report, expected_input_id, tools):
    result = report["result"]
    expected_path = qrate / "target/qlidoc" / expected_input_id.removeprefix("sha256:") / tools["qlidoc"]["executable_sha256"].removeprefix("sha256:") / "public"
    artifact = (qrate / result["artifact_path"]).resolve()
    require(artifact == expected_path.resolve(), "Incorrect documentation output binding")
    generated = files_under(artifact)
    require(set(generated) == {"index.md", "modules/smoke.md"}, "Incorrect documentation file set")
    require(b"identity" in generated["modules/smoke.md"], "Missing public declaration documentation")
    require(result["files"] == [{"path": label, "sha256": sha256(content)} for label, content in sorted(generated.items())], "Documentation file digest mismatch")
    require(result["document_private_items"] is False, "Default documentation must be public-only")
    return generated


def verify_qrate(qrate, binaries, tools, environment):
    expected, sources = qrate_inputs(qrate)
    expected_input_id = input_id("qargo.qrate.v1", expected)
    expected_source_id = input_id("qleisli.source.v1", sources)
    manifest = "--manifest-path=" + str(qrate / "Qargo.toml")
    reports = {}
    for command, selected_tool in (("check", "qargo"), ("build", "qargo"), ("lint", "qlippy"), ("fmt", "qlifmt"), ("doc", "qlidoc")):
        args = [command, manifest]
        if command == "lint":
            args.append("--deny-warnings")
        elif command == "fmt":
            args.append("--check")
        report = invoke(binaries["qargo"], args, environment)
        require(report["format"] == "qargo.result" and report["command"] == command, "Incorrect command envelope")
        result = report["result"]
        check = {"status": "not_run", "reason": "syntax_only"} if command in ("fmt", "doc") else {"status": "passed", "reason": None}
        require(result["source_count"] == 1 and result["qleisli_check"] == check, "Incorrect source processing step")
        require(result["input_id"] == expected_input_id and result["source_id"] == expected_source_id, "Input/source identity mismatch")
        require("verified" not in result, "Unexpected proof claim")
        require(result["tool"] == tools[selected_tool], "Selected tool identity mismatch")
        if selected_tool != "qargo":
            require(result["orchestrator"] == tools["qargo"], "Orchestrator identity mismatch")
        if command == "fmt":
            require(result["check"] is True and result["changed_files"] == [] and result["updated_files"] == [] and result["diff"] == "", "Bundled source must already be formatted")
            require(result["formatted_source_id"] == expected_source_id, "Formatting changed canonical input")
        reports[command] = report

    spaced_check = invoke(binaries["qargo"], ["check", "--manifest-path", str(qrate / "Qargo.toml")], environment)
    require(spaced_check == reports["check"], "Space-separated manifest path changed the check result")

    artifact = (qrate / reports["build"]["result"]["artifact_path"]).resolve()
    require((qrate / "target/qargo").resolve() in artifact.parents, "Build artifact escaped output root")
    snapshot = artifact / "snapshot"
    require(files_under(snapshot) == expected, "Snapshot differs from declared original inputs")
    for root_name in ("src", "tests", "docs"):
        require((snapshot / root_name).is_dir(), "Missing snapshot root")
    module_index = json.loads((artifact / "module-index.json").read_bytes())
    require(module_index == [{"name": "smoke", "path": "smoke.qli", "declarations": [{"name": "identity", "kind": "unitary"}]}], "Incorrect public module index")
    record = json.loads((artifact / "build-record.json").read_bytes())
    require(record["format"] == "qargo.build-record" and record["version"] == 1, "Incorrect build record schema")
    require(record["input_id"] == expected_input_id and record["source_count"] == 1, "Incorrect build binding")
    require(record["qleisli_check"] == {"status": "passed", "reason": None} and record["tool"] == tools["qargo"], "Incorrect build provenance")
    require(record["qrate"] == {"name": qrate.name, "version": PRODUCT_VERSION} and record["profile"] == "finite-v0", "Incorrect qrate/profile")
    for backend, reason in (("QLT", "backend_unavailable"), ("qlidoc", "not_requested")):
        require({"name": backend, "status": "not_run", "reason": reason} in record["steps"], "Incorrect unrun backend step")
    require(invoke(binaries["qargo"], ["build", manifest], environment) == reports["build"], "Repeated build changed")
    require(files_under(snapshot) == expected, "Repeated build changed snapshot")

    documentation = verify_doc(qrate, reports["doc"], expected_input_id, tools)
    repeated = invoke(binaries["qargo"], ["doc", manifest], environment)
    require(repeated == reports["doc"], "Repeated documentation result changed")
    require(verify_doc(qrate, repeated, expected_input_id, tools) == documentation, "Repeated documentation bytes changed")

    report = invoke(binaries["qargo"], ["test", manifest], environment, expected_exit=1)
    require(report["diagnostics"][0]["id"] == "backend_unavailable", "Unavailable QLT did not fail explicitly")
    require(report["result"]["input_id"] == expected_input_id, "Unavailable QLT lost input binding")
    require(report["result"]["backend"] == {"name": "QLT", "status": "unavailable", "reason": "not_implemented"}, "Incorrect unavailable QLT record")

    for name in ("qlippy", "qlifmt"):
        args = [str(qrate / "src")]
        args.append("--deny-warnings" if name == "qlippy" else "--check")
        report = invoke(binaries[name], args, environment)
        require(report["format"] == name + ".result" and report["result"]["tool"] == tools[name], "Incorrect standalone tool identity")
        require(report["result"]["source_id"] == expected_source_id, "Standalone source binding mismatch")
    with tempfile.TemporaryDirectory(prefix="qlidoc-release-output-") as output_root:
        destination = Path(output_root).resolve() / "docs"
        args = [str(qrate / "src"), "--output=" + str(destination)]
        report = invoke(binaries["qlidoc"], args, environment)
        require(report["format"] == "qlidoc.result" and report["result"]["tool"] == tools["qlidoc"], "Incorrect standalone documentation identity")
        require(report["result"]["source_id"] == expected_source_id, "Standalone documentation source mismatch")
        require(files_under(destination) == documentation, "Standalone and Qargo documentation differ")
        spaced_args = [str(qrate / "src"), "--output", str(destination)]
        require(invoke(binaries["qlidoc"], spaced_args, environment) == report, "Space-separated output changed repeated documentation")


def verify(source_root, bin_dir):
    source_root = source_root.resolve()
    bin_dir = bin_dir.resolve()
    for label in ("Cargo.toml", "Cargo.lock", "LICENSE", "NOTICE", "README.md", "CHANGELOG.md", "docs/specification.md", "docs/releasing.md", "docs/toolchains.md", "docs/ecosystem-policy.md", "AGENTS.md", *["rust/" + name + "/Cargo.toml" for name in QRATES]):
        require((source_root / label).is_file(), "Missing source-release input: " + label)
    for label in ("Cargo.toml", *["rust/" + name + "/Cargo.toml" for name in QRATES]):
        manifest = (source_root / label).read_text(encoding="utf-8")
        require(re.search(r'^version\s*=\s*"' + re.escape(PRODUCT_VERSION) + r'"\s*$', manifest, re.MULTILINE), "Incorrect developer product version: " + label)
        require(re.search(r'^rust-version\s*=\s*"1\.85"\s*$', manifest, re.MULTILINE), "Incorrect MSRV: " + label)
        if label == "Cargo.toml":
            require(re.search(r'^name\s*=\s*"qargo"\s*$', manifest, re.MULTILINE), "Incorrect crates.io package name")
            require(re.search(r'^publish\s*=\s*\["crates-io"\]\s*$', manifest, re.MULTILINE), "Qargo package must target crates.io")
        else:
            require(re.search(r'^publish\s*=\s*false\s*$', manifest, re.MULTILINE), "Internal developer package must remain unpublished: " + label)
        require(re.search(r'^unsafe_code\s*=\s*"forbid"\s*$', manifest, re.MULTILINE), "Unsafe code must remain forbidden: " + label)
    require(re.search(r'^qleisli\s*=\s*"=0\.2\.1"\s*$', (source_root / "Cargo.toml").read_text(encoding="utf-8"), re.MULTILINE), "Qleisli dependency must remain exact")
    binaries = {name: bin_dir / name for name in TOOLS}

    with tempfile.TemporaryDirectory(prefix="qargo-release-developer-traps-") as trap_directory:
        trap_root = Path(trap_directory)
        markers = {name: trap_root / (name + "-was-started") for name in ("cargo", "rustdoc")}
        environment = dict(os.environ, PATH=str(trap_root))
        for name, marker in markers.items():
            variable = "QARGO_" + name.upper() + "_TRAP"
            trap = trap_root / name
            trap.write_text('#!/bin/sh\nprintf started > "$' + variable + '"\nexit 99\n', encoding="utf-8")
            trap.chmod(0o755)
            environment[variable] = str(marker)
            environment[name.upper()] = str(trap)
        tools = {}
        for name, binary in binaries.items():
            report = invoke(binary, ["--version"], environment)
            require(report["format"] == name + ".result" and report["command"] == "version", "Incorrect version envelope")
            tool = report["result"].get("tool", report["result"])
            require(tool["name"] == name and tool["version"] == PRODUCT_VERSION, "Incorrect product version")
            require(tool["qleisli_version"] == "0.2.1" and tool["profile"] == "finite-v0", "Incorrect compiler/profile")
            require(tool["executable_sha256"] == sha256(binary.read_bytes()), "Incorrect executable identity")
            tools[name] = tool
        catalog = invoke(binaries["qlippy"], ["--list-rules"], environment)
        require(catalog["command"] == "list-rules", "Incorrect rule catalog command")
        policy = catalog["result"]
        require(policy["catalog_version"] == 1 and policy["tool"] == tools["qlippy"], "Incorrect catalog identity")
        require([group["id"] for group in policy["groups"]] == ["idiom", "complexity", "resource"], "Incorrect rule groups")
        require([rule["id"] for rule in policy["rules"]] == ["double_inverse", "redundant_repeat_one", "unused_import"], "Incorrect rule inventory")
        require(all(rule["promotion"] == "advisory" and rule["default_severity"] == "warning" for rule in policy["rules"]), "Incorrect advisory rule policy")
        require("qleisli_check" not in policy and "verified" not in policy, "Rule catalog claimed checking")
        for name in QRATES:
            verify_qrate(source_root / "qrates" / name, binaries, tools, environment)
            require(not any(marker.exists() for marker in markers.values()), "Runtime invoked developer Cargo or Rustdoc")

    print(f"Source verification passed: four {PRODUCT_VERSION} executables, three complete qrates, advisory rule catalog, both path-option forms, checked smoke sources, stable snapshots/builds, canonical formatting, deterministic Markdown, unavailable QLT, and no Cargo/Rustdoc invocation.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--bin-dir", type=Path, required=True)
    args = parser.parse_args()
    verify(args.source_root, args.bin_dir)


if __name__ == "__main__":
    main()
