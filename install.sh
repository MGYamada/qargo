#!/bin/sh
# Install a verified standard bundle; this is never a Qargo runtime operation.

fail() {
    printf 'qargo installer: %s\n' "$*" >&2
    exit 1
}

usage() {
    cat <<'EOF'
Usage: sh install.sh [--version MAJOR.MINOR.PATCH] [--prefix PATH]
Install qargo, qlippy, qlifmt, and qlidoc without Rust or Cargo.
Supports Linux x86_64 and macOS x86_64/ARM64. Linux ARM64 is unavailable.
The default is the latest formal release under $HOME/.local.
PATH must be absolute. No sudo or shell configuration changes are made.
Run again to update, or use --version to select a particular release.
EOF
}

valid_version() {
    printf '%s\n' "$1" | awk '
        /^[0-9]+\.[0-9]+\.[0-9]+$/ {
            split($0, parts, ".")
            for (i = 1; i <= 3; i++)
                if (parts[i] !~ /^(0|[1-9][0-9]*)$/) exit 1
            ok = 1
        }
        END { if (!ok) exit 1 }
    '
}

hash_file() {
    "$checksum_tool" "$1" | awk '{print $1}'
}

download() {
    curl --proto '=https' --tlsv1.2 -fLsS --connect-timeout 15 --max-time 300 \
        "$1" -o "$2" || fail "Download failed: $1"
}

cleanup() {
    if [ "$committed" = no ]; then
        for tool in $created_links; do
            if [ "$(readlink "$prefix/bin/$tool" 2>/dev/null || :)" = "../lib/qargo/current/bin/$tool" ]; then
                rm -f "$prefix/bin/$tool"
            fi
        done
    fi
    [ -z "$switch_link" ] || rm -f "$switch_link"
    [ -z "$stage" ] || rm -rf "$stage"
    [ -z "$work" ] || rm -rf "$work"
    [ "$locked" = no ] || rmdir "$managed/.install-lock"
}

main() {
    set -eu
    umask 022
    LC_ALL=C
    export LC_ALL
    version=
    prefix=
    version_seen=no
    prefix_seen=no
    while [ "$#" -gt 0 ]; do
        option=$1
        shift
        case "$option" in
            --help) usage; return ;;
            --version|--prefix)
                [ "$#" -gt 0 ] || fail "$option requires a value."
                value=$1
                shift
                ;;
            --version=*|--prefix=*) value=${option#*=}; option=${option%%=*} ;;
            *) fail "Unknown argument: $option" ;;
        esac
        [ -n "$value" ] || fail "$option requires a nonempty value."
        case "$option" in
            --version)
                [ "$version_seen" = no ] || fail "Repeated --version."
                version_seen=yes
                valid_version "$value" || fail "Expected canonical MAJOR.MINOR.PATCH."
                version=$value
                ;;
            --prefix)
                [ "$prefix_seen" = no ] || fail "Repeated --prefix."
                prefix_seen=yes
                prefix=$value
                ;;
        esac
    done
    if [ "$prefix_seen" = no ]; then
        [ -n "${HOME:-}" ] || fail "HOME is unset; supply --prefix."
        prefix=$HOME/.local
    fi
    case "$prefix" in /*) ;; *) fail "--prefix must be an absolute path." ;; esac
    case "$prefix" in *'
'*|*"$(printf '\r')"*) fail "--prefix cannot contain line breaks." ;; esac

    for command in curl tar uname mktemp awk sort cmp readlink ln mv mkdir rm rmdir ls cat; do
        command -v "$command" >/dev/null 2>&1 || fail "Required command is missing: $command"
    done
    if command -v sha256sum >/dev/null 2>&1; then
        checksum_tool=sha256sum
    elif command -v shasum >/dev/null 2>&1; then
        # A wrapper keeps checksum arguments identical on both platforms.
        checksum_tool=sha256_mac
    else
        fail "Required command is missing: sha256sum or shasum."
    fi
    os=$(uname -s)
    arch=$(uname -m)
    if [ "$os" = Darwin ] && [ "$arch" = x86_64 ] && [ "$(sysctl -n hw.optional.arm64 2>/dev/null || :)" = 1 ]; then
        arch=arm64
    fi
    case "$arch" in x86_64|amd64) cpu=x86_64 ;; aarch64|arm64) cpu=aarch64 ;; *) fail "Unsupported CPU: $arch" ;; esac
    case "$os" in
        Linux)
            [ "$cpu" = x86_64 ] || fail "Linux ARM64 is unavailable with Qleisli 0.2.1; see https://github.com/MGYamada/qargo/issues/18."
            target=$cpu-unknown-linux-musl
            ;;
        Darwin) target=$cpu-apple-darwin ;;
        *) fail "Unsupported OS: $os" ;;
    esac

    repository=https://github.com/MGYamada/qargo
    if [ -z "$version" ]; then
        latest=$(curl --proto '=https' --tlsv1.2 -fLsS --connect-timeout 15 --max-time 60 \
            -o /dev/null -w '%{url_effective}' "$repository/releases/latest") || fail "Cannot resolve the latest formal release."
        case "$latest" in "$repository/releases/tag/v"*) version=${latest#"$repository/releases/tag/v"} ;; *) fail "Unexpected latest-release URL." ;; esac
        valid_version "$version" || fail "Latest release has an unsupported tag."
    fi
    bundle=qargo-$version-$target
    archive=$bundle.tar.gz
    release=$repository/releases/download/v$version
    work=
    stage=
    switch_link=
    locked=no
    committed=no
    created_links=
    trap cleanup 0
    trap 'exit 1' HUP INT TERM
    work=$(mktemp -d "${TMPDIR:-/tmp}/qargo-install.XXXXXX") || fail "Cannot create temporary directory."
    download "$release/SHA256SUMS" "$work/SHA256SUMS"
    expected_hash=$(awk -v name="$archive" '
        $2 == name {
            if (NF != 2 || length($1) != 64 || $1 ~ /[^0-9a-f]/) bad = 1
            hash = $1; count++
        }
        END { if (count != 1 || bad) exit 1; print hash }
    ' "$work/SHA256SUMS") || fail "Missing, repeated, or invalid archive checksum."
    download "$release/$archive" "$work/$archive"
    [ "$(hash_file "$work/$archive")" = "$expected_hash" ] || fail "Archive SHA-256 mismatch."

    # The producer emits only these six regular files, without directory entries.
    {
        printf '%s\n' "$bundle/LICENSE" "$bundle/NOTICE"
        for tool in qargo qlippy qlifmt qlidoc; do printf '%s\n' "$bundle/bin/$tool"; done
    } | sort > "$work/expected"
    tar -P -tzf "$work/$archive" > "$work/inventory" || fail "Invalid archive."
    sort "$work/inventory" > "$work/actual"
    cmp -s "$work/expected" "$work/actual" || fail "Unsafe or unexpected archive inventory."
    tar -P -tvzf "$work/$archive" > "$work/types" || fail "Invalid archive metadata."
    awk '
        /\/bin\/(qargo|qlippy|qlifmt|qlidoc)$/ {
            if (substr($0, 1, 10) != "-rwxr-xr-x") exit 1
            next
        }
        /\/(LICENSE|NOTICE)$/ {
            if (substr($0, 1, 10) != "-rw-r--r--") exit 1
            next
        }
        { exit 1 }
        END { if (NR != 6) exit 1 }
    ' "$work/types" || fail "Archive must contain six regular files with canonical modes, without links."
    mkdir "$work/unpacked"
    tar -xzf "$work/$archive" --no-same-owner -C "$work/unpacked" || fail "Cannot extract archive."
    unpacked=$work/unpacked/$bundle
    for tool in qargo qlippy qlifmt qlidoc; do
        [ -f "$unpacked/bin/$tool" ] && [ ! -L "$unpacked/bin/$tool" ] && [ -x "$unpacked/bin/$tool" ] || fail "Invalid executable: $tool"
        actual_version=$("$unpacked/bin/$tool" --version) || fail "Cannot execute $tool on this system."
        [ "$actual_version" = "$tool $version (Qleisli 0.2.1, finite-v0)" ] || fail "Incorrect executable version: $tool"
    done

    mkdir -p "$prefix"
    prefix=$(cd -P "$prefix" && pwd)
    [ "$prefix" != / ] || fail "The filesystem root cannot be the prefix."
    for directory in "$prefix/bin" "$prefix/lib" "$prefix/lib/qargo" "$prefix/lib/qargo/releases"; do
        [ ! -L "$directory" ] || fail "Managed directory cannot be a symlink: $directory"
        mkdir -p "$directory"
    done
    managed=$prefix/lib/qargo
    mkdir "$managed/.install-lock" 2>/dev/null || fail "Another install is running, or a stale .install-lock needs removal."
    locked=yes
    if [ -e "$managed/current" ] || [ -L "$managed/current" ]; then
        [ -L "$managed/current" ] || fail "Refusing to replace an unmanaged current path."
        previous=$(readlink "$managed/current")
        case "$previous" in releases/qargo-*) ;; *) fail "Unmanaged current link." ;; esac
        previous_name=${previous#releases/}
        case "$previous_name" in *[!a-zA-Z0-9._-]*|*..*) fail "Unsafe current link." ;; esac
        [ -f "$managed/$previous/.archive-sha256" ] && [ ! -L "$managed/$previous" ] || fail "Current bundle is not managed by this installer."
    fi
    for tool in qargo qlippy qlifmt qlidoc; do
        link=$prefix/bin/$tool
        if [ -e "$link" ] || [ -L "$link" ]; then
            [ -L "$link" ] && [ "$(readlink "$link")" = "../lib/qargo/current/bin/$tool" ] || fail "Refusing to overwrite existing $link"
        fi
    done
    destination=$managed/releases/$bundle
    if [ -e "$destination" ] || [ -L "$destination" ]; then
        [ -d "$destination" ] && [ ! -L "$destination" ] && [ ! -L "$destination/bin" ] || fail "Unsafe existing bundle."
        [ -f "$destination/.archive-sha256" ] && [ ! -L "$destination/.archive-sha256" ] || fail "Existing bundle is unmanaged."
        [ "$(cat "$destination/.archive-sha256")" = "$expected_hash" ] || fail "Existing release has different archive bytes."
        for file in LICENSE NOTICE bin/qargo bin/qlippy bin/qlifmt bin/qlidoc; do
            [ -f "$destination/$file" ] && [ ! -L "$destination/$file" ] && cmp -s "$unpacked/$file" "$destination/$file" || fail "Existing bundle differs: $file"
            case "$file" in bin/*) expected_mode=-rwxr-xr-x ;; *) expected_mode=-rw-r--r-- ;; esac
            actual_mode=$(ls -ld "$destination/$file" | awk '{print substr($1, 1, 10)}')
            [ "$actual_mode" = "$expected_mode" ] || fail "Existing bundle mode differs: $file"
        done
        for tool in qargo qlippy qlifmt qlidoc; do [ -x "$destination/bin/$tool" ] || fail "Existing executable is not executable: $tool"; done
    else
        stage=$(mktemp -d "$managed/.stage.XXXXXX")
        # Extract again on the destination filesystem so the final move is atomic.
        tar -xzf "$work/$archive" --no-same-owner -C "$stage"
        printf '%s\n' "$expected_hash" > "$stage/$bundle/.archive-sha256"
        mv "$stage/$bundle" "$destination"
    fi
    for tool in qargo qlippy qlifmt qlidoc; do
        if [ ! -L "$prefix/bin/$tool" ]; then
            created_links="$created_links $tool"
            ln -s "../lib/qargo/current/bin/$tool" "$prefix/bin/$tool"
        fi
    done
    switch_link=$managed/.install-lock/current
    ln -s "releases/$bundle" "$switch_link"
    case "$os" in
        Linux) mv -fT "$switch_link" "$managed/current" ;;
        Darwin) mv -fh "$switch_link" "$managed/current" ;;
    esac
    committed=yes
    printf 'Installed Qargo %s (%s) under %s\n' "$version" "$target" "$prefix"
    case ":${PATH:-}:" in
        *":$prefix/bin:"*) ;;
        *) printf 'Add this directory to PATH: %s/bin\n' "$prefix" ;;
    esac
}

sha256_mac() { shasum -a 256 "$1"; }

main "$@"
