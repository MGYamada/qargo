#!/usr/bin/env python3
"""Host-only release-metadata checks; no Cargo, Qleisli, or kernel invocation."""

import copy
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import verify_package
import verify_release as verifier


class ReleaseMetadataTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="release-verifier-host-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        for label, content in {
            "Cargo.toml": '[package]\nname="qleisli"\nversion="0.3.0-beta"\n',
            "stdlib/Qargo.toml": 'schema-version=2\n[qrate]\nname="std"\nversion="0.3.0-beta"\nedition="2026"\n',
            "stdlib/src/a.qli": "unitary fn a(q:Q<Bit>)->Q<Bit>{q}\n",
            "stdlib/src/b.qli": "unitary fn b(q:Q<Bit>)->Q<Bit>{h(q)}\n",
            "src/frontend/source.rs": '''impl BundledRegistry {
    pub fn manifest() -> &'static str { include_str!("../../stdlib/Qargo.toml") }
    pub fn sources() -> &'static [BundledSource] {
        static SOURCES: &[BundledSource] = &[
            BundledSource {
                name: "std::b",
                path: "<bundled>/std/b.qli",
                text: include_str!("../../stdlib/src/b.qli"),
            },
            BundledSource {
                name: "std::a",
                path: "<bundled>/std/a.qli",
                text: include_str!("../../stdlib/src/a.qli"),
            },
        ];
        SOURCES
    }
}
''',
        }.items():
            destination = self.root / label
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_text(content)
        self.std, self.pins = verifier.expected_std(self.root)
        self.configuration = {"Qlirgo.toml": b'schema-version=2\n[qrate]\nedition="2026"\n'}
        self.checker = Path("/explicit/native-checker")
        # Synthetic diagnostic metadata only; these are not native decisions.
        self.native = {"checker_identity": "unpinned", "checker_paths": [str(self.checker)],
                       "count": 32, "requests_checked": 2, "complete": True,
                       "entry_request_checked": None, "concrete_ir_roots": {"local": 1, "bundled": 24},
                       "source_preservation_verified": False}
        self.result = {"source_count": 1, "effective_std": self.std,
                       "source_configuration_id": verifier.input_id("qlirgo.source-configuration.v1", self.configuration),
                       "native_decisions": self.native}

    def validate(self, result):
        verifier.require_environment(result, self.std, self.configuration, self.checker, 1)

    def test_complete_independent_inventory_uses_sorted_canonical_names_and_raw_bytes(self):
        self.assertEqual([row["name"] for row in self.std["std"]["modules"]], ["std::a", "std::b"])
        original = self.std["std"]["content_id"]
        for label in ("stdlib/Qargo.toml", "stdlib/src/a.qli"):
            with self.subTest(label=label):
                path = self.root / label
                old = path.read_bytes()
                path.write_bytes(old + b" ")
                changed, pins = verifier.expected_std(self.root)
                self.assertNotEqual(changed["std"]["content_id"], original)
                self.assertNotEqual(pins, self.pins)
                path.write_bytes(old)

    def test_missing_extra_and_unsupported_registry_entries_fail_closed(self):
        path = self.root / "stdlib/src/extra.qli"
        path.write_bytes(b"extra")
        with self.assertRaisesRegex(RuntimeError, "complete std"):
            verifier.expected_std(self.root)
        path.unlink()
        path = self.root / "stdlib/src/a.qli"
        old = path.read_bytes()
        path.unlink()
        with self.assertRaisesRegex(RuntimeError, "Nonregular bundled"):
            verifier.expected_std(self.root)
        path.write_bytes(old)
        path = self.root / "src/frontend/source.rs"
        path.write_text(path.read_text().replace('name: "std::a"', 'name: computed_name()'))
        with self.assertRaisesRegex(RuntimeError, "Unsupported or incomplete"):
            verifier.expected_std(self.root)

    def test_duplicate_noncanonical_and_symlink_modules_fail_closed(self):
        path = self.root / "src/frontend/source.rs"
        original = path.read_text()
        path.write_text(original.replace('name: "std::a"', 'name: "std::b"'))
        with self.assertRaisesRegex(RuntimeError, "Noncanonical|Duplicate"):
            verifier.expected_std(self.root)
        path.write_text(original)
        source = self.root / "stdlib/src/a.qli"
        source.unlink()
        source.symlink_to(self.root / "stdlib/src/b.qli")
        with self.assertRaisesRegex(RuntimeError, "Nonregular"):
            verifier.expected_std(self.root)

    def test_full_prerelease_pair_and_explicit_edition_are_not_inferred(self):
        path = self.root / "stdlib/Qargo.toml"
        original = path.read_text()
        for old, new in [('version="0.3.0-beta"', 'version="0.3.0"'),
                         ('version="0.3.0-beta"', 'version="0.3.0-beta+other"'),
                         ('edition="2026"', 'edition="2027"'),
                         ('edition="2026"', ''), ('name="std"', 'name="other"')]:
            with self.subTest(new=new):
                path.write_text(original.replace(old, new))
                with self.assertRaisesRegex(RuntimeError, "full Qleisli/std pair"):
                    verifier.expected_std(self.root)
        path.write_text(original)

    def test_same_version_wrong_package_name_is_not_the_intended_qleisli_source(self):
        path = self.root / "Cargo.toml"
        path.write_text(path.read_text().replace('name="qleisli"', 'name="unrelated"'))
        with self.assertRaisesRegex(RuntimeError, "not Qleisli"):
            verifier.expected_std(self.root)

    def test_explicit_symlink_kernel_is_normalized_in_actual_execution_environment(self):
        real = self.root / "kernel"
        real.write_bytes(b"not executed")
        real.chmod(0o700)
        alias = self.root / "selected-kernel"
        alias.symlink_to(real)
        original = str(alias.absolute())
        with patch.dict(os.environ, {"QLEISLI_KERNEL": original}):
            checker, environment = verifier.native_environment()
            self.assertEqual(checker, real.resolve())
            self.assertEqual(environment["QLEISLI_KERNEL"], str(checker))
            self.assertNotEqual(environment["QLEISLI_KERNEL"], original)
            self.assertEqual(os.environ["QLEISLI_KERNEL"], original)

    def test_exact_file_map_does_not_replace_empty_snapshot_directory_inventory(self):
        snapshot = self.root / "snapshot"
        snapshot.mkdir()
        expected = {"Qlirgo.toml": b"captured manifest"}
        (snapshot / "Qlirgo.toml").write_bytes(expected["Qlirgo.toml"])
        for name in ("src", "tests", "docs"):
            (snapshot / name).mkdir()
        verifier.verify_snapshot(snapshot, expected)
        (snapshot / "tests").rmdir()
        self.assertEqual(verifier.files_under(snapshot), expected)
        with self.assertRaisesRegex(RuntimeError, "Missing snapshot root: tests"):
            verifier.verify_snapshot(snapshot, expected)

    def test_managed_configuration_preserves_root_override_nested_bytes_and_owning_fallback(self):
        owning = b'[source]\nroot="src"\n'
        expected = {"Qlirgo.toml": owning, "src/a.qli": b"a", "src/sub/Qlirgo.toml": b"nested"}
        self.assertEqual(verifier.source_configuration(expected),
                         {"Qlirgo.toml": owning, "sub/Qlirgo.toml": b"nested"})
        expected["src/Qlirgo.toml"] = b"root override"
        self.assertEqual(verifier.source_configuration(expected),
                         {"Qlirgo.toml": b"root override", "sub/Qlirgo.toml": b"nested"})
        self.assertNotIn("src/a.qli", verifier.source_configuration(expected))

    def test_environment_rejects_stale_missing_extra_reordered_and_changed_constituents(self):
        self.validate(self.result)
        mutations = []
        for field in ("qleisli_version",):
            changed = copy.deepcopy(self.result)
            changed["effective_std"][field] = "0.2.1"
            mutations.append(changed)
        for field in ("version", "manifest_sha256", "content_id"):
            changed = copy.deepcopy(self.result)
            changed["effective_std"]["std"][field] = "stale"
            mutations.append(changed)
        for transform in (lambda rows: rows.pop(), lambda rows: rows.append({"name": "std::extra", "sha256": "stale"}),
                          lambda rows: rows.reverse(), lambda rows: rows[0].update(sha256="stale")):
            changed = copy.deepcopy(self.result)
            transform(changed["effective_std"]["std"]["modules"])
            mutations.append(changed)
        changed = copy.deepcopy(self.result)
        changed["effective_std"]["unexpected"] = True
        mutations.append(changed)
        changed = copy.deepcopy(self.result)
        changed["source_configuration_id"] = "sha256:" + "0" * 64
        mutations.append(changed)
        for changed in mutations:
            with self.subTest(changed=changed), self.assertRaises(RuntimeError):
                self.validate(changed)

    def test_native_fields_types_scope_paths_and_counter_relations_remain_exact(self):
        self.validate(self.result)
        for field, value in [("count", True), ("count", -1), ("count", 2**64),
                             ("requests_checked", 0), ("requests_checked", 33), ("requests_checked", 1.0),
                             ("complete", 1), ("complete", False), ("entry_request_checked", False),
                             ("source_preservation_verified", True), ("checker_identity", "attested"),
                             ("checker_paths", [str(self.checker)] * 2),
                             ("checker_paths", ["/wrong/checker"]),
                             ("concrete_ir_roots", {"local": True, "bundled": 24})]:
            changed = copy.deepcopy(self.result)
            changed["native_decisions"][field] = value
            with self.subTest(field=field, value=value), self.assertRaises(RuntimeError):
                self.validate(changed)
        for field in self.native:
            changed = copy.deepcopy(self.result)
            del changed["native_decisions"][field]
            with self.subTest(missing=field), self.assertRaises(RuntimeError):
                self.validate(changed)
        changed = copy.deepcopy(self.result)
        changed["native_decisions"]["extra"] = False
        with self.assertRaises(RuntimeError):
            self.validate(changed)

    def test_empty_metadata_has_explicit_null_native_and_no_accepted_program_claim(self):
        result = copy.deepcopy(self.result)
        result.update(source_count=0, native_decisions=None)
        verifier.require_environment(result, self.std, self.configuration, self.checker, 0)
        for value in ({}, self.native):
            result["native_decisions"] = value
            with self.assertRaises(RuntimeError):
                verifier.require_environment(result, self.std, self.configuration, self.checker, 0)
        del result["native_decisions"]
        with self.assertRaises(RuntimeError):
            verifier.require_environment(result, self.std, self.configuration, self.checker, 0)

    def test_parent_lint_binds_only_configuration_and_closed_children_gain_no_environment(self):
        cfg = self.result["source_configuration_id"]
        verifier.require_no_environment({}, None)
        verifier.require_no_environment({"source_configuration_id": cfg}, self.configuration)
        for result, config in [({"effective_std": self.std}, None),
                               ({"native_decisions": None}, None),
                               ({"source_configuration_id": cfg}, None),
                               ({"source_configuration_id": "stale"}, self.configuration)]:
            with self.subTest(result=result), self.assertRaises(RuntimeError):
                verifier.require_no_environment(result, config)

    def record(self, count=1):
        check = {"status": "passed", "reason": None} if count else {"status": "not_run", "reason": "no_sources"}
        return {"format": "qlirgo.build-record", "version": 2, "input_id": "input",
                "qrate": {"name": "sample", "version": verifier.QRATE_VERSION}, "source_count": count,
                "qleisli_check": check, "profile": "finite-v0", "tool": {"name": "qlirgo"},
                "effective_std": self.std, "source_configuration_id": self.result["source_configuration_id"],
                "native_decisions": self.native if count else None,
                "steps": [{"name": "manifest", "status": "passed", "reason": None},
                          {"name": "snapshot", "status": "passed", "reason": None},
                          {"name": "qleisli", **check},
                          {"name": "QLT", "status": "not_run", "reason": "backend_unavailable"},
                          {"name": "qlidoc", "status": "not_run", "reason": "not_requested"},
                          {"name": "build", "status": "passed", "reason": None}]}

    def test_build_record_is_exact_version_two_and_rejects_mixed_context_even_equal_bool_numbers(self):
        for count in (0, 1):
            record = self.record(count)
            result = {key: record[key] for key in ("effective_std", "source_configuration_id", "native_decisions")}
            validate = lambda value: verifier.verify_build_record(value, "sample", "input", count, {"name": "qlirgo"}, result)
            validate(record)
            mutations = []
            for field, value in [("version", 1), ("source_count", bool(count)),
                                 ("source_configuration_id", "stale"), ("effective_std", {}),
                                 ("native_decisions", {}), ("steps", [])]:
                changed = copy.deepcopy(record)
                changed[field] = value
                mutations.append(changed)
            for field in ("effective_std", "native_decisions", "source_configuration_id"):
                changed = copy.deepcopy(record)
                del changed[field]
                mutations.append(changed)
            changed = copy.deepcopy(record)
            changed["unexpected"] = True
            mutations.append(changed)
            if count:
                changed = copy.deepcopy(record)
                changed["native_decisions"]["count"] = 32.0
                mutations.append(changed)
            for changed in mutations:
                with self.subTest(count=count, changed=changed), self.assertRaises(RuntimeError):
                    validate(changed)

    def test_result_versions_are_explicit_parent_two_child_one_not_self_reported(self):
        for name in verifier.TOOLS:
            version = 2 if name == "qlirgo" else 1
            report = {"format": name + ".result", "version": version, "outcome": "ok", "diagnostics": []}
            def output(value):
                return type("Output", (), {"returncode": 0, "stderr": b"", "stdout": json.dumps(value).encode() + b"\n"})()
            with patch.object(verifier.subprocess, "run", return_value=output(report)) as run:
                self.assertEqual(verifier.invoke(Path("/binary"), ["--version"], {}, name), report)
                self.assertEqual(run.call_args.args[0], ["/binary", "--version", "--format=json"])
            for wrong in (3 - version, True):
                report["version"] = wrong
                with patch.object(verifier.subprocess, "run", return_value=output(report)), self.assertRaises(RuntimeError):
                    verifier.invoke(Path("/binary"), [], {}, name)

    def test_duplicate_fields_fail_closed(self):
        with self.assertRaisesRegex(RuntimeError, "Repeated JSON field"):
            json.loads('{"count":1,"count":2}', object_pairs_hook=verifier.unique_object)

    def test_final_unavailable_empty_test_cannot_hide_a_late_native_invocation(self):
        marker = self.root / "empty-native-was-started"
        report = {"diagnostics": [{"id": "backend_unavailable"}]}
        with patch.object(verifier, "invoke", return_value=report) as invoke:
            verifier.verify_empty_test(Path("/qlirgo"), Path("/Qlirgo.toml"), {}, marker)
            invoke.assert_called_once_with(Path("/qlirgo"), ["test", "--manifest-path", "/Qlirgo.toml"], {}, "qlirgo", expected_exit=1)
        def late_marker(*args, **kwargs):
            marker.write_bytes(b"started")
            return report
        with patch.object(verifier, "invoke", side_effect=late_marker), self.assertRaisesRegex(RuntimeError, "invoked the native checker"):
            verifier.verify_empty_test(Path("/qlirgo"), Path("/Qlirgo.toml"), {}, marker)

    def test_missing_dependency_source_binding_never_starts_runtime_or_cargo(self):
        with patch.object(verifier.subprocess, "run") as run:
            with self.assertRaisesRegex(RuntimeError, "explicit --qleisli-source-root"):
                verifier.verify(self.root, self.root)
            with self.assertRaisesRegex(RuntimeError, "explicit --qleisli-source-root"):
                verify_package.verify_package(self.root, self.root / "archive", self.root)
            run.assert_not_called()

    def test_nonempty_verifier_requires_explicit_native_selection_before_runtime(self):
        with patch.dict(os.environ, {}, clear=True), patch.object(verifier.subprocess, "run") as run:
            with self.assertRaisesRegex(RuntimeError, "Set QLEISLI_KERNEL"):
                verifier.verify(self.root, self.root, self.root)
            run.assert_not_called()

    def test_developer_package_cli_forwards_explicit_source_association(self):
        with patch("sys.argv", ["verify_package.py", "--source-root=/qlirgo", "--crate=/qlirgo.crate",
                                "--bin-dir=/bins", "--qleisli-source-root=/intended-qleisli"]), \
             patch.object(verify_package, "verify_package") as verify:
            verify_package.main()
            verify.assert_called_once_with(Path("/qlirgo"), Path("/qlirgo.crate"), Path("/bins"), Path("/intended-qleisli"))


class ProductReleaseVersionTests(unittest.TestCase):
    def test_product_prerelease_and_qrate_version_are_separate(self):
        from release_version import qrate_version, is_beta
        for value, expected in [("0.2.0", "0.2.0"), ("0.2.0-beta", "0.2.0"), ("0.2.0-beta.1", "0.2.0"), ("12.30.4-beta.27", "12.30.4")]:
            self.assertEqual(qrate_version(value), expected)
            self.assertEqual(is_beta(value), "-beta" in value)
        for invalid in ["v0.2.0", "00.2.0", "0.02.0", "0.2.0-beta.0", "0.2.0-beta.01", "0.2.0-beta.", "0.2.0-beta.1/evil", "0.2.0-rc.1", "0.2.0+metadata", "0.2.0\n"]:
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                qrate_version(invalid)


if __name__ == "__main__":
    unittest.main()
