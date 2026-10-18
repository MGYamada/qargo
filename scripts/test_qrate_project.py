#!/usr/bin/env python3
"""Boundary/identity regressions without invoking Cargo or requiring a compiler."""

import copy
import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import patch

from capture_build import (audit_linux_access, build_binding, capture_kernel, external_tree,
                           generated_inputs, linux_resource_inputs, verify_native_unchanged)
from qrate_project import QRATES, audit, audit_metadata, audit_vendor, copy_tree, extract, sha256, snapshot
from verify_qrate import compare_entrypoints, isolated_config, require_lint_case
from verify_release import verify_versions

SOURCE = Path(__file__).resolve().parents[1]


class ProjectBoundaryTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="qrate-identity-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve() / "project"
        self.root.mkdir()
        for directory in ("qrates", "rust"):
            for name in QRATES:
                copy_tree(SOURCE / directory / name, self.root / directory / name)
        for label in ("Cargo.toml", "src/bin/qlippy.rs", "src/bin/qlifmt.rs", "src/bin/qlidoc.rs",
                      "tests/qlippy.rs", "tests/qlifmt.rs", "tests/qlidoc.rs", "tests/snapshot.rs"):
            target = self.root / label
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(SOURCE / label, target)

    def identity(self):
        return snapshot(self.root, "qlifmt")["project_id"]

    def test_extraction_cannot_recursively_capture_its_own_destination(self):
        destination = self.root / "qrates/qlippy/recursive-copy"
        with self.assertRaisesRegex(RuntimeError, "outside the original checkout"):
            extract(self.root, destination, "qlippy", Path("unused-cargo"))
        self.assertFalse(destination.exists())

    def test_native_configuration_is_captured_and_unrecorded_access_is_rejected(self):
        certificate = self.root / "ca-certificates.crt"
        certificate.write_bytes(b"first certificate bundle")
        before = external_tree(certificate)
        certificate.write_bytes(b"changed certificate bundle")
        self.assertNotEqual(before, external_tree(certificate))
        trace = f'123 openat(AT_FDCWD, "cert", O_RDONLY) = 3<{certificate}>\n'
        audit_linux_access(trace, [self.root])
        trace += '123 openat(AT_FDCWD, "outside", O_RDONLY) = 4</unrecorded/data>\n'
        trace += '123 execve("/unrecorded/compiler", [], []) <unfinished ...>\n'
        with self.assertRaises(RuntimeError) as rejected:
            audit_linux_access(trace, [self.root])
        self.assertIn("Unrecorded native/project read: /unrecorded/data", str(rejected.exception))
        self.assertIn("Unrecorded build executable: /unrecorded/compiler", str(rejected.exception))
        with self.assertRaisesRegex(RuntimeError, "Relative build executable"):
            audit_linux_access('execve("compiler", [], []) = 0', [self.root])

    def test_linux_resource_capture_includes_the_process_cgroup_and_ancestors(self):
        cgroup = self.root / "proc-cgroup"
        cgroup.write_text("0::/system.slice/runner.service\n")
        sys = self.root / "cgroup"
        child = sys / "system.slice/runner.service"
        child.mkdir(parents=True)
        expected = [child / "cpu.max", child.parent / "cpu.max", sys / "cpu.max"]
        for path in expected:
            path.write_text("200000 100000\n")
        self.assertEqual(linux_resource_inputs(cgroup, sys), expected)
        cgroup.write_text("0::/../../outside\n")
        with self.assertRaisesRegex(RuntimeError, "Invalid unified cgroup path"):
            linux_resource_inputs(cgroup, sys)

    def test_linux_exec_audit_distinguishes_failed_probes_and_interleaved_launches(self):
        allowed = self.root / "compiler"
        trace = (
            '10 execve("/absent/emcc", [], []) = -1 ENOENT (No such file)\n'
            '11 execve("/absent/emcc", [], []) <unfinished ...>\n'
            f'12 execve("{allowed}", [], []) <unfinished ...>\n'
            '11 <... execve resumed>) = -1 ENOENT (No such file)\n'
            '12 <... execve resumed>) = 0\n'
        )
        audit_linux_access(trace, [self.root])
        with self.assertRaisesRegex(RuntimeError, "Unrecorded build executable"):
            audit_linux_access(trace.replace('11 <... execve resumed>) = -1 ENOENT (No such file)',
                                             '11 <... execve resumed>) = 0'), [self.root])
        with self.assertRaisesRegex(RuntimeError, "resumed without its entry"):
            audit_linux_access('77 <... execve resumed>) = 0', [self.root])

    def test_linux_directory_descriptors_do_not_admit_file_contents(self):
        trace = (
            '10 open("/", O_RDONLY|O_DIRECTORY) = 3</>\n'
            '11 openat(3</>, "outside", O_RDONLY|O_DIRECTORY <unfinished ...>\n'
            '12 openat(3</>, "outside", O_RDONLY <unfinished ...>\n'
            '11 <... openat resumed>) = 4</outside>\n'
            '12 <... openat resumed>) = -1 ENOENT (No such file)\n'
        )
        audit_linux_access(trace, [self.root])
        with self.assertRaisesRegex(RuntimeError, "Unrecorded native/project read: /outside"):
            audit_linux_access(trace.replace('12 <... openat resumed>) = -1 ENOENT (No such file)',
                                             '12 <... openat resumed>) = 5</outside>'), [self.root])
        with self.assertRaisesRegex(RuntimeError, "Open trace resumed without its entry"):
            audit_linux_access('77 <... openat resumed>) = 0', [self.root])

    def test_relocation_and_excluded_build_cache_preserve_identity(self):
        before = self.identity()
        for label in ("target/old.rs", ".git/config", "rust/qlippy/target/binary", "__pycache__/noise.pyc"):
            path = self.root / label
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("not a project input")
        self.assertEqual(before, self.identity())
        moved = self.root.parent / "moved"
        shutil.copytree(self.root, moved)
        self.assertEqual(before, snapshot(moved, "qlifmt")["project_id"])
        os.utime(moved / "rust/qlifmt/Cargo.toml", (1, 1))
        self.assertEqual(before, snapshot(moved, "qlifmt")["project_id"])

    def test_every_configuration_dependency_helper_and_generated_input_is_bound(self):
        labels = [f"rust/{name}/{file}" for name in ("qlippy", "qlifmt")
                  for file in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo/config.toml", "build.rs", "helpers/generator.py", "generated/source.rs")]
        labels += ["qrates/qlippy/src/report.rs", "qrates/qlifmt/src/smoke.qli", "vendor/example/src/lib.rs"]
        for label in labels:
            with self.subTest(label=label):
                before = self.identity()
                path = self.root / label
                path.parent.mkdir(parents=True, exist_ok=True)
                original = path.read_bytes() if path.exists() else None
                path.write_bytes((original or b"") + b"\n# changed input\n")
                self.assertNotEqual(before, self.identity())
                if original is None:
                    changed = self.identity()
                    path.unlink()
                    self.assertNotEqual(changed, self.identity())
                else:
                    path.write_bytes(original)
                    self.assertEqual(before, self.identity())

    def test_executable_modes_empty_directories_and_own_record(self):
        before = self.identity()
        (self.root / "project-record.json").write_text("its own digest is not an input")
        self.assertEqual(before, self.identity())
        path = self.root / "rust/qlifmt/Cargo.toml"
        path.chmod(0o755)
        self.assertNotEqual(before, self.identity())
        path.chmod(0o644)
        self.assertEqual(before, self.identity())
        (self.root / "rust/qlifmt/empty-input").mkdir()
        self.assertNotEqual(before, self.identity())

    def test_links_and_special_inputs_are_rejected(self):
        link = self.root / "rust/qlifmt/escape"
        link.symlink_to(SOURCE)
        with self.assertRaisesRegex(RuntimeError, "links/special"):
            self.identity()
        link.unlink()
        os.mkfifo(link)
        with self.assertRaisesRegex(RuntimeError, "links/special"):
            self.identity()

    def test_all_declared_dependency_kinds_and_conditional_edges_are_audited(self):
        audit(self.root)
        path = self.root / "rust/qlippy/Cargo.toml"
        original = path.read_text()
        for header in ("dev-dependencies", "build-dependencies", "target.'cfg(target_os = \"none\")'.dependencies"):
            path.write_text(original + '\n[' + header + ']\nrenamed = { package = "qargo", path = "../..", optional = true }\n')
            with self.assertRaisesRegex(RuntimeError, "Reverse qargo dependency"):
                audit(self.root)
        path.write_text(original)
        path.write_text(original + '\n[target.\'cfg(target_os = "none")\'.dependencies]\nqargo = "=0.1.9"\n')
        with self.assertRaisesRegex(RuntimeError, "Reverse qargo dependency"):
            audit(self.root)
        path.write_text(original)

    def test_shared_support_and_canonical_sources_cannot_reverse_direction(self):
        for label, addition in (("qrates/qlifmt/src/lib.rs", "use qlippy_engine::rules::RULES;"),
                                ("qrates/qlifmt/src/lib.rs", "use qlippy_engine as hidden;"),
                                ("qrates/qlidoc/src/lib.rs", "use qlippy_engine::support::adapter;"),
                                ("qrates/qlippy/src/report.rs", "use crate::rules;"),
                                ("qrates/qlippy/src/report.rs", "use crate::{rules};"),
                                ("qrates/qlippy/tests/engine.rs", "use qargo_tools::qargo;"),
                                ("qrates/qlippy/src/lib.rs", 'include!("../../../src/lib.rs");')):
            path = self.root / label
            original = path.read_text()
            path.write_text(original + "\n" + addition)
            with self.assertRaises(RuntimeError):
                audit(self.root)
            path.write_text(original)

    def test_root_adapters_cannot_acquire_semantics(self):
        for label in ("src/bin/qlifmt.rs", "tests/qlippy.rs"):
            path = self.root / label
            original = path.read_text()
            path.write_text(original + '\nfn policy() {}\n')
            with self.assertRaisesRegex(RuntimeError, "adapter"):
                audit(self.root)
            path.write_text(original)

    def test_resolved_graph_rejects_transitive_package_and_target_escapes(self):
        base = {"workspace_root": str(self.root / "rust/qlifmt"), "packages": [
            {"name": "qlifmt-engine", "manifest_path": str(self.root / "rust/qlifmt/Cargo.toml"),
             "targets": [{"src_path": str(self.root / "qrates/qlifmt/src/lib.rs")}], "dependencies": []}]}
        audit_metadata(base, self.root)
        for key, value in (("name", "qargo"), ("manifest_path", str(SOURCE / "Cargo.toml")),
                           ("targets", [{"src_path": str(SOURCE / "src/lib.rs")}] ),
                           ("dependencies", [{"path": str(SOURCE)}])):
            data = copy.deepcopy(base)
            data["packages"][0][key] = value
            with self.assertRaises(RuntimeError):
                audit_metadata(data, self.root)

    def test_every_standard_version_mismatch_fails_without_repair(self):
        version = verify_versions(self.root)
        for name in QRATES:
            for label in (f"qrates/{name}/Qargo.toml", f"rust/{name}/Cargo.toml"):
                path = self.root / label
                original = path.read_text()
                changed = original.replace('version = "' + version + '"', 'version = "9.8.7"', 1)
                path.write_text(changed)
                with self.assertRaisesRegex(RuntimeError, "version mismatch"):
                    verify_versions(self.root)
                self.assertEqual(path.read_text(), changed)
                path.write_text(original)
        with self.assertRaisesRegex(RuntimeError, "Distribution expectation"):
            verify_versions(self.root, "9.8.7")

    def test_build_binding_separates_project_native_choices_generated_and_artifacts(self):
        args = ["sha256:project", {"native_id": "sha256:toolchain-1"},
                {"flags": "", "target": "host", "features": ["default"]},
                [{"path": "build/out/generated.rs", "sha256": "sha256:bytes-1"}], []]
        before = build_binding(*args)["build_environment_id"]
        for index, changed in ((0, "sha256:other-project"), (1, {"native_id": "sha256:toolchain-2"}),
                               (2, {"flags": "-C opt-level=2"}), (2, {"target": "other"}),
                               (2, {"features": ["extra"]}), (3, [{"sha256": "sha256:bytes-2"}])):
            altered = copy.deepcopy(args)
            altered[index] = changed
            self.assertNotEqual(before, build_binding(*altered)["build_environment_id"])
        args[4] = [{"sha256": "sha256:output"}]
        self.assertEqual(before, build_binding(*args)["build_environment_id"])

    def test_ambient_cargo_configuration_is_rejected(self):
        config = self.root.parent / ".cargo/config.toml"
        config.parent.mkdir()
        config.write_text('[build]\nrustflags = ["--cfg", "injected"]\n')
        with self.assertRaisesRegex(RuntimeError, "Ambient ancestor"):
            isolated_config(self.root, "qlifmt", self.root.parent / "work", {})

    def test_vendor_checksums_cover_inactive_dependencies_and_extra_files(self):
        directory = self.root / "vendor/example-1.0.0"
        directory.mkdir(parents=True)
        content = b"locked registry source"
        (directory / "lib.rs").write_bytes(content)
        record = {"package": "archive-checksum", "files": {"lib.rs": sha256(content)[7:]}}
        (directory / ".cargo-checksum.json").write_text(json.dumps(record))
        (self.root / "rust/qlifmt/Cargo.lock").write_text(
            'version = 4\n[[package]]\nname = "example"\nversion = "1.0.0"\n'
            'source = "registry+https://github.com/rust-lang/crates.io-index"\nchecksum = "archive-checksum"\n')
        audit_vendor(self.root, "qlifmt")
        (directory / "lib.rs").write_bytes(b"modified")
        with self.assertRaisesRegex(RuntimeError, "checksum inventory"):
            audit_vendor(self.root, "qlifmt")
        (directory / "lib.rs").write_bytes(content)
        (directory / "extra.rs").write_bytes(content)
        with self.assertRaisesRegex(RuntimeError, "checksum inventory"):
            audit_vendor(self.root, "qlifmt")

    def test_generated_dependency_bytes_are_captured_from_each_build_stage(self):
        target = self.root.parent / "target"
        generated = target / "debug/build/dependency-identity/out/bindings.rs"
        generated.parent.mkdir(parents=True)
        generated.write_text("generated version one")
        before = generated_inputs(target)
        macro = target / "debug/deps/macro.dylib"
        macro.parent.mkdir()
        macro.write_bytes(b"compiled procedural macro")
        self.assertNotEqual(before, generated_inputs(target))
        macro.unlink()
        generated.write_text("generated version two")
        after = generated_inputs(target)
        self.assertNotEqual(before, after)
        self.assertEqual(before[0]["path"], after[0]["path"])


class ExtractionNativeContextTests(unittest.TestCase):
    """Host-only filesystem/response controls; no compiler or native invocation."""

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="qrate-native-context-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.forbid_process = patch("subprocess.Popen", side_effect=AssertionError("No processes in host units"))
        self.forbid_process.start()
        self.addCleanup(self.forbid_process.stop)

    def executable(self):
        path = self.root / "kernel"
        path.write_bytes(b"inert selected executable bytes")
        path.chmod(0o755)
        return path

    def test_kernel_capture_is_canonical_file_only_and_changed_bytes_refuse(self):
        kernel = self.executable()
        alias = self.root / "selected-alias"
        alias.symlink_to(kernel)
        selected, inventory = capture_kernel(alias)
        self.assertEqual(selected, kernel)
        self.assertEqual(len(inventory), 1)
        self.assertEqual((inventory[0]["path"], inventory[0]["kind"], inventory[0]["executable"]),
                         (".", "file", True))
        native = {"inventories": {str(selected): inventory}}
        verify_native_unchanged(native, self.root / "unused-toolchain")
        audit_linux_access(f'1 execve("{selected}", [], []) = 0\n', [selected])
        with self.assertRaisesRegex(RuntimeError, "Unrecorded"):
            audit_linux_access(f'1 openat(AT_FDCWD, "sibling", O_RDONLY) = 3<{self.root / "sibling"}>\n',
                               [selected])
        kernel.write_bytes(b"changed bytes")
        with self.assertRaisesRegex(RuntimeError, "Native inputs changed"):
            verify_native_unchanged(native, self.root / "unused-toolchain")

    def test_missing_directory_nonexecuting_and_special_kernel_refuse_without_reading(self):
        with self.assertRaises(FileNotFoundError):
            capture_kernel(self.root / "missing")
        with self.assertRaisesRegex(RuntimeError, "regular executable"):
            capture_kernel(self.root)
        kernel = self.executable()
        kernel.chmod(0o644)
        with self.assertRaisesRegex(RuntimeError, "regular executable"):
            capture_kernel(kernel)
        if hasattr(os, "mkfifo"):
            fifo = self.root / "pipe"
            os.mkfifo(fifo)
            with self.assertRaisesRegex(RuntimeError, "regular executable"):
                capture_kernel(fifo)

    def test_isolated_environment_keeps_only_the_explicit_kernel_selection(self):
        project = self.root / "project"
        development = project / "rust/qlippy"
        (development / ".cargo").mkdir(parents=True)
        (development / ".cargo/config.toml").write_text(
            '[source.crates-io]\nreplace-with="captured"\n[source.captured]\ndirectory="../../vendor"\n')
        (development / "rust-toolchain.toml").write_text(
            '[toolchain]\nchannel="1.85.0"\nprofile="minimal"\ncomponents=["clippy","rustfmt"]\n')
        work = self.root / "work"
        work.mkdir()
        kernel, _ = capture_kernel(self.executable())
        with patch.dict(os.environ, {"QLEISLI_KERNEL": "/not/the/selected/kernel", "UNADMITTED": "ambient"}):
            environment = isolated_config(project, "qlippy", work, {"QLEISLI_KERNEL": str(kernel)})
        self.assertEqual(environment["QLEISLI_KERNEL"], str(kernel))
        self.assertNotIn("UNADMITTED", environment)

    def test_lint_refusals_cannot_mask_the_actual_positive_or_fault(self):
        def report(fixture):
            failed = fixture in ("syntax", "type")
            check = {"status": "failed", "reason": "compiler_error"} if failed else {
                "status": "not_run", "reason": "no_sources"} if fixture == "empty" else {
                "status": "passed", "reason": None}
            diagnostic = [{"category": "compiler", "severity": "error",
                           "id": "parse" if fixture == "syntax" else "unknown_name"}] if failed else []
            return {"format": "qlippy.result", "version": 1, "command": "lint",
                    "outcome": "error" if failed else "ok", "diagnostics": diagnostic,
                    "result": {"source_count": 0 if fixture == "empty" else 1, "qleisli_check": check}}
        for fixture in ("empty", "valid", "gate", "syntax", "type"):
            envelope = report(fixture)
            result = SimpleNamespace(returncode=1 if fixture in ("syntax", "type") else 0)
            require_lint_case("qlippy", fixture, ["src"], True, result, envelope)
        changed = report("gate")
        changed["result"]["qleisli_check"] = {"status": "failed", "reason": "compiler_error"}
        with self.assertRaisesRegex(RuntimeError, "positive/empty"):
            require_lint_case("qlippy", "gate", ["src"], True, SimpleNamespace(returncode=0), changed)
        for id in ("project", "kernel", "type_mismatch"):
            changed = report("type")
            changed["diagnostics"][0]["id"] = id
            with self.assertRaisesRegex(RuntimeError, "wrong compiler"):
                require_lint_case("qlippy", "type", ["src"], True, SimpleNamespace(returncode=1), changed)
        changed = report("valid")
        changed["result"]["source_count"] = True
        with self.assertRaisesRegex(RuntimeError, "actual source"):
            require_lint_case("qlippy", "valid", ["src"], True, SimpleNamespace(returncode=0), changed)

    def test_comparison_stages_real_manifests_and_keeps_identity_gate_empty_and_faults(self):
        seen = set()
        digest = "sha256:" + "a" * 64
        originals = {"valid": "pub unitary fn identity(q:Q<Bit>)->Q<Bit>{q}",
                     "gate": "pub unitary fn had(q:Q<Bit>)->Q<Bit>{h(q)}",
                     "syntax": "not a module {{{",
                     "type": "pub unitary fn invalid()->Q<Bit>{missing()}"}

        def fake_run(command, cwd, **kwargs):
            source = cwd / "src/smoke.qli"
            fixture = next((name for name, text in originals.items()
                            if source.exists() and source.read_text() == text), "empty")
            seen.add(fixture)
            manifest_path = cwd / "Qargo.toml"
            self.assertEqual(manifest_path.exists(), fixture != "empty")
            if fixture != "empty":
                data = json.loads(json.dumps(__import__("tomllib").loads(manifest_path.read_text())))
                self.assertEqual((data["schema-version"], data["qrate"]["edition"]), (2, "2026"))
            self.assertEqual(kwargs["env"]["QLEISLI_KERNEL"], "/explicit/selected/kernel")
            args = command[1:]
            lint = args in (["src"], ["src", "--deny-warnings"],
                            ["src", "--format=json"], ["src", "--deny-warnings", "--format=json"])
            failed = lint and fixture in ("syntax", "type")
            check = {"status": "failed", "reason": "compiler_error"} if failed else {
                "status": "not_run", "reason": "no_sources"} if fixture == "empty" else {
                "status": "passed", "reason": None}
            diagnostics = [{"id": "parse" if fixture == "syntax" else "unknown_name",
                            "category": "compiler", "severity": "error"}] if failed else []
            envelope = {"format": "qlippy.result", "version": 1, "command": "lint" if lint else "control",
                        "outcome": "error" if failed else "ok", "diagnostics": diagnostics,
                        "result": {"source_count": 0 if fixture == "empty" else 1,
                                   "qleisli_check": check, "tool": {"executable_sha256": digest}}}
            return SimpleNamespace(returncode=1 if failed else 0, stderr=b"",
                                   stdout=(json.dumps(envelope).encode() if "--format=json" in args else b"same text"))
        with patch("verify_qrate.subprocess.run", side_effect=fake_run), \
             patch("verify_qrate.file_digest", return_value=digest):
            compare_entrypoints("qlippy", self.root / "standalone", self.root / "bundled", self.root,
                                {"QLEISLI_KERNEL": "/explicit/selected/kernel"})
        self.assertEqual(seen, {"empty", *originals})


if __name__ == "__main__":
    unittest.main()
