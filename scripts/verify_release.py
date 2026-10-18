#!/usr/bin/env python3
"""Verify source-release executables; never part of a qargo runtime operation."""

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import tempfile
import tomllib

from qrate_project import audit


QRATES = ("qlippy", "qlifmt", "qlidoc")
TOOLS = ("qargo", *QRATES)
QLEISLI_VERSION = "0.3.0-beta"
# Release orchestration reads one authority; component declarations stay literal.
PRODUCT_VERSION = tomllib.loads((Path(__file__).resolve().parents[1] / "Cargo.toml").read_text())["package"]["version"]


def verify_versions(root, expected=None):
    release = tomllib.loads((root / "Cargo.toml").read_text())["package"]["version"]
    require(isinstance(release, str), "Root release version must be literal")
    if expected is not None:
        require(release == expected, "Distribution expectation differs from root release version")
    for name in QRATES:
        for label, section in ((f"qrates/{name}/Qargo.toml", "qrate"), (f"rust/{name}/Cargo.toml", "package")):
            data = tomllib.loads((root / label).read_text())
            require(data[section]["version"] == release, "Release version mismatch: " + label)
        if name != "qlippy":
            data = tomllib.loads((root / f"rust/{name}/Cargo.toml").read_text())
            require(data["dependencies"]["qlippy-engine"]["version"] == "=" + release,
                    "Support dependency selection differs from the release")
    return release


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


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, "Repeated JSON field: " + key)
        result[key] = value
    return result


def invoke(binary, args, environment, expected_tool, expected_exit=0):
    require(expected_tool in TOOLS, "Expected tool must be explicit")
    output = subprocess.run(
        [str(binary), *args, "--format=json"],
        env=environment,
        capture_output=True,
        timeout=60,
    )
    require(output.returncode == expected_exit, "Unexpected exit for " + str(args) + ": " + repr(output.stdout) + repr(output.stderr))
    require(not output.stderr, "Unexpected stderr: " + repr(output.stderr))
    require(output.stdout.endswith(b"\n") and output.stdout.count(b"\n") == 1, "Expected one JSON envelope")
    report = json.loads(output.stdout, object_pairs_hook=unique_object)
    require(type(report) is dict and report.get("format") == expected_tool + ".result", "Incorrect result format")
    require(type(report.get("version")) is int and report["version"] == (2 if expected_tool == "qargo" else 1), "Incorrect result schema")
    require(report["outcome"] == ("ok" if expected_exit == 0 else "error"), "Incorrect outcome")
    if expected_exit == 0:
        require(report["diagnostics"] == [], "Unexpected diagnostic")
    return report


def expected_std(source_root):
    """Compare an explicitly associated source distribution, not binary provenance.

    Parse only the current literal sole-registry grammar. A registry refactor
    requires an explicit verifier update rather than a partial inventory.
    """
    root = source_root.resolve(strict=True)
    inputs = {}
    for label in ("Cargo.toml", "stdlib/Qargo.toml", "src/frontend/source.rs"):
        path = root / label
        require(path.is_file() and not path.is_symlink(), "Nonregular intended source input: " + label)
        inputs[label] = path.read_bytes()
    package = tomllib.loads(inputs["Cargo.toml"].decode("utf-8"))["package"]
    require(package.get("name") == "qleisli", "Intended source package is not Qleisli")
    product = package["version"]
    manifest = tomllib.loads(inputs["stdlib/Qargo.toml"].decode("utf-8"))
    require(product == QLEISLI_VERSION, "Unexpected intended Qleisli source version")
    require(type(manifest.get("schema-version")) is int and manifest["schema-version"] == 2,
            "Incorrect std manifest schema")
    require(manifest.get("qrate", {}).get("name") == "std" and
            manifest["qrate"].get("version") == product and
            manifest["qrate"].get("edition") == "2026", "Incorrect full Qleisli/std pair")
    registry = inputs["src/frontend/source.rs"].decode("utf-8")
    blocks = re.findall(r'static SOURCES: &\[BundledSource\] = &\[(.*?)\n        \];', registry, re.DOTALL)
    require(len(blocks) == 1, "Expected the single literal bundled registry")
    require(registry.count('include_str!("../../stdlib/Qargo.toml")') == 1,
            "Expected actual embedded std manifest")
    entry = re.compile(r'\s*BundledSource\s*\{\s*name: "(std::[a-z_][a-z0-9_]*)",\s*'
                       r'path: "(<bundled>/std/[a-z_][a-z0-9_]*\.qli)",\s*'
                       r'text: include_str!\("(\.\./\.\./stdlib/src/[a-z_][a-z0-9_]*\.qli)"\),\s*\},')
    position, modules, labels = 0, {}, set()
    while blocks[0][position:].strip():
        match = entry.match(blocks[0], position)
        require(match is not None, "Unsupported or incomplete literal bundled registry")
        name, location, include = match.groups()
        suffix = name.removeprefix("std::") + ".qli"
        label = include.removeprefix("../../")
        require(location == "<bundled>/std/" + suffix and label == "stdlib/src/" + suffix,
                "Noncanonical bundled source association")
        require(name not in modules and label not in labels, "Duplicate bundled module")
        path = root / label
        require(path.is_file() and not path.is_symlink(), "Nonregular bundled source: " + label)
        content = path.read_bytes()
        content.decode("utf-8")
        modules[name], inputs[label] = content, content
        labels.add(label)
        position = match.end()
    require(modules, "Empty bundled registry")
    discovered_files = files_under(root / "stdlib/src")
    for path in (root / "stdlib/src").rglob("*"):
        require(path.is_dir() or path.is_file(), "Nonregular std inventory entry: " + str(path))
    discovered = {"stdlib/src/" + label for label in discovered_files if label.endswith(".qli")}
    require(labels == discovered, "Bundled registry and complete std .qli inventory differ")
    constituents = {"manifest:std": inputs["stdlib/Qargo.toml"]}
    constituents.update({"module:" + name: content for name, content in modules.items()})
    value = {"qleisli_version": product, "std": {
        "name": "std", "version": product, "edition": "2026",
        "manifest_sha256": sha256(inputs["stdlib/Qargo.toml"]),
        "modules": [{"name": name, "sha256": sha256(content)} for name, content in sorted(modules.items())],
        "content_id": input_id("qargo.bundled-std.v1", constituents)}}
    return value, {label: sha256(content) for label, content in sorted(inputs.items())}


def source_configuration(expected):
    """Managed owning manifest fallback plus source-root/nested precedence."""
    manifest = tomllib.loads(expected["Qargo.toml"].decode("utf-8"))
    prefix = manifest["source"]["root"] + "/"
    result = {"Qargo.toml": expected["Qargo.toml"]}
    for label, content in sorted(expected.items()):
        if label.startswith(prefix) and PurePosixPath(label).name == "Qargo.toml":
            result[label[len(prefix):]] = content
    return result


def require_environment(result, std, configuration, checker, source_count):
    require(type(result.get("source_count")) is int and result["source_count"] == source_count, "Incorrect environment source count")
    require(result.get("effective_std") == std, "Effective std differs from intended source distribution")
    require(result.get("source_configuration_id") == input_id("qargo.source-configuration.v1", configuration),
            "Source configuration binding mismatch")
    native = result.get("native_decisions")
    require("native_decisions" in result, "Missing native decision scope")
    if source_count == 0:
        require(native is None, "Empty input acquired native decisions")
        return
    keys = {"checker_identity", "checker_paths", "count", "requests_checked", "complete",
            "entry_request_checked", "concrete_ir_roots", "source_preservation_verified"}
    require(type(native) is dict and set(native) == keys, "Incorrect native decision fields")
    require(native["checker_identity"] == "unpinned" and native["source_preservation_verified"] is False,
            "Native metadata claimed attestation or source preservation")
    require(type(native["count"]) is int and type(native["requests_checked"]) is int and
            0 < native["requests_checked"] <= native["count"] <= (1 << 64) - 1, "Incorrect native counters")
    require(type(native["complete"]) is bool and native["complete"], "Incomplete tiny-smoke native observations")
    require(native["entry_request_checked"] is None, "Library smoke acquired an entry request")
    paths = native["checker_paths"]
    require(type(paths) is list and all(type(path) is str for path in paths) and
            paths == sorted(set(paths)) and paths == [str(checker)], "Unexpected observed native checker path")
    roots = native["concrete_ir_roots"]
    require(type(roots) is dict and set(roots) == {"local", "bundled"} and
            all(type(value) is int and 0 <= value <= (1 << 64) - 1 for value in roots.values()), "Incorrect concrete-root counters")
    require(roots["local"] == 1 and roots["bundled"] > 0 and
            native["count"] >= roots["local"] + roots["bundled"], "Tiny-smoke roots lack native decisions")


def require_no_environment(result, configuration=None):
    require("effective_std" not in result and "native_decisions" not in result,
            "Syntax/child result claimed parent acceptance environment")
    if configuration is None:
        require("source_configuration_id" not in result, "Unexpected source configuration field")
    else:
        require(result.get("source_configuration_id") == input_id("qargo.source-configuration.v1", configuration),
                "Parent lint configuration binding mismatch")


def verify_build_record(record, name, expected_input_id, source_count, tool, result):
    check = {"status": "passed", "reason": None} if source_count else {"status": "not_run", "reason": "no_sources"}
    expected = {"format": "qargo.build-record", "version": 2, "input_id": expected_input_id,
                "qrate": {"name": name, "version": PRODUCT_VERSION}, "source_count": source_count,
                "qleisli_check": check, "profile": "finite-v0", "tool": tool,
                "effective_std": result["effective_std"], "source_configuration_id": result["source_configuration_id"],
                "native_decisions": result["native_decisions"],
                "steps": [{"name": "manifest", "status": "passed", "reason": None},
                          {"name": "snapshot", "status": "passed", "reason": None},
                          {"name": "qleisli", **check},
                          {"name": "QLT", "status": "not_run", "reason": "backend_unavailable"},
                          {"name": "qlidoc", "status": "not_run", "reason": "not_requested"},
                          {"name": "build", "status": "passed", "reason": None}]}
    require(type(record.get("version")) is int and type(record.get("source_count")) is int and
            json.dumps(record, sort_keys=True, allow_nan=False) == json.dumps(expected, sort_keys=True, allow_nan=False),
            "Stale, mixed, or incomplete build record")


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


def verify_snapshot(snapshot, expected):
    require(files_under(snapshot) == expected, "Snapshot differs from declared original inputs")
    for name in ("src", "tests", "docs"):
        require((snapshot / name).is_dir(), "Missing snapshot root: " + name)


def native_environment():
    kernel = os.environ.get("QLEISLI_KERNEL")
    require(kernel and Path(kernel).is_absolute(), "Set QLEISLI_KERNEL to an explicit compatible native checker")
    checker = Path(kernel).resolve(strict=True)
    str(checker).encode("utf-8")
    require(checker.is_file() and os.access(checker, os.X_OK), "Native checker is not executable")
    # Match the actual execution input to the canonical expected path. The
    # Kernel preserves the selected spelling; it does not normalize reports.
    return checker, dict(os.environ, QLEISLI_KERNEL=str(checker))


def verify_qrate(qrate, binaries, tools, environment, std, checker):
    expected, sources = qrate_inputs(qrate)
    expected_input_id = input_id("qargo.qrate.v1", expected)
    expected_source_id = input_id("qleisli.source.v1", sources)
    configuration = source_configuration(expected)
    manifest = "--manifest-path=" + str(qrate / "Qargo.toml")
    reports = {}
    for command, selected_tool in (("check", "qargo"), ("build", "qargo"), ("lint", "qlippy"), ("fmt", "qlifmt"), ("doc", "qlidoc")):
        args = [command, manifest]
        if command == "lint":
            args.append("--deny-warnings")
        elif command == "fmt":
            args.append("--check")
        report = invoke(binaries["qargo"], args, environment, "qargo")
        require(report["format"] == "qargo.result" and report["command"] == command, "Incorrect command envelope")
        result = report["result"]
        check = {"status": "not_run", "reason": "syntax_only"} if command in ("fmt", "doc") else {"status": "passed", "reason": None}
        require(result["source_count"] == 1 and result["qleisli_check"] == check, "Incorrect source processing step")
        require(result["input_id"] == expected_input_id and result["source_id"] == expected_source_id, "Input/source identity mismatch")
        require("verified" not in result, "Unexpected proof claim")
        require(result["tool"] == tools[selected_tool], "Selected tool identity mismatch")
        if command in ("check", "build"):
            require_environment(result, std, configuration, checker, 1)
        else:
            require_no_environment(result, configuration if command == "lint" else None)
        if selected_tool != "qargo":
            require(result["orchestrator"] == tools["qargo"], "Orchestrator identity mismatch")
        if command == "fmt":
            require(result["check"] is True and result["changed_files"] == [] and result["updated_files"] == [] and result["diff"] == "", "Bundled source must already be formatted")
            require(result["formatted_source_id"] == expected_source_id, "Formatting changed canonical input")
        reports[command] = report

    spaced_check = invoke(binaries["qargo"], ["check", "--manifest-path", str(qrate / "Qargo.toml")], environment, "qargo")
    require(spaced_check == reports["check"], "Space-separated manifest path changed the check result")

    artifact = (qrate / reports["build"]["result"]["artifact_path"]).resolve()
    require((qrate / "target/qargo").resolve() in artifact.parents, "Build artifact escaped output root")
    snapshot = artifact / "snapshot"
    verify_snapshot(snapshot, expected)
    module_index = json.loads((artifact / "module-index.json").read_bytes())
    require(module_index == [{"name": "smoke", "path": "smoke.qli", "declarations": [{"name": "identity", "kind": "unitary"}]}], "Incorrect public module index")
    record = json.loads((artifact / "build-record.json").read_bytes(), object_pairs_hook=unique_object)
    verify_build_record(record, qrate.name, expected_input_id, 1, tools["qargo"], reports["build"]["result"])
    require(reports["check"]["result"]["native_decisions"] == reports["build"]["result"]["native_decisions"],
            "Check/build native observation scopes differ")
    require(invoke(binaries["qargo"], ["build", manifest], environment, "qargo") == reports["build"], "Repeated build changed")
    require(files_under(snapshot) == expected, "Repeated build changed snapshot")

    documentation = verify_doc(qrate, reports["doc"], expected_input_id, tools)
    repeated = invoke(binaries["qargo"], ["doc", manifest], environment, "qargo")
    require(repeated == reports["doc"], "Repeated documentation result changed")
    require(verify_doc(qrate, repeated, expected_input_id, tools) == documentation, "Repeated documentation bytes changed")

    report = invoke(binaries["qargo"], ["test", manifest], environment, "qargo", expected_exit=1)
    require(report["diagnostics"][0]["id"] == "backend_unavailable", "Unavailable QLT did not fail explicitly")
    require(report["result"]["input_id"] == expected_input_id, "Unavailable QLT lost input binding")
    require(report["result"]["backend"] == {"name": "QLT", "status": "unavailable", "reason": "not_implemented"}, "Incorrect unavailable QLT record")

    for name in ("qlippy", "qlifmt"):
        args = [str(qrate / "src")]
        args.append("--deny-warnings" if name == "qlippy" else "--check")
        report = invoke(binaries[name], args, environment, name)
        require(report["format"] == name + ".result" and report["result"]["tool"] == tools[name], "Incorrect standalone tool identity")
        require(report["result"]["source_id"] == expected_source_id, "Standalone source binding mismatch")
        require_no_environment(report["result"])
    with tempfile.TemporaryDirectory(prefix="qlidoc-release-output-") as output_root:
        destination = Path(output_root).resolve() / "docs"
        args = [str(qrate / "src"), "--output=" + str(destination)]
        report = invoke(binaries["qlidoc"], args, environment, "qlidoc")
        require(report["format"] == "qlidoc.result" and report["result"]["tool"] == tools["qlidoc"], "Incorrect standalone documentation identity")
        require(report["result"]["source_id"] == expected_source_id, "Standalone documentation source mismatch")
        require_no_environment(report["result"])
        require(files_under(destination) == documentation, "Standalone and qargo documentation differ")
        spaced_args = [str(qrate / "src"), "--output", str(destination)]
        require(invoke(binaries["qlidoc"], spaced_args, environment, "qlidoc") == report, "Space-separated output changed repeated documentation")


def verify_empty_test(binary, manifest, environment, native_marker):
    report = invoke(binary, ["test", "--manifest-path", str(manifest)], environment, "qargo", expected_exit=1)
    require(report["diagnostics"][0]["id"] == "backend_unavailable", "Empty qrate enabled QLT")
    require(not native_marker.exists(), "Empty input invoked the native checker")


def qrate_inputs_empty(root):
    expected = {"Qargo.toml": (root / "Qargo.toml").read_bytes()}
    for name in ("src", "tests", "docs"):
        require(not files_under(root / name), "Empty fixture acquired inputs")
    return expected, {}


def verify(source_root, bin_dir, qleisli_source_root=None):
    require(qleisli_source_root is not None, "Developer verification requires explicit --qleisli-source-root")
    std, source_pins = expected_std(qleisli_source_root)
    checker, native_env = native_environment()
    source_root = source_root.resolve()
    bin_dir = bin_dir.resolve()
    verify_versions(source_root, PRODUCT_VERSION)
    audit(source_root)
    for label in ("Cargo.toml", "Cargo.lock", "LICENSE", "NOTICE", "README.md", "CHANGELOG.md", "install.sh", "docs/specification.md", "docs/releasing.md", "docs/toolchains.md", "docs/ecosystem-policy.md", "docs/qrate-extraction-plan.md", "docs/qrate-development.md", "AGENTS.md", "QRATEBOUNDARY.md", *["rust/" + name + "/" + file for name in QRATES for file in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml")]):
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
        require(re.search(r'^qleisli\s*=\s*"=' + re.escape(QLEISLI_VERSION) + r'"\s*$', manifest, re.MULTILINE), "Qleisli dependency must remain exact: " + label)
    binaries = {name: bin_dir / name for name in TOOLS}

    with tempfile.TemporaryDirectory(prefix="qargo-release-developer-traps-") as trap_directory:
        trap_root = Path(trap_directory)
        markers = {name: trap_root / (name + "-was-started") for name in ("cargo", "rustdoc")}
        environment = dict(native_env, PATH=str(trap_root))
        for name, marker in markers.items():
            variable = "QARGO_" + name.upper() + "_TRAP"
            trap = trap_root / name
            trap.write_text('#!/bin/sh\nprintf started > "$' + variable + '"\nexit 99\n', encoding="utf-8")
            trap.chmod(0o755)
            environment[variable] = str(marker)
            environment[name.upper()] = str(trap)
        tools = {}
        for name, binary in binaries.items():
            report = invoke(binary, ["--version"], environment, name)
            require(report["format"] == name + ".result" and report["command"] == "version", "Incorrect version envelope")
            tool = report["result"].get("tool", report["result"])
            require(tool["name"] == name and tool["version"] == PRODUCT_VERSION, "Incorrect product version")
            require(tool["qleisli_version"] == QLEISLI_VERSION and tool["profile"] == "finite-v0", "Incorrect compiler/profile")
            require(tool["executable_sha256"] == sha256(binary.read_bytes()), "Incorrect executable identity")
            tools[name] = tool
        catalog = invoke(binaries["qlippy"], ["--list-rules"], environment, "qlippy")
        require(catalog["command"] == "list-rules", "Incorrect rule catalog command")
        policy = catalog["result"]
        require(policy["catalog_version"] == 1 and policy["tool"] == tools["qlippy"], "Incorrect catalog identity")
        require([group["id"] for group in policy["groups"]] == ["idiom", "complexity", "resource"], "Incorrect rule groups")
        require([rule["id"] for rule in policy["rules"]] == ["double_inverse", "redundant_repeat_one", "unused_import"], "Incorrect rule inventory")
        require(all(rule["promotion"] == "advisory" and rule["default_severity"] == "warning" for rule in policy["rules"]), "Incorrect advisory rule policy")
        require("qleisli_check" not in policy and "verified" not in policy, "Rule catalog claimed checking")
        for name in QRATES:
            verify_qrate(source_root / "qrates" / name, binaries, tools, environment, std, checker)
            require(not any(marker.exists() for marker in markers.values()), "Runtime invoked developer Cargo or Rustdoc")

        with tempfile.TemporaryDirectory(prefix="qargo-release-empty-") as directory:
            root = Path(directory)
            for name in ("src", "tests", "docs"):
                (root / name).mkdir()
            manifest = root / "Qargo.toml"
            manifest.write_text('schema-version = 2\n[qrate]\nname = "empty"\nversion = "'
                                + PRODUCT_VERSION + '"\nedition = "2026"\n[source]\nroot = "src"\n'
                                '[tests]\nroot = "tests"\n[docs]\nroot = "docs"\n', encoding="utf-8")
            # A rejecting trap establishes no native invocation for valid empties;
            # it never supplies acceptance and is not used for nonempty inputs.
            marker = trap_root / "empty-native-was-started"
            native_trap = trap_root / "empty-native-trap"
            native_trap.write_text('#!/bin/sh\nprintf started > "$QARGO_EMPTY_NATIVE_TRAP"\nexit 99\n', encoding="utf-8")
            native_trap.chmod(0o755)
            empty_environment = dict(environment, QLEISLI_KERNEL=str(native_trap), QARGO_EMPTY_NATIVE_TRAP=str(marker))
            configuration = {"Qargo.toml": manifest.read_bytes()}
            empty_inputs, _ = qrate_inputs_empty(root)
            for command in ("check", "build", "lint", "fmt", "doc"):
                report = invoke(binaries["qargo"], [command, "--manifest-path", str(manifest)], empty_environment, "qargo")
                require(report["result"]["source_count"] == 0, "Empty root acquired sources")
                require(report["result"]["qleisli_check"] == {"status": "not_run", "reason": "no_sources"},
                        "Empty root claimed Qleisli checking")
                if command in ("check", "build"):
                    require_environment(report["result"], std, configuration, checker, 0)
                else:
                    require_no_environment(report["result"], configuration if command == "lint" else None)
                if command == "build":
                    artifact = root / report["result"]["artifact_path"]
                    verify_snapshot(artifact / "snapshot", empty_inputs)
                    record = json.loads((artifact / "build-record.json").read_bytes(), object_pairs_hook=unique_object)
                    verify_build_record(record, "empty", input_id("qargo.qrate.v1", empty_inputs), 0, tools["qargo"], report["result"])
                require(not marker.exists(), "Empty input invoked the native checker")
            verify_empty_test(binaries["qargo"], manifest, empty_environment, marker)
            require(not any(marker.exists() for marker in markers.values()), "Empty runtime invoked Cargo or Rustdoc")

    require(expected_std(qleisli_source_root) == (std, source_pins), "Intended Qleisli/std source inputs changed during verification")
    print("Scope: developer comparison with the explicitly intended Qleisli/std sources; no binary-source attestation, child std attestation, registry resolution, or new release authorization.")
    print(f"Source verification passed: four {PRODUCT_VERSION} executables, three complete qrates, advisory rule catalog, both path-option forms, checked smoke sources, stable snapshots/builds, canonical formatting, deterministic Markdown, unavailable QLT, and no Cargo/Rustdoc invocation.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--bin-dir", type=Path, required=True)
    parser.add_argument("--qleisli-source-root", type=Path, required=True,
                        help="Explicit intended Qleisli dependency source distribution; not binary attestation")
    args = parser.parse_args()
    verify(args.source_root, args.bin_dir, args.qleisli_source_root)


if __name__ == "__main__":
    main()
