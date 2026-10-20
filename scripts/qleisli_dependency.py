#!/usr/bin/env python3
"""Read explicit developer Cargo metadata; never resolve, build, or select runtime tools."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import stat
import tomllib

EXPECTED = "0.3.0-beta"
REGISTRY = "registry+https://github.com/rust-lang/crates.io-index"


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def dependency_root(metadata, manifest_path, required_source=None):
    """Associate one actual registry dependency with its complete source directory.

    The caller must retain metadata from the same explicit locked build graph.
    This data comparison is not binary/source authentication or registry access.
    """
    manifest_path = manifest_path.resolve(strict=True)
    require(manifest_path.is_file() and not manifest_path.is_symlink(), "Invalid caller manifest")
    manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
    declared = manifest["dependencies"]["qleisli"]
    if isinstance(declared, dict) and "workspace" in declared:
        require(set(declared) == {"workspace"} and declared["workspace"] is True,
                "Unsupported Qleisli workspace inheritance")
        # Only this explicit manifest may supply the inherited pin; no ancestor lookup.
        declared = manifest.get("workspace", {}).get("dependencies", {}).get("qleisli")
    if isinstance(declared, dict):
        require(set(declared) == {"version"}, "Unsupported Qleisli dependency declaration")
        declared = declared["version"]
    require(declared == "=" + EXPECTED, "Caller must pin the exact intended Qleisli")
    require(type(metadata.get("version")) is int and metadata["version"] == 1, "Unsupported Cargo metadata")
    packages = metadata["packages"]
    roots = [package for package in packages if Path(package["manifest_path"]).resolve() == manifest_path]
    require(len(roots) == 1, "Metadata does not identify the explicit caller manifest")
    root = roots[0]
    require(metadata["resolve"]["root"] == root["id"], "Metadata selected a different root package")
    nodes = [node for node in metadata["resolve"]["nodes"] if node["id"] == root["id"]]
    require(len(nodes) == 1, "Missing or duplicate root dependency node")
    dependencies = [dependency for dependency in nodes[0]["deps"] if dependency["name"] == "qleisli"]
    require(len(dependencies) == 1, "Missing or duplicate direct Qleisli dependency")
    candidates = [package for package in packages if package["name"] == "qleisli"]
    require(len(candidates) == 1 and candidates[0]["id"] == dependencies[0]["pkg"], "Mixed or unbound Qleisli packages")
    package = candidates[0]
    require(package["version"] == EXPECTED and package["source"] == REGISTRY,
            "Registry CI requires the exact registry Qleisli; no local/Git fallback")
    dependency_manifest = Path(package["manifest_path"])
    require(dependency_manifest.is_absolute() and dependency_manifest.is_file() and not dependency_manifest.is_symlink(),
            "Invalid actual Qleisli dependency manifest")
    source = dependency_manifest.resolve(strict=True).parent
    actual = tomllib.loads(dependency_manifest.read_text(encoding="utf-8"))["package"]
    require(actual["name"] == "qleisli" and actual["version"] == EXPECTED, "Actual dependency identity differs")
    if required_source is not None:
        require(source == required_source.resolve(strict=True), "Dependency source differs from the native-provisioning source")
    text = str(source)
    text.encode("utf-8")
    require("\n" not in text and "\r" not in text, "Source directory cannot be exported as one CI environment value")
    # Source availability only; the caller still builds/audits the official kernel.
    for label in ("stdlib/Qargo.toml", "src/frontend/source.rs", "lean-kernel/Main.lean", "lean-kernel/Audit.lean",
                  "scripts/package_lean_kernel.py", "scripts/check_lean_kernel.py", "scripts/native_packet.py",
                  "tests/fixtures/verification_v022/finite/t.v2.qirf", "LICENSE", "NOTICE"):
        path = source / label
        require(path.is_file() and not path.is_symlink(), "Dependency lacks native preparation source: " + label)
    toolchain = (source / "lean-kernel/lean-toolchain").read_text().strip()
    lake = tomllib.loads((source / "lean-kernel/lakefile.toml").read_text())
    lock = json.loads((source / "lean-kernel/lake-manifest.json").read_bytes())
    require(toolchain == "leanprover/lean4:v4.30.0" and lake.get("version") == EXPECTED and
            not lake.get("require") and lock.get("packages") == [], "Expected matching pinned Mathlib-free native sources")
    return source



def locked_checksum(manifest_path):
    lock = manifest_path.resolve(strict=True).parent / "Cargo.lock"
    require(lock.is_file() and not lock.is_symlink(), "Missing explicit registry lockfile")
    packages = tomllib.loads(lock.read_text(encoding="utf-8"))["package"]
    selected = [package for package in packages if package["name"] == "qleisli"]
    require(len(selected) == 1 and selected[0].get("version") == EXPECTED and
            selected[0].get("source") == REGISTRY, "Lockfile does not bind the exact registry Qleisli")
    checksum = selected[0].get("checksum")
    require(isinstance(checksum, str) and re.fullmatch("[0-9a-f]{64}", checksum),
            "Missing registry package checksum")
    return checksum


def package_files(root):
    """Complete regular package payload, excluding only root Cargo management files."""
    result = {}
    def visit(path):
        before = path.lstat()
        label = path.relative_to(root).as_posix()
        label.encode("utf-8")
        if stat.S_ISDIR(before.st_mode):
            children = sorted(child.name for child in path.iterdir())
            for name in children:
                visit(path / name)
            require(children == sorted(child.name for child in path.iterdir()), "Package changed during comparison")
        else:
            require(stat.S_ISREG(before.st_mode), "Package links/special files are forbidden: " + label)
            if label not in (".cargo-ok", ".cargo-checksum.json"):
                result[label] = {"sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                                 "executable": bool(before.st_mode & 0o111)}
        after = path.lstat()
        require((before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns, before.st_ctime_ns) ==
                (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns, after.st_ctime_ns),
                "Package changed during comparison: " + label)
    require(root.is_dir() and not root.is_symlink(), "Invalid package source directory")
    visit(root)
    return result


def same_packaged_source(source, reference, manifest_path, reference_manifest):
    """Bind a vendored copy to an explicit registry source and both actual locks.

    Cargo's lock/checksum provenance is a development-input association, not
    executable/source authentication, a native decision or registry availability.
    """
    reference = reference.resolve(strict=True)
    checksum = locked_checksum(manifest_path)
    require(checksum == locked_checksum(reference_manifest), "Registry package checksums differ")
    index = source / ".cargo-checksum.json"
    require(index.is_file() and not index.is_symlink(), "Missing complete vendored package inventory")
    raw_index = index.read_bytes()
    record = json.loads(raw_index, object_pairs_hook=unique_object)
    require(set(record) == {"files", "package"} and record["package"] == checksum and
            isinstance(record["files"], dict), "Vendored package identity differs from the explicit locks")
    actual = package_files(source)
    hashes = {label: row["sha256"] for label, row in actual.items()}
    require(record["files"] == hashes, "Vendored inventory does not match all actual package bytes")
    require(actual == package_files(reference), "Actual registry and vendored package bytes/modes differ")
    require(index.read_bytes() == raw_index and package_files(source) == actual,
            "Package association changed during comparison")
    return source


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, "Repeated metadata field: " + key)
        result[key] = value
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--metadata", type=Path, required=True)
    parser.add_argument("--manifest-path", type=Path, required=True)
    parser.add_argument("--require-source-root", type=Path)
    parser.add_argument("--same-package-source-root", type=Path,
                        help="Explicit registry source to compare with a complete vendored copy")
    parser.add_argument("--same-package-manifest-path", type=Path,
                        help="Explicit manifest/lock pair for the compared registry source")
    args = parser.parse_args()
    metadata = json.loads(args.metadata.read_bytes(), object_pairs_hook=unique_object)
    require((args.same_package_source_root is None) == (args.same_package_manifest_path is None),
            "Supply both explicit same-package inputs")
    source = dependency_root(metadata, args.manifest_path, args.require_source_root)
    if args.same_package_source_root is not None:
        same_packaged_source(source, args.same_package_source_root, args.manifest_path,
                             args.same_package_manifest_path)
    print(source)


if __name__ == "__main__":
    main()
