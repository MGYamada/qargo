#!/usr/bin/env python3
"""Distribution integrity checks using generated executable headers and temporary files."""

import hashlib
import io
from pathlib import Path
import struct
import tarfile
import tempfile
import unittest
from unittest.mock import patch

from distribution import TARGETS, archive_files, binary_platform, build, bundle_name, collect
from verify_release import TOOLS


def executable(target, *, interpreter=False, dynamic=False, dependency=None, minimum=11):
    if target.endswith("linux-musl"):
        content = bytearray(256)
        content[:6] = b"\x7fELF\x02\x01"
        struct.pack_into("<H", content, 18, 62 if target.startswith("x86_64") else 183)
        struct.pack_into("<Q", content, 32, 64)
        struct.pack_into("<HH", content, 54, 56, 1 if interpreter or dynamic else 0)
        if interpreter or dynamic:
            struct.pack_into("<I", content, 64, 3 if interpreter else 2)
        if dynamic:
            struct.pack_into("<Q", content, 72, 128)
            struct.pack_into("<Q", content, 96, 32)
            struct.pack_into("<q", content, 128, 1)
        return bytes(content)
    commands = struct.pack("<6I", 0x32, 24, 1, minimum << 16, 14 << 16, 0)
    if dependency:
        name = dependency.encode() + b"\0"
        size = (24 + len(name) + 7) // 8 * 8
        commands += struct.pack("<6I", 0xC, size, 24, 0, 0, 0) + name.ljust(size - 24, b"\0")
    content = bytearray(32)
    struct.pack_into("<I", content, 0, 0xFEEDFACF)
    struct.pack_into("<I", content, 4, 0x01000007 if target.startswith("x86_64") else 0x0100000C)
    struct.pack_into("<II", content, 16, 2 if dependency else 1, len(commands))
    return bytes(content + commands).ljust(64, b"\0")


class DistributionTests(unittest.TestCase):
    def setUp(self):
        printer = patch("builtins.print")
        printer.start()
        self.addCleanup(printer.stop)
        self.temporary = tempfile.TemporaryDirectory(prefix="qargo-distribution-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.source = self.root / "source"
        self.source.mkdir()
        for name in ("LICENSE", "NOTICE", "install.sh"):
            (self.source / name).write_text(name + "\n")

    def bundle(self, target, directory=None):
        binaries = self.root / (target + "-bin")
        binaries.mkdir(exist_ok=True)
        for tool in TOOLS:
            (binaries / tool).write_bytes(executable(target))
        directory = directory or self.root / target
        with patch("distribution.verify") as verifier:
            build(self.source, binaries, target, directory)
            verifier.assert_called_once_with(self.source, binaries)
        return directory / (bundle_name(target) + ".tar.gz")

    def test_all_target_headers_and_cpu_rejection(self):
        for target in TARGETS:
            binary_platform(executable(target), target)
            wrong = target.replace("aarch64", "x86_64") if target.startswith("aarch64") else target.replace("x86_64", "aarch64")
            with self.assertRaises(RuntimeError):
                binary_platform(executable(target), wrong)
        for invalid in (b"", b"MZ" + bytes(80)):
            with self.assertRaises(RuntimeError):
                binary_platform(invalid, TARGETS[0])

    def test_linux_requires_no_interpreter_or_dynamic_libraries(self):
        for target in (target for target in TARGETS if target.endswith("linux-musl")):
            for option in ("interpreter", "dynamic"):
                with self.assertRaises(RuntimeError):
                    binary_platform(executable(target, **{option: True}), target)

    def test_macos_requires_11_and_only_system_libraries(self):
        for target in (target for target in TARGETS if target.endswith("apple-darwin")):
            binary_platform(executable(target, dependency="/usr/lib/libSystem.B.dylib"), target)
            for dependency in ("@rpath/development.dylib", "/opt/homebrew/lib/unavailable.dylib"):
                with self.assertRaises(RuntimeError):
                    binary_platform(executable(target, dependency=dependency), target)
            with self.assertRaises(RuntimeError):
                binary_platform(executable(target, minimum=14), target)

    def test_archive_is_reproducible_and_refuses_replacement(self):
        first = self.bundle(TARGETS[0], self.root / "first")
        second = self.bundle(TARGETS[0], self.root / "second")
        self.assertEqual(first.read_bytes(), second.read_bytes())
        files = archive_files(self.source, first, TARGETS[0])
        self.assertEqual(set(files), {"LICENSE", "NOTICE", *["bin/" + tool for tool in TOOLS]})
        with self.assertRaises(FileExistsError):
            self.bundle(TARGETS[0], self.root / "first")

    def test_archive_rejects_duplicate_path_symlink_permissions_and_wrong_license(self):
        original = self.bundle(TARGETS[0])
        with tarfile.open(original) as archive:
            original_entries = [(entry, archive.extractfile(entry).read()) for entry in archive]
        for mutation in ("duplicate", "symlink", "permissions", "license", "path", "missing"):
            path = self.root / (mutation + ".tar.gz")
            with tarfile.open(path, "w:gz", format=tarfile.USTAR_FORMAT) as archive:
                for index, (entry, content) in enumerate(original_entries):
                    entry = tarfile.TarInfo.frombuf(entry.tobuf(), "utf-8", "strict")
                    if index == 0:
                        if mutation == "symlink":
                            entry.type, entry.linkname, entry.size = tarfile.SYMTYPE, "/tmp/outside", 0
                        elif mutation == "permissions":
                            entry.mode = 0o777
                        elif mutation == "license":
                            content = b"wrong license"
                            entry.size = len(content)
                        elif mutation == "path":
                            entry.name = "../escape"
                        elif mutation == "missing":
                            continue
                    archive.addfile(entry, io.BytesIO(content))
                    if index == 0 and mutation == "duplicate":
                        archive.addfile(entry, io.BytesIO(content))
            with self.assertRaises(RuntimeError, msg=mutation):
                archive_files(self.source, path, TARGETS[0])

    def test_collect_requires_complete_unique_targets_and_hashes_installer(self):
        inputs = self.root / "inputs"
        for target in TARGETS:
            self.bundle(target, inputs / target)
        output = self.root / "output"
        collect(self.source, inputs, output)
        expected = {bundle_name(target) + ".tar.gz" for target in TARGETS} | {"install.sh", "SHA256SUMS"}
        self.assertEqual({path.name for path in output.iterdir()}, expected)
        sums = (output / "SHA256SUMS").read_text().splitlines()
        self.assertEqual(len(sums), len(TARGETS) + 1)
        for line in sums:
            digest, name = line.split("  ")
            self.assertEqual(digest, hashlib.sha256((output / name).read_bytes()).hexdigest())
        duplicate = inputs / "duplicate"
        duplicate.mkdir()
        source = inputs / TARGETS[0] / (bundle_name(TARGETS[0]) + ".tar.gz")
        (duplicate / source.name).write_bytes(source.read_bytes())
        with self.assertRaises(RuntimeError):
            collect(self.source, inputs, self.root / "invalid")
        (duplicate / source.name).unlink()
        source.unlink()
        with self.assertRaises(RuntimeError):
            collect(self.source, inputs, self.root / "incomplete")


if __name__ == "__main__":
    unittest.main()
