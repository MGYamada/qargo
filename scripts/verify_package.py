#!/usr/bin/env python3
"""Verify the standalone crates.io archive and installed standard bundle."""

import argparse
import json
from pathlib import Path, PurePosixPath
import subprocess
import tarfile
import tempfile

from verify_release import PRODUCT_VERSION, QRATES, TOOLS, files_under, qrate_inputs, require, verify


def verify_package(source_root, archive_path, bin_dir):
    source_root = source_root.resolve()
    prefix = "qargo-" + PRODUCT_VERSION
    with tarfile.open(archive_path, "r:gz") as archive:
        packaged = {}
        for entry in archive.getmembers():
            path = PurePosixPath(entry.name)
            require(entry.isfile(), "Nonregular package entry: " + entry.name)
            require(not path.is_absolute() and path.parts[0] == prefix, "Incorrect package root")
            require(all(part not in (".", "..") for part in path.parts), "Unsafe package path")
            label = path.relative_to(prefix).as_posix()
            require(label not in packaged, "Repeated package entry: " + label)
            packaged[label] = archive.extractfile(entry).read()

    expected = {}
    for directory in ("src", "tests", "docs", "scripts"):
        for label, content in files_under(source_root / directory).items():
            if not {"target", "__pycache__", ".DS_Store"}.intersection(PurePosixPath(label).parts):
                expected[directory + "/" + label] = content
    for name in QRATES:
        inputs, _ = qrate_inputs(source_root / "qrates" / name)
        for label, content in inputs.items():
            expected["qrates/" + name + "/" + label] = content
    for label in ("README.md", "CHANGELOG.md", "LICENSE", "NOTICE", "AGENTS.md", "install.sh"):
        expected[label] = (source_root / label).read_bytes()
    expected["Cargo.toml.orig"] = (source_root / "Cargo.toml").read_bytes()
    different = [label for label, content in expected.items() if packaged.get(label) != content]
    require(not different, "Packaged source differs from the candidate or is missing: "
            + ", ".join(different[:10]))
    require(set(packaged) <= set(expected) | {"Cargo.toml", "Cargo.lock", ".cargo_vcs_info.json"},
            "Unexpected files in crates.io package")
    require("Cargo.lock" in packaged and "Cargo.toml" in packaged, "Missing package manifests")
    for name in QRATES:
        require(not any(label.startswith("qrates/" + name + "/") and label.endswith(".qlt")
                        for label in packaged), "Unexpected QLT input")

    with tempfile.TemporaryDirectory(prefix="qargo-package-check-") as directory:
        root = Path(directory)
        for label, content in packaged.items():
            destination = root / label
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(content)
        output = subprocess.run(
            ["cargo", "metadata", "--offline", "--locked", "--no-deps", "--format-version=1",
             "--manifest-path=" + str(root / "Cargo.toml")],
            capture_output=True, check=True,
        )
        packages = json.loads(output.stdout)["packages"]
        require(len(packages) == 1, "Package requires private workspace members")
        package = packages[0]
        require(package["name"] == "qargo" and package["version"] == PRODUCT_VERSION,
                "Incorrect package identity")
        require(package["rust_version"] == "1.85", "Incorrect package MSRV")
        bins = {target["name"] for target in package["targets"] if "bin" in target["kind"]}
        require(bins == set(TOOLS), "Package must install all four executables")
        require(all(dependency.get("path") is None for dependency in package["dependencies"]),
                "Unresolved path dependency")
        require(all(dependency["source"] is not None for dependency in package["dependencies"]),
                "Dependency is not available from a registry")
        compiler = [dependency for dependency in package["dependencies"]
                    if dependency["name"] == "qleisli"]
        require(len(compiler) == 1 and compiler[0]["req"] == "=0.2.1", "Incorrect Qleisli requirement")
        require(set(files_under(bin_dir.resolve())) == set(TOOLS), "Incorrect installed executable set")

    verify(source_root, bin_dir)
    print("Package verification passed: complete candidate sources, registry-only dependencies, "
          "and four installed executables.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--crate", type=Path, required=True)
    parser.add_argument("--bin-dir", type=Path, required=True)
    args = parser.parse_args()
    verify_package(args.source_root, args.crate, args.bin_dir)


if __name__ == "__main__":
    main()
