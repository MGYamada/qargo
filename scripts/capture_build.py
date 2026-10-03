#!/usr/bin/env python3
"""Explicit native build inputs and generated-output binding for extraction checks.

This is a developer observation, not a reproducible-build or sandbox attestation.
Only the prescribed native Linux/GCC and macOS/CLT environments are admitted.
"""

import hashlib
import os
from pathlib import Path
import platform
import stat
import subprocess
from concurrent.futures import ThreadPoolExecutor

from qrate_project import identity, require


def output(command):
    search = str(Path(command[0]).parent) + ":/usr/bin:/bin"
    return subprocess.check_output([str(part) for part in command], env={"PATH": search, "LC_ALL": "C", "OPENSSL_CONF": "/dev/null", "GIT_CONFIG_NOSYSTEM": "1",
                "GIT_CONFIG_SYSTEM": "/dev/null", "GIT_CONFIG_GLOBAL": "/dev/null"}).decode().strip()


def file_digest(path):
    with path.open("rb") as stream:
        return "sha256:" + hashlib.file_digest(stream, "sha256").hexdigest()


def external_tree(root):
    """Native distributions contain symlinks: bind their text and regular bytes.

    Resolution of the selected executables is separately captured. Do not use
    this relaxed inventory for portable project inputs, which forbid all links.
    """
    result = []
    regular = []

    def visit(path):
        label = path.relative_to(root).as_posix()
        before = path.lstat()
        if stat.S_ISLNK(before.st_mode):
            entry = {"path": label, "kind": "symlink", "target": os.readlink(path)}
        elif stat.S_ISDIR(before.st_mode):
            entry = {"path": label, "kind": "directory"}
            for child in sorted(path.iterdir()):
                visit(child)
        else:
            require(stat.S_ISREG(before.st_mode), "Unsupported native input: " + str(path))
            regular.append((path, before))
            return
        after = path.lstat()
        require((before.st_ino, before.st_dev, before.st_size, before.st_mtime_ns, before.st_ctime_ns)
                == (after.st_ino, after.st_dev, after.st_size, after.st_mtime_ns, after.st_ctime_ns),
                "Native input changed during capture: " + str(path))
        result.append(entry)

    visit(root)
    def capture_file(item):
        path, before = item
        digest = file_digest(path)
        after = path.lstat()
        require((before.st_ino, before.st_dev, before.st_size, before.st_mtime_ns, before.st_ctime_ns)
                == (after.st_ino, after.st_dev, after.st_size, after.st_mtime_ns, after.st_ctime_ns),
                "Native file changed during capture: " + str(path))
        return {"path": path.relative_to(root).as_posix(), "kind": "file", "sha256": digest,
                "executable": bool(before.st_mode & 0o111), "size": before.st_size}
    with ThreadPoolExecutor(max_workers=16) as pool:
        result.extend(pool.map(capture_file, regular))
    return sorted(result, key=lambda entry: entry["path"].encode())


def linux_resource_inputs(cgroup=Path("/proc/self/cgroup"), root=Path("/sys/fs/cgroup")):
    """Capture the cgroup CPU limits queried by Rust's parallelism detection."""
    paths = []
    for line in cgroup.read_text().splitlines():
        hierarchy, controllers, relative = line.split(":", 2)
        if hierarchy != "0" or controllers:
            continue
        relative = Path(relative.lstrip("/"))
        require(".." not in relative.parts, "Invalid unified cgroup path")
        directory = root / relative
        for ancestor in (directory, *directory.parents):
            if not ancestor.is_relative_to(root):
                break
            control = ancestor / "cpu.max"
            if control.is_file():
                paths.append(control)
    return paths


def capture_native(toolchain):
    toolchain = toolchain.resolve()
    rustc = toolchain / "bin/rustc"
    require(rustc.is_file() and (toolchain / "bin/cargo").is_file(), "Pass a resolved Rust toolchain directory, not rustup shims")
    require(Path(output([rustc, "--print", "sysroot"])).resolve() == toolchain,
            "Compiler sysroot differs from the captured toolchain")
    system = platform.system()
    host = next(line.split(": ", 1)[1] for line in output([rustc, "-vV"]).splitlines() if line.startswith("host: "))
    if system == "Darwin":
        require(host.endswith("-apple-darwin"), "Only native host builds are admitted")
        developer = Path("/Library/Developer/CommandLineTools")
        require(developer.is_dir(), "macOS extraction verification requires Command Line Tools")
        sdk = (developer / "SDKs/MacOSX.sdk").resolve()
        compiler = developer / "usr/bin/clang"
        dyld = Path("/System/Volumes/Preboot/Cryptexes/OS/System/Library/dyld")
        if not dyld.is_dir():
            dyld = Path("/System/Library/dyld")
        require(any(dyld.glob("dyld_shared_cache*")), "Cannot identify macOS system library content")
        roots = [developer, dyld, Path("/bin"), Path("/usr/lib"),
                 *[Path("/usr/bin") / name for name in ("xcrun", "xcode-select", "env", "uname")],
                 Path("/usr/bin/sandbox-exec"), Path("/private/etc/ssl/openssl.cnf"),
                 Path("/System/Library/CoreServices/SystemVersion.plist")]
        env = {"SDKROOT": str(sdk), "DEVELOPER_DIR": str(developer),
               "LIBCLANG_PATH": str(developer / "usr/lib"), "MACOSX_DEPLOYMENT_TARGET": "11.0"}
        # libproc 0.14.11 uses this CLT SDK header path in its build script.
        require((sdk / "usr/include/libproc.h").is_file(), "Missing admitted libproc SDK input")
    elif system == "Linux":
        require(host == "x86_64-unknown-linux-gnu", "Only native Linux x86_64 GNU development builds are admitted")
        compiler = Path("/usr/bin/gcc").resolve()
        require(compiler.is_file(), "Linux extraction verification requires system GCC")
        roots = [Path(name) for name in ("/usr/include", "/usr/lib/gcc", "/usr/lib/x86_64-linux-gnu",
                                        "/lib/x86_64-linux-gnu", "/usr/lib64", "/lib64",
                                        "/etc/ld.so.cache", "/etc/ld.so.conf", "/etc/ld.so.conf.d",
                                        "/etc/ssl/openssl.cnf", "/etc/ssl/certs/ca-certificates.crt",
                                        "/sys/kernel/mm/transparent_hugepage/enabled") if Path(name).exists()]
        roots += [Path("/usr/bin") / name for name in ("gcc", "as", "ld", "ar", "ranlib", "objcopy", "strip", "env", "uname", "sh", "strace")]
        roots += linux_resource_inputs()
        roots = list(dict.fromkeys(path.resolve() for path in roots))
        env = {}
    else:
        raise RuntimeError("Unsupported native build capture platform: " + system)
    require(host.startswith(platform.machine().replace("arm64", "aarch64")), "Cross builds require a separate capture contract")
    tools = {}
    for name in ("cargo", "rustc", "rustdoc", "clippy-driver", "cargo-clippy", "rustfmt", "cargo-fmt"):
        binary = toolchain / "bin" / name
        require(binary.is_file(), "Missing verification tool: " + name)
        tools[name] = {"sha256": file_digest(binary), "version": output([binary, "--version"])}
    tools["cc"] = {"sha256": file_digest(compiler.resolve()), "version": output([compiler, "--version"])}
    env.update({"CC": str(compiler), "CXX": str(compiler), "RUSTC": str(rustc),
                "RUSTDOC": str(toolchain / "bin/rustdoc"),
                "RUSTFLAGS": "-C linker=" + str(compiler), "RUSTDOCFLAGS": "-D warnings",
                "PATH": str(toolchain / "bin") + ":/usr/bin:/bin", "LC_ALL": "C", "TZ": "UTC",
                "OPENSSL_CONF": "/dev/null", "GIT_CONFIG_NOSYSTEM": "1",
                "GIT_CONFIG_SYSTEM": "/dev/null", "GIT_CONFIG_GLOBAL": "/dev/null"})
    print("Capturing Rust toolchain, native tools, SDK/headers and system libraries...", flush=True)
    inventories = {"rust-toolchain": external_tree(toolchain)}
    for root in roots:
        print("Capturing native input: " + str(root), flush=True)
        inventories[str(root)] = external_tree(root)
    record = {"format": "qargo.native-build-inputs", "version": 1, "host": host,
              "platform": platform.platform(), "tools": tools, "environment": env,
              "inventories": inventories}
    record["native_id"] = identity("qargo.native-build-inputs.v1", record)
    return record, env


def verify_native_unchanged(native, toolchain):
    for label, expected in native["inventories"].items():
        root = toolchain if label == "rust-toolchain" else Path(label)
        require(external_tree(root) == expected, "Native inputs changed during verification: " + label)


class BuildGuard:
    """Deny unrecorded macOS reads; audit successful Linux opens with strace.

    Temporary build/test state and OS process/device interfaces are admitted
    explicitly. Project/configuration escape attempts are never a silent success.
    """

    def __init__(self, project, work, toolchain, native):
        self.work = work
        self.roots = [project, work, toolchain, *[Path(label) for label in native["inventories"] if label != "rust-toolchain"]]
        self.sequence = 0
        self.darwin = platform.system() == "Darwin"
        if self.darwin:
            # Path strings are serialized as JSON strings, also valid SBPL strings.
            import json
            clauses = " ".join("(subpath " + json.dumps(str(root)) + ")" for root in self.roots)
            profile = ('(version 1) (allow default) (deny network*) '
                       '(deny file-read-data (require-not (require-any (vnode-type DIRECTORY) ' + clauses +
                       ' (subpath "/dev") (literal "/private/etc/localtime") '
                       '(literal "/usr/share/zoneinfo/UTC")))) '
                       '(deny file-write* (require-not (require-any (subpath ' + json.dumps(str(work)) +
                       ') (subpath "/dev"))))')
            self.profile = work / "build.sb"
            self.profile.write_text(profile)
        else:
            require(Path("/usr/bin/strace").is_file(), "Linux extraction verification requires strace")

    def run(self, command, **kwargs):
        if self.darwin:
            command = ["/usr/bin/sandbox-exec", "-f", str(self.profile), *map(str, command)]
            return subprocess.run(command, **kwargs)
        self.sequence += 1
        trace = self.work / f"build-access-{self.sequence}.log"
        result = subprocess.run(["/usr/bin/strace", "-f", "-qq", "-yy", "-s", "8192",
                                 "-e", "trace=open,openat,openat2,execve", "-o", str(trace),
                                 *map(str, command)], **kwargs)
        audit_linux_access(trace.read_text(), self.roots)
        return result


def audit_linux_access(trace, roots):
    """Reject every observed unrecorded read/launch, reporting the full set."""
    import re
    violations = set()
    for line in trace.splitlines():
        # -yy annotates successful opened descriptors with resolved paths.
        # Failed probes cannot supply compilation bytes.
        opened = re.search(r"= \d+<(/[^>]*)>", line)
        if opened:
            path = Path(opened[1].removesuffix(" (deleted)"))
            if not (any(path.is_relative_to(root) for root in roots)
                    or path.is_relative_to("/proc") or path.is_relative_to("/dev")):
                violations.add("Unrecorded native/project read: " + str(path))
        # Audit attempts too: concurrent strace output can split execve's
        # entry and successful return over separate lines.
        executed = re.search(r'execve\("([^"]+)"', line)
        if executed:
            path = Path(executed[1])
            if not path.is_absolute():
                violations.add("Relative build executable requires an explicit capture rule: " + str(path))
            elif not any(path.resolve().is_relative_to(root) for root in roots):
                violations.add("Unrecorded build executable: " + str(path))
    require(not violations, "\n".join(sorted(violations)))


def generated_inputs(target):
    """Bind every dependency OUT_DIR byte, not only files ending in .rs.

    Build-script executables/output directives bind the generator and its Cargo
    inputs; the enclosing record also binds the locked project and native inputs.
    Cargo dep-info records the compilation inputs actually reported by rustc.
    """
    result = []
    for path in sorted(target.rglob("*")):
        if not path.is_file():
            continue
        relative = path.relative_to(target)
        # Proc-macro shared objects are invoked generators, not final products.
        if "build" in relative.parts or path.suffix in (".d", ".so", ".dylib", ".dll"):
            require(not path.is_symlink(), "Generated compilation input is a symlink")
            result.append({"path": relative.as_posix(), "sha256": file_digest(path), "size": path.stat().st_size})
    return result


def build_binding(project_id, native, choices, generated, artifacts):
    record = {"format": "qargo.rust-build-environment", "version": 1,
              "project_id": project_id, "native_id": native["native_id"],
              "choices": choices, "generated_inputs": generated, "artifacts": artifacts}
    # Final artifact digests are outputs, separate from environment identity.
    binding = {key: value for key, value in record.items() if key != "artifacts"}
    record["build_environment_id"] = identity("qargo.rust-build-environment.v1", binding)
    return record
