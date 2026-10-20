#!/usr/bin/env python3
"""Host-only checks of explicit metadata association; no Cargo/native execution."""

import copy
import json
from pathlib import Path
import tempfile
import shutil
import tomllib
import unittest
from unittest.mock import patch

from qleisli_dependency import dependency_root, main, package_files, same_packaged_source


class DependencyAssociationTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.manifest = self.root / "Cargo.toml"
        self.manifest.write_text('[package]\nname="qlirgo"\nversion="0.1.9"\n[dependencies]\nqleisli="=0.3.0-beta"\n')
        self.source = self.root / "registry/qleisli-0.3.0-beta"
        files = {"Cargo.toml": '[package]\nname="qleisli"\nversion="0.3.0-beta"\n',
                 "lean-kernel/lean-toolchain": "leanprover/lean4:v4.30.0\n",
                 "lean-kernel/lakefile.toml": 'name="qleisli_kernel"\nversion="0.3.0-beta"\n',
                 "lean-kernel/lake-manifest.json": '{"packages":[]}',
                 "stdlib/Qargo.toml": "not interpreted here", "src/frontend/source.rs": "source",
                 "lean-kernel/Main.lean": "source", "lean-kernel/Audit.lean": "source",
                 "scripts/package_lean_kernel.py": "not executed", "scripts/check_lean_kernel.py": "not executed",
                 "scripts/native_packet.py": "not executed", "tests/fixtures/verification_v022/finite/t.v2.qirf": "fixture",
                 "LICENSE": "license", "NOTICE": "notice"}
        for label, content in files.items():
            path = self.source / label
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content)
        self.metadata = {"version": 1, "packages": [
            {"id": "root", "name": "qlirgo", "manifest_path": str(self.manifest)},
            {"id": "dependency", "name": "qleisli", "version": "0.3.0-beta",
             "source": "registry+https://github.com/rust-lang/crates.io-index", "manifest_path": str(self.source / "Cargo.toml")}],
            "resolve": {"root": "root", "nodes": [{"id": "root", "deps": [{"name": "qleisli", "pkg": "dependency"}]}]}}

    def test_explicit_root_and_direct_package_are_bound_without_a_resolver(self):
        self.assertEqual(dependency_root(self.metadata, self.manifest), self.source)
        self.assertEqual(dependency_root(self.metadata, self.manifest, self.source), self.source)
        self.manifest.write_text(self.manifest.read_text().replace('qleisli="=0.3.0-beta"', '[dependencies.qleisli]\nversion="=0.3.0-beta"'))
        self.assertEqual(dependency_root(self.metadata, self.manifest), self.source)

    def test_actual_qlirgo_root_workspace_declaration_is_bound(self):
        # Exercise the real source manifest shape, not a normalized package copy.
        source_manifest = Path(__file__).resolve().parent.parent / "Cargo.toml"
        manifest = tomllib.loads(source_manifest.read_text(encoding="utf-8"))
        self.assertEqual(manifest["dependencies"]["qleisli"], {"workspace": True})
        self.assertEqual(manifest["workspace"]["dependencies"]["qleisli"], "=0.3.0-beta")
        self.manifest.write_bytes(source_manifest.read_bytes())
        self.assertEqual(dependency_root(self.metadata, self.manifest), self.source)
        self.manifest.write_text('[dependencies]\nqleisli={workspace=true}\n'
                                 '[workspace.dependencies.qleisli]\nversion="=0.3.0-beta"\n')
        self.assertEqual(dependency_root(self.metadata, self.manifest), self.source)

    def test_workspace_inheritance_never_searches_ancestors_or_repairs_malformed_pins(self):
        invalid = [
            '[dependencies]\nqleisli={workspace=true}\n',
            '[dependencies]\nqleisli={workspace=false}\n[workspace.dependencies]\nqleisli="=0.3.0-beta"\n',
            '[dependencies]\nqleisli={workspace=1}\n[workspace.dependencies]\nqleisli="=0.3.0-beta"\n',
            '[dependencies]\nqleisli={workspace=true,version="=0.3.0-beta"}\n[workspace.dependencies]\nqleisli="=0.3.0-beta"\n',
            '[dependencies]\nqleisli={workspace=true,path="../other"}\n[workspace.dependencies]\nqleisli="=0.3.0-beta"\n',
            '[dependencies]\nqleisli={workspace=true}\n[workspace.dependencies]\nqleisli={workspace=true}\n',
            '[dependencies]\nqleisli={workspace=true}\n[workspace.dependencies]\nqleisli="=0.2.1"\n',
            '[dependencies]\nqleisli={workspace=true}\n[workspace.dependencies]\nqleisli={version="=0.3.0-beta",path="../other"}\n',
        ]
        # A valid enclosing workspace must not fill the explicitly missing same-manifest pin.
        enclosing = self.root / "enclosing"
        enclosing.mkdir()
        (enclosing / "Cargo.toml").write_text('[workspace.dependencies]\nqleisli="=0.3.0-beta"\n')
        nested = enclosing / "nested"
        nested.mkdir()
        missing = nested / "Cargo.toml"
        missing.write_text(invalid[0])
        with self.assertRaisesRegex(RuntimeError, "exact intended"):
            dependency_root(self.metadata, missing)
        for declaration in invalid:
            with self.subTest(declaration=declaration):
                self.manifest.write_text(declaration)
                with self.assertRaises(RuntimeError):
                    dependency_root(self.metadata, self.manifest)

    def test_old_mixed_local_git_and_wrong_root_records_refuse(self):
        changes = []
        for field, value in [("version", "0.2.1"), ("source", None), ("source", "git+https://example.invalid/repo")]:
            value_metadata = copy.deepcopy(self.metadata)
            value_metadata["packages"][1][field] = value
            changes.append(value_metadata)
        value_metadata = copy.deepcopy(self.metadata)
        value_metadata["packages"].append(copy.deepcopy(value_metadata["packages"][1]))
        changes.append(value_metadata)
        value_metadata = copy.deepcopy(self.metadata)
        value_metadata["resolve"]["root"] = "other"
        changes.append(value_metadata)
        value_metadata = copy.deepcopy(self.metadata)
        value_metadata["resolve"]["nodes"][0]["deps"][0]["pkg"] = "other"
        changes.append(value_metadata)
        for changed in changes:
            with self.subTest(changed=changed), self.assertRaises(RuntimeError):
                dependency_root(changed, self.manifest)

    def test_no_local_dependency_declaration_or_missing_link_is_filled_in(self):
        self.manifest.write_text('[dependencies.qleisli]\nversion="=0.3.0-beta"\npath="../other"\n')
        with self.assertRaisesRegex(RuntimeError, "Unsupported"):
            dependency_root(self.metadata, self.manifest)
        self.manifest.write_text('[dependencies]\nqleisli="=0.3.0-beta"\n')
        changed = copy.deepcopy(self.metadata)
        changed["resolve"]["nodes"][0]["deps"] = []
        with self.assertRaisesRegex(RuntimeError, "Missing or duplicate"):
            dependency_root(changed, self.manifest)

    def test_same_version_other_actual_source_and_kernel_source_are_not_inferred(self):
        other = self.root / "other"
        other.mkdir()
        with self.assertRaisesRegex(RuntimeError, "native-provisioning"):
            dependency_root(self.metadata, self.manifest, other)
        path = self.source / "Cargo.toml"
        path.write_text(path.read_text().replace('name="qleisli"', 'name="different"'))
        with self.assertRaisesRegex(RuntimeError, "Actual dependency identity"):
            dependency_root(self.metadata, self.manifest)

    def test_missing_or_mathlib_dependent_native_sources_refuse(self):
        path = self.source / "scripts/package_lean_kernel.py"
        path.unlink()
        with self.assertRaisesRegex(RuntimeError, "lacks native"):
            dependency_root(self.metadata, self.manifest)
        path.write_text("not executed")
        path = self.source / "lean-kernel/lake-manifest.json"
        path.write_text('{"packages":[{"name":"Mathlib"}]}')
        with self.assertRaisesRegex(RuntimeError, "Mathlib-free"):
            dependency_root(self.metadata, self.manifest)
        path.write_text('{"packages":[]}')
        (self.source / "lean-kernel/lean-toolchain").write_text("leanprover/lean4:v4.29.0")
        with self.assertRaisesRegex(RuntimeError, "Mathlib-free"):
            dependency_root(self.metadata, self.manifest)

    def paired_source(self):
        reference = self.root / "explicit-registry-source"
        shutil.copytree(self.source, reference)
        other_manifest = self.root / "bundle/Cargo.toml"
        other_manifest.parent.mkdir()
        other_manifest.write_bytes(self.manifest.read_bytes())
        checksum = "a" * 64
        lock = ('version=4\n[[package]]\nname="qleisli"\nversion="0.3.0-beta"\n'
                'source="registry+https://github.com/rust-lang/crates.io-index"\nchecksum="' + checksum + '"\n')
        (self.manifest.parent / "Cargo.lock").write_text(lock)
        (other_manifest.parent / "Cargo.lock").write_text(lock)
        self.checksums = {label: row["sha256"] for label, row in package_files(self.source).items()}
        (self.source / ".cargo-checksum.json").write_text(json.dumps({"package": checksum, "files": self.checksums}))
        return reference, other_manifest

    def test_complete_vendor_and_registry_payloads_bind_bytes_modes_and_both_locks(self):
        reference, other_manifest = self.paired_source()
        (reference / ".cargo-ok").write_text('{"v":1}')
        self.assertEqual(same_packaged_source(self.source, reference, self.manifest, other_manifest), self.source)
        path = reference / "LICENSE"
        path.write_text("changed")
        with self.assertRaisesRegex(RuntimeError, "bytes/modes differ"):
            same_packaged_source(self.source, reference, self.manifest, other_manifest)
        path.write_bytes((self.source / "LICENSE").read_bytes())
        path.chmod(path.stat().st_mode | 0o111)
        with self.assertRaisesRegex(RuntimeError, "bytes/modes differ"):
            same_packaged_source(self.source, reference, self.manifest, other_manifest)

    def test_extra_missing_symlink_or_forged_vendor_payload_does_not_get_identity_credit(self):
        reference, other_manifest = self.paired_source()
        extra = self.source / "extra.lean"
        extra.write_text("extra")
        with self.assertRaisesRegex(RuntimeError, "all actual package"):
            same_packaged_source(self.source, reference, self.manifest, other_manifest)
        extra.unlink()
        license_bytes = (self.source / "LICENSE").read_bytes()
        (self.source / "LICENSE").unlink()
        with self.assertRaisesRegex(RuntimeError, "all actual package"):
            same_packaged_source(self.source, reference, self.manifest, other_manifest)
        (self.source / "LICENSE").symlink_to(reference / "LICENSE")
        with self.assertRaisesRegex(RuntimeError, "links/special"):
            same_packaged_source(self.source, reference, self.manifest, other_manifest)
        (self.source / "LICENSE").unlink()
        (self.source / "LICENSE").write_bytes(license_bytes)
        index = self.source / ".cargo-checksum.json"
        value = json.loads(index.read_bytes())
        value["files"]["LICENSE"] = "0" * 64
        index.write_text(json.dumps(value))
        with self.assertRaisesRegex(RuntimeError, "all actual package"):
            same_packaged_source(self.source, reference, self.manifest, other_manifest)

    def test_same_version_different_locked_archive_or_local_graph_refuses(self):
        reference, other_manifest = self.paired_source()
        lock = other_manifest.parent / "Cargo.lock"
        original = lock.read_text()
        lock.write_text(original.replace("a" * 64, "b" * 64))
        with self.assertRaisesRegex(RuntimeError, "checksums differ"):
            same_packaged_source(self.source, reference, self.manifest, other_manifest)
        lock.write_text(original.replace('source="registry+https://github.com/rust-lang/crates.io-index"\n', ""))
        with self.assertRaisesRegex(RuntimeError, "exact registry"):
            same_packaged_source(self.source, reference, self.manifest, other_manifest)

    def test_data_cli_requires_the_explicit_comparison_pair_and_forwards_it(self):
        reference, other_manifest = self.paired_source()
        path = self.root / "metadata.json"
        path.write_text(json.dumps(self.metadata))
        base = ["qleisli_dependency.py", "--metadata=" + str(path), "--manifest-path=" + str(self.manifest)]
        with patch("sys.argv", [*base, "--same-package-source-root=" + str(reference)]), \
             self.assertRaisesRegex(RuntimeError, "both explicit"):
            main()
        with patch("sys.argv", [*base, "--same-package-source-root=" + str(reference),
                                "--same-package-manifest-path=" + str(other_manifest)]), \
             patch("builtins.print") as printed:
            main()
            printed.assert_called_once_with(self.source)

    def test_data_cli_uses_only_the_supplied_metadata_and_manifest(self):
        path = self.root / "metadata.json"
        path.write_text(json.dumps(self.metadata))
        with patch("sys.argv", ["qleisli_dependency.py", "--metadata=" + str(path),
                                "--manifest-path=" + str(self.manifest), "--require-source-root=" + str(self.source)]), \
             patch("builtins.print") as printed:
            main()
            printed.assert_called_once_with(self.source)


if __name__ == "__main__":
    unittest.main()
