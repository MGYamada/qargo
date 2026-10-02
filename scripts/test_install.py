#!/usr/bin/env python3
"""Installer boundary tests using temporary bundles and local HTTPS response fixtures."""

import hashlib
import io
import os
from pathlib import Path
import platform
import shlex
import shutil
import subprocess
import tarfile
import tempfile
import unittest

from verify_release import PRODUCT_VERSION, TOOLS


ROOT = Path(__file__).resolve().parent.parent


def host_target():
    cpu = "aarch64" if platform.machine() in ("arm64", "aarch64") else "x86_64"
    return cpu + ("-apple-darwin" if platform.system() == "Darwin" else "-unknown-linux-musl")


class InstallerFixture:
    def __init__(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="qargo-installer-test-")
        self.root = Path(self.temporary.name)
        self.prefix = self.root / "prefix with spaces"
        self.downloads = self.root / "downloads"
        self.commands = self.root / "commands"
        self.commands.mkdir()
        self.requests = self.root / "requests"
        self.marker = self.root / "unexpected-developer-tool"
        self.environment = dict(os.environ)
        self.environment.update({
            "PATH": str(self.commands) + os.pathsep + os.environ["PATH"],
            "QARGO_INSTALL_FIXTURES": str(self.downloads),
            "QARGO_INSTALL_REQUESTS": str(self.requests),
            "QARGO_TEST_LATEST_TAG": "v" + PRODUCT_VERSION,
            "QARGO_INSTALL_MARKER": str(self.marker),
        })
        self.command("curl", """#!/bin/sh
url=
output=
next=
for argument do
    if [ "$next" = output ]; then output=$argument; next=; continue; fi
    case "$argument" in
        -o) next=output ;;
        https://*) url=$argument ;;
    esac
done
printf '%s\\n' "$url" >> "$QARGO_INSTALL_REQUESTS"
case "$url" in *"${QARGO_TEST_FAIL_DOWNLOAD:-impossible-failure-pattern}"*) exit 22 ;; esac
if [ "$url" = https://github.com/MGYamada/qargo/releases/latest ]; then
    printf 'https://github.com/MGYamada/qargo/releases/tag/%s' "$QARGO_TEST_LATEST_TAG"
    exit 0
fi
case "$url" in
    https://github.com/MGYamada/qargo/releases/download/*)
        relative=${url#https://github.com/MGYamada/qargo/releases/download/}
        cp "$QARGO_INSTALL_FIXTURES/$relative" "$output"
        ;;
    *) exit 22 ;;
esac
""")
        real_uname = shlex.quote(shutil.which("uname"))
        self.command("uname", f"""#!/bin/sh
case "$1" in
    -s) if [ -n "${{QARGO_TEST_OS:-}}" ]; then printf '%s\\n' "$QARGO_TEST_OS"; exit; fi ;;
    -m) if [ -n "${{QARGO_TEST_ARCH:-}}" ]; then printf '%s\\n' "$QARGO_TEST_ARCH"; exit; fi ;;
esac
exec {real_uname} "$@"
""")
        self.command("sysctl", "#!/bin/sh\nprintf '%s\\n' \"${QARGO_TEST_ROSETTA:-0}\"\n")
        real_mv = shlex.quote(shutil.which("mv"))
        switch_flag = "-fh" if platform.system() == "Darwin" else "-fT"
        self.command("mv", f"""#!/bin/sh
case "$1" in
    -fT|-fh)
        [ "${{QARGO_TEST_FAIL_SWITCH:-no}}" != yes ] || exit 1
        shift
        exec {real_mv} {switch_flag} "$@"
        ;;
esac
exec {real_mv} "$@"
""")
        for name in ("cargo", "rustc", "rustdoc", "python3", "jq"):
            self.command(name, "#!/bin/sh\nprintf started > \"$QARGO_INSTALL_MARKER\"\nexit 99\n")

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.temporary.cleanup()

    def command(self, name, source):
        path = self.commands / name
        path.write_text(source)
        path.chmod(0o755)

    def add_archive(self, version, target, archive_path):
        destination = self.downloads / ("v" + version)
        destination.mkdir(parents=True, exist_ok=True)
        name = f"qargo-{version}-{target}.tar.gz"
        shutil.copyfile(archive_path, destination / name)
        digest = hashlib.sha256((destination / name).read_bytes()).hexdigest()
        (destination / "SHA256SUMS").write_text(digest + "  " + name + "\n")

    def release(self, version=PRODUCT_VERSION, target=None, *, extra=None, link=None,
                duplicate=False, wrong_version=False, nonexecutable=False, absolute=False,
                unsafe_mode=False):
        target = target or host_target()
        name = f"qargo-{version}-{target}"
        directory = self.downloads / ("v" + version)
        directory.mkdir(parents=True, exist_ok=True)
        path = directory / (name + ".tar.gz")
        with tarfile.open(path, "w:gz", format=tarfile.USTAR_FORMAT) as archive:
            files = {"LICENSE": b"license\n", "NOTICE": b"notice\n"}
            for tool in TOOLS:
                reported = "9.9.9" if wrong_version and tool == "qlidoc" else version
                files["bin/" + tool] = f"#!/bin/sh\nprintf '%s\\n' '{tool} {reported} (Qleisli 0.2.1, finite-v0)'\n".encode()
            for label, data in files.items():
                entry = tarfile.TarInfo(name + "/" + label)
                if absolute and label == "NOTICE":
                    entry.name = "/" + entry.name
                entry.mode = 0o755 if label.startswith("bin/") else 0o644
                if nonexecutable and label == "bin/qlidoc":
                    entry.mode = 0o644
                if unsafe_mode and label == "bin/qlidoc":
                    entry.mode = 0o777
                if link and label == "NOTICE":
                    entry.type, entry.linkname = link, "/tmp/qargo-should-never-be-read"
                    archive.addfile(entry)
                else:
                    entry.size = len(data)
                    archive.addfile(entry, io.BytesIO(data))
                    if duplicate and label == "NOTICE":
                        archive.addfile(entry, io.BytesIO(data))
            if extra:
                entry = tarfile.TarInfo(extra)
                entry.size = 4
                entry.mode = 0o644
                archive.addfile(entry, io.BytesIO(b"evil"))
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        (directory / "SHA256SUMS").write_text(digest + "  " + path.name + "\n")
        return path

    def run(self, *args, environment=None, stdin=False, use_default_prefix=False):
        env = dict(self.environment)
        if environment:
            env.update(environment)
        prefix_args = [] if use_default_prefix else ["--prefix", str(self.prefix)]
        if stdin:
            command = ["/bin/sh", "-s", "--", *prefix_args, *args]
            source = (ROOT / "install.sh").read_text()
        else:
            command = ["/bin/sh", str(ROOT / "install.sh"), *prefix_args, *args]
            source = None
        return subprocess.run(command, env=env, input=source, text=True,
                              capture_output=True, timeout=30)

    def current(self):
        return (self.prefix / "lib/qargo/current").readlink().as_posix()


class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.fixture = InstallerFixture()
        self.addCleanup(self.fixture.temporary.cleanup)

    def success(self, result):
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertFalse(self.fixture.marker.exists(), "Installer started a developer tool")

    def failure(self, result):
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertFalse(self.fixture.marker.exists())
        self.assertFalse((self.fixture.prefix / "lib/qargo/.install-lock").exists())

    def test_first_install_latest_repeated_and_stdin(self):
        self.fixture.release()
        self.success(self.fixture.run(stdin=True))
        before = self.fixture.current()
        self.success(self.fixture.run())
        self.assertEqual(self.fixture.current(), before)
        for tool in TOOLS:
            path = self.fixture.prefix / "bin" / tool
            self.assertEqual(path.readlink().as_posix(), "../lib/qargo/current/bin/" + tool)
            self.assertTrue(os.access(path, os.X_OK))
        requests = self.fixture.requests.read_text().splitlines()
        self.assertEqual(requests.count("https://github.com/MGYamada/qargo/releases/latest"), 2)
        self.assertTrue(all("/download/v" + PRODUCT_VERSION + "/" in url
                            for url in requests if not url.endswith("/latest")))

    def test_explicit_version_update_and_downgrade(self):
        self.fixture.release("0.1.4")
        self.fixture.release()
        self.success(self.fixture.run("--version=0.1.4"))
        old = self.fixture.current()
        self.success(self.fixture.run())
        self.assertNotEqual(self.fixture.current(), old)
        self.success(self.fixture.run("--version", "0.1.4"))
        self.assertEqual(self.fixture.current(), old)

    def test_all_platform_cpu_mappings_and_rosetta(self):
        for system, arch, target in (
            ("Linux", "x86_64", "x86_64-unknown-linux-musl"),
            ("Darwin", "x86_64", "x86_64-apple-darwin"),
            ("Darwin", "arm64", "aarch64-apple-darwin"),
        ):
            with self.subTest(system=system, arch=arch), InstallerFixture() as fixture:
                fixture.release(target=target)
                result = fixture.run(environment={"QARGO_TEST_OS": system, "QARGO_TEST_ARCH": arch})
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn(target, fixture.current())
        self.fixture.release(target="aarch64-apple-darwin")
        self.success(self.fixture.run(environment={"QARGO_TEST_OS": "Darwin", "QARGO_TEST_ARCH": "x86_64", "QARGO_TEST_ROSETTA": "1"}))

    def test_linux_arm64_explains_checker_blocker_without_downloading_or_changing_installation(self):
        self.fixture.release()
        self.success(self.fixture.run())
        previous = self.fixture.current()
        requests = self.fixture.requests.read_bytes()
        for arch in ("aarch64", "arm64"):
            result = self.fixture.run(environment={"QARGO_TEST_OS": "Linux", "QARGO_TEST_ARCH": arch})
            self.failure(result)
            self.assertIn("Linux ARM64 is unavailable with Qleisli 0.2.1", result.stderr)
            self.assertIn("/issues/18", result.stderr)
            self.assertEqual(self.fixture.current(), previous)
            self.assertEqual(self.fixture.requests.read_bytes(), requests)

    def test_unsupported_platform_cpu_and_invalid_arguments_do_not_download(self):
        for environment in ({"QARGO_TEST_OS": "Windows"}, {"QARGO_TEST_ARCH": "riscv64"}):
            self.failure(self.fixture.run(environment=environment))
        for args in (("--version", "01.1.6"), ("--version", "v0.1.6"),
                     ("--version", "0.1.6/evil"), ("--version",),
                     ("--version", "0.1.6", "--version=0.1.6"),
                     ("--prefix", "/tmp/duplicate"), ("--unknown",)):
            self.failure(self.fixture.run(*args))
        self.assertFalse(self.fixture.requests.exists())

    def test_unknown_latest_tag(self):
        self.failure(self.fixture.run(environment={"QARGO_TEST_LATEST_TAG": "v0.1.6-rc1"}))
        self.assertFalse(self.fixture.prefix.exists())

    def test_download_failure_and_checksum_mismatch(self):
        archive = self.fixture.release()
        self.failure(self.fixture.run(environment={"QARGO_TEST_FAIL_DOWNLOAD": ".tar.gz"}))
        sums = archive.parent / "SHA256SUMS"
        sums.write_text("0" * 64 + "  " + archive.name + "\n")
        self.failure(self.fixture.run())
        self.assertFalse(self.fixture.prefix.exists())

    def test_missing_duplicate_and_malformed_checksums(self):
        archive = self.fixture.release()
        sums = archive.parent / "SHA256SUMS"
        original = sums.read_text()
        for content in ("", original + original, "z" * 64 + "  " + archive.name + "\n"):
            sums.write_text(content)
            self.failure(self.fixture.run())

    def test_corrupt_and_unsafe_archives(self):
        for options in ({"extra": "../escape"}, {"extra": "/tmp/escape"},
                        {"extra": "unexpected"}, {"duplicate": True},
                        {"link": tarfile.SYMTYPE}, {"link": tarfile.LNKTYPE},
                        {"link": tarfile.FIFOTYPE}, {"absolute": True}, {"unsafe_mode": True}):
            self.fixture.release(**options)
            self.failure(self.fixture.run())
            self.assertFalse(self.fixture.prefix.exists())
        archive = self.fixture.release()
        archive.write_bytes(b"broken gzip")
        (archive.parent / "SHA256SUMS").write_text(hashlib.sha256(archive.read_bytes()).hexdigest() + "  " + archive.name + "\n")
        self.failure(self.fixture.run())

    def test_wrong_version_and_nonexecutable_file(self):
        for options in ({"wrong_version": True}, {"nonexecutable": True}):
            self.fixture.release(**options)
            self.failure(self.fixture.run())
            self.assertFalse(self.fixture.prefix.exists())

    def test_existing_file_and_foreign_symlink_are_preserved(self):
        self.fixture.release()
        directory = self.fixture.prefix / "bin"
        directory.mkdir(parents=True)
        path = directory / "qlippy"
        path.write_text("keep me")
        self.failure(self.fixture.run())
        self.assertEqual(path.read_text(), "keep me")
        path.unlink()
        path.symlink_to("/tmp/foreign-qargo")
        self.failure(self.fixture.run())
        self.assertEqual(path.readlink().as_posix(), "/tmp/foreign-qargo")
        self.assertFalse((directory / "qargo").exists())

    def test_existing_managed_binary_tampering_is_rejected(self):
        self.fixture.release()
        self.success(self.fixture.run())
        current = self.fixture.current()
        (self.fixture.prefix / "bin/qlippy").write_text("changed")
        self.failure(self.fixture.run())
        self.assertEqual(self.fixture.current(), current)

    def test_existing_executable_permissions_are_verified(self):
        self.fixture.release()
        self.success(self.fixture.run())
        current = self.fixture.current()
        (self.fixture.prefix / "bin/qlidoc").chmod(0o777)
        self.failure(self.fixture.run())
        self.assertEqual(self.fixture.current(), current)

    def test_default_prefix_without_writing_to_the_users_home(self):
        self.fixture.release()
        expected = os.environ["HOME"] + "/.local"
        real_mkdir = shlex.quote(shutil.which("mkdir"))
        self.fixture.command("mkdir", f"""#!/bin/sh
for argument do
    if [ "$argument" = "$QARGO_TEST_DEFAULT_PREFIX" ]; then
        printf '%s' "$argument" > "$QARGO_TEST_PREFIX_RECORD"
        exit 1
    fi
done
exec {real_mkdir} "$@"
""")
        record = self.fixture.root / "default-prefix"
        result = self.fixture.run(use_default_prefix=True, environment={
            "QARGO_TEST_DEFAULT_PREFIX": expected,
            "QARGO_TEST_PREFIX_RECORD": str(record),
        })
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(record.read_text(), expected)

    def test_shasum_fallback_requires_no_python_or_jq(self):
        if not shutil.which("shasum"):
            self.skipTest("Host has sha256sum only")
        self.fixture.release()
        for command in ("tar", "mktemp", "awk", "sort", "cmp", "readlink", "ln", "mkdir",
                        "rm", "rmdir", "ls", "cat", "cp", "shasum"):
            (self.fixture.commands / command).symlink_to(shutil.which(command))
        for command in ("gzip", "perl"):
            if shutil.which(command):
                (self.fixture.commands / command).symlink_to(shutil.which(command))
        self.success(self.fixture.run(environment={"PATH": str(self.fixture.commands)}))

    def test_changed_release_archive_is_rejected(self):
        self.fixture.release()
        self.success(self.fixture.run())
        current = self.fixture.current()
        # Valid executable output, but different archived bytes for an immutable version.
        archive = self.fixture.release()
        data = archive.read_bytes() + b"\0"
        archive.write_bytes(data)
        (archive.parent / "SHA256SUMS").write_text(hashlib.sha256(data).hexdigest() + "  " + archive.name + "\n")
        self.failure(self.fixture.run())
        self.assertEqual(self.fixture.current(), current)

    def test_update_switch_failure_preserves_all_old_tools(self):
        self.fixture.release("0.1.4")
        self.fixture.release()
        self.success(self.fixture.run("--version", "0.1.4"))
        current = self.fixture.current()
        before = {tool: (self.fixture.prefix / "bin" / tool).read_bytes() for tool in TOOLS}
        self.failure(self.fixture.run(environment={"QARGO_TEST_FAIL_SWITCH": "yes"}))
        self.assertEqual(self.fixture.current(), current)
        self.assertEqual(before, {tool: (self.fixture.prefix / "bin" / tool).read_bytes() for tool in TOOLS})

    def test_first_switch_failure_removes_new_entry_links(self):
        self.fixture.release()
        self.failure(self.fixture.run(environment={"QARGO_TEST_FAIL_SWITCH": "yes"}))
        for tool in TOOLS:
            self.assertFalse((self.fixture.prefix / "bin" / tool).is_symlink())

    def test_symlinked_managed_directory_is_rejected(self):
        self.fixture.release()
        self.fixture.prefix.mkdir()
        outside = self.fixture.root / "outside"
        outside.mkdir()
        (self.fixture.prefix / "lib").symlink_to(outside)
        self.failure(self.fixture.run())
        self.assertEqual(list(outside.iterdir()), [])

    def test_stale_lock_is_not_removed(self):
        self.fixture.release()
        lock = self.fixture.prefix / "lib/qargo/.install-lock"
        lock.mkdir(parents=True)
        result = self.fixture.run()
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(lock.is_dir())
        self.assertFalse((self.fixture.prefix / "lib/qargo/current").exists())

    def test_help_needs_no_network(self):
        self.success(self.fixture.run("--help"))
        self.assertFalse(self.fixture.requests.exists())


if __name__ == "__main__":
    unittest.main()
