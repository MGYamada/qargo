#!/usr/bin/env python3
"""Build, verify, and collect binary release assets outside Qargo operations."""

import argparse
import gzip
import hashlib
import io
from pathlib import Path
import re
import struct
import tarfile
import tempfile

from verify_release import PRODUCT_VERSION, TOOLS, require, verify


TARGETS = (
    "x86_64-unknown-linux-musl",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
)


def bundle_name(target, version=PRODUCT_VERSION):
    require(target in TARGETS, "Unsupported distribution target")
    return f"qargo-{version}-{target}"


def verify_published_assets(artifact_dir, version=PRODUCT_VERSION):
    """Require the exact flat release inventory before archive/checksum verification."""
    require(re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", version),
            "Expected a canonical release version")
    expected = {bundle_name(target, version) + ".tar.gz" for target in TARGETS}
    expected.update({"install.sh", "SHA256SUMS"})
    entries = list(artifact_dir.iterdir())
    actual = {entry.name for entry in entries}
    require(actual == expected,
            "Incorrect published asset inventory; missing: " + ", ".join(sorted(expected - actual))
            + "; unexpected: " + ", ".join(sorted(actual - expected)))
    require(all(entry.is_file() and not entry.is_symlink() for entry in entries),
            "Published assets must be regular files, without links or nested directories")
    print(f"Published asset inventory passed for {version}: {len(expected)} regular files.")


def binary_platform(content, target):
    """Check architecture, static Linux linkage, and macOS deployment/load commands."""
    require(len(content) >= 64, "Truncated executable")
    if target.endswith("linux-musl"):
        require(content[:6] == b"\x7fELF\x02\x01", "Expected little-endian ELF64")
        machine = struct.unpack_from("<H", content, 18)[0]
        require(machine == (62 if target.startswith("x86_64") else 183), "Incorrect ELF CPU")
        offset = struct.unpack_from("<Q", content, 32)[0]
        size, count = struct.unpack_from("<HH", content, 54)
        require(size >= 56 and offset + size * count <= len(content), "Invalid ELF program headers")
        for index in range(count):
            header = offset + index * size
            kind = struct.unpack_from("<I", content, header)[0]
            require(kind != 3, "Linux bundle requires static linkage (PT_INTERP found)")
            if kind == 2:
                start = struct.unpack_from("<Q", content, header + 8)[0]
                length = struct.unpack_from("<Q", content, header + 32)[0]
                require(start + length <= len(content) and length % 16 == 0, "Invalid ELF dynamic table")
                for position in range(start, start + length, 16):
                    tag = struct.unpack_from("<q", content, position)[0]
                    if tag == 0:
                        break
                    require(tag != 1, "Linux executable has a dynamic library dependency")
    else:
        require(content[:4] == b"\xcf\xfa\xed\xfe", "Expected little-endian Mach-O64")
        cpu, = struct.unpack_from("<I", content, 4)
        require(cpu == (0x01000007 if target.startswith("x86_64") else 0x0100000C), "Incorrect Mach-O CPU")
        count, total = struct.unpack_from("<II", content, 16)
        require(32 + total <= len(content), "Invalid Mach-O load commands")
        offset = 32
        minimum = None
        for _ in range(count):
            require(offset + 8 <= 32 + total, "Truncated Mach-O load command")
            kind, size = struct.unpack_from("<II", content, offset)
            require(size >= 8 and offset + size <= 32 + total, "Invalid Mach-O load command size")
            if kind in (0x24, 0x32):
                require(size >= (16 if kind == 0x24 else 24), "Invalid deployment command")
                if kind == 0x32:
                    require(struct.unpack_from("<I", content, offset + 8)[0] == 1, "Expected macOS platform")
                minimum = struct.unpack_from("<I", content, offset + (8 if kind == 0x24 else 12))[0]
            if kind in (0xC, 0x80000018, 0x8000001F):
                require(size >= 24, "Invalid dylib command")
                name_offset = struct.unpack_from("<I", content, offset + 8)[0]
                require(24 <= name_offset < size, "Invalid dylib name offset")
                name = content[offset + name_offset:offset + size].split(b"\0", 1)[0]
                require(name.startswith((b"/usr/lib/", b"/System/Library/")), "Non-system macOS dependency")
            offset += size
        require(offset == 32 + total, "Incorrect Mach-O command inventory")
        require(minimum == 11 << 16, "macOS deployment target must be 11.0")


def archive_files(source_root, archive_path, target):
    root = bundle_name(target)
    expected = {f"{root}/bin/{tool}": 0o755 for tool in TOOLS}
    expected.update({f"{root}/{name}": 0o644 for name in ("LICENSE", "NOTICE")})
    files = {}
    with tarfile.open(archive_path, "r:gz") as archive:
        for entry in archive:
            label = entry.name.removeprefix(root + "/")
            require(entry.name in expected and label not in files, "Unsafe, repeated, or unexpected archive path")
            require(entry.isfile() and entry.mode == expected[entry.name], "Invalid archive file type or mode")
            require(0 < entry.size <= 64 * 1024 * 1024, "Invalid archive file size")
            require(entry.uid == entry.gid == entry.mtime == 0 and not entry.pax_headers, "Unexpected archive metadata")
            files[label] = archive.extractfile(entry).read()
    require(len(files) == len(expected), "Incomplete binary bundle")
    for name in ("LICENSE", "NOTICE"):
        require(files[name] == (source_root / name).read_bytes(), "Incorrect " + name)
    for tool in TOOLS:
        binary_platform(files["bin/" + tool], target)
    return files


def build(source_root, bin_dir, target, output_dir):
    # Runtime verification also binds the candidate's canonical qrate sources.
    verify(source_root, bin_dir)
    files = {"bin/" + tool: (bin_dir / tool).read_bytes() for tool in TOOLS}
    files.update({name: (source_root / name).read_bytes() for name in ("LICENSE", "NOTICE")})
    for tool in TOOLS:
        binary_platform(files["bin/" + tool], target)
    output_dir.mkdir(parents=True, exist_ok=True)
    path = output_dir / (bundle_name(target) + ".tar.gz")
    with path.open("xb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode="w", format=tarfile.USTAR_FORMAT) as archive:
                for label, content in sorted(files.items()):
                    entry = tarfile.TarInfo(bundle_name(target) + "/" + label)
                    entry.size = len(content)
                    entry.mode = 0o755 if label.startswith("bin/") else 0o644
                    archive.addfile(entry, io.BytesIO(content))
    archive_files(source_root, path, target)
    print("Built " + str(path))


def verify_archive(source_root, archive_path, target):
    files = archive_files(source_root, archive_path, target)
    with tempfile.TemporaryDirectory(prefix="qargo-binary-verify-") as directory:
        root = Path(directory)
        for label, content in files.items():
            path = root / label
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(content)
            path.chmod(0o755 if label.startswith("bin/") else 0o644)
        verify(source_root, root / "bin")
        # Exercise the real installer with local responses for the production URLs.
        from test_install import InstallerFixture
        with InstallerFixture() as fixture:
            fixture.add_archive(PRODUCT_VERSION, target, archive_path)
            result = fixture.run("--version", PRODUCT_VERSION)
            require(result.returncode == 0, "Installer failed: " + result.stderr)
            verify(source_root, fixture.prefix / "bin")
            repeated = fixture.run()
            require(repeated.returncode == 0, "Repeated/latest installation failed: " + repeated.stderr)
            require(not fixture.marker.exists(), "Installer invoked a developer tool or Python/jq")
            require((fixture.prefix / "lib/qargo/current").readlink().as_posix()
                    == "releases/" + bundle_name(target), "Incorrect installed bundle link")
    print("Binary archive and Cargo-free installation verification passed: " + target)


def collect(source_root, artifact_dir, output_dir):
    """Only assemble the complete candidate set; never publish or replace assets."""
    candidates = {}
    for target in TARGETS:
        name = bundle_name(target) + ".tar.gz"
        matches = list(artifact_dir.rglob(name))
        require(len(matches) == 1 and matches[0].is_file() and not matches[0].is_symlink(), "Missing or repeated asset: " + name)
        archive_files(source_root, matches[0], target)
        candidates[name] = matches[0].read_bytes()
    candidates["install.sh"] = (source_root / "install.sh").read_bytes()
    sums = "".join(hashlib.sha256(data).hexdigest() + "  " + name + "\n"
                   for name, data in sorted(candidates.items()))
    candidates["SHA256SUMS"] = sums.encode("ascii")
    output_dir.mkdir(parents=True, exist_ok=True)
    for name, data in sorted(candidates.items()):
        with (output_dir / name).open("xb") as output:
            output.write(data)
    print("Collected three archives, install.sh, and SHA256SUMS. No release was published.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    inventory = commands.add_parser("verify-published-assets")
    inventory.add_argument("--artifact-dir", type=Path, required=True)
    inventory.add_argument("--version", default=PRODUCT_VERSION)
    for command in ("build", "verify", "collect"):
        child = commands.add_parser(command)
        child.add_argument("--source-root", type=Path, default=Path(__file__).resolve().parent.parent)
        if command != "collect":
            child.add_argument("--target", choices=TARGETS, required=True)
        if command == "build":
            child.add_argument("--bin-dir", type=Path, required=True)
        elif command == "verify":
            child.add_argument("--archive", type=Path, required=True)
        else:
            child.add_argument("--artifact-dir", type=Path, required=True)
        if command != "verify":
            child.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "verify-published-assets":
        verify_published_assets(args.artifact_dir, args.version)
    elif args.command == "build":
        build(args.source_root.resolve(), args.bin_dir.resolve(), args.target, args.output_dir)
    elif args.command == "verify":
        verify_archive(args.source_root.resolve(), args.archive, args.target)
    else:
        collect(args.source_root.resolve(), args.artifact_dir, args.output_dir)


if __name__ == "__main__":
    main()
