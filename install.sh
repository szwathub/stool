#!/usr/bin/env bash
# Install a verified prebuilt stool release without Git or a compiler.
set -euo pipefail

repo='szwathub/stool'
work_dir=''
staged_binary=''
cleanup() {
    if [ -n "$staged_binary" ]; then rm -f -- "$staged_binary" || true; fi
    if [ -n "$work_dir" ]; then rm -rf -- "$work_dir" || true; fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
fail() { printf 'Error: %s\n' "$*" >&2; exit 1; }

# This installer needs base system utilities, never Git or a compiler.
for tool in uname mktemp mkdir rm awk chmod cp mv; do
    command -v "$tool" >/dev/null 2>&1 || fail "Required command is missing: $tool"
done
if command -v curl >/dev/null 2>&1; then
    downloader=curl
elif command -v wget >/dev/null 2>&1; then
    downloader=wget
else
    fail 'Install curl or wget before installing stool.'
fi
if command -v sha256sum >/dev/null 2>&1; then
    hasher=sha256sum
elif command -v shasum >/dev/null 2>&1; then
    hasher=shasum
else
    fail 'Install sha256sum or shasum before installing stool; SHA256 verification is required.'
fi

download() {
    printf 'Downloading %s\n' "$1" >&2
    if [ "$downloader" = curl ]; then
        curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 --retry 2 --connect-timeout 15 "$1" --output "$2"
    else
        wget --https-only --quiet --output-document="$2" "$1"
    fi
}

system=$(uname -s)
machine=$(uname -m)
case "$machine" in
    arm64|aarch64) arch=aarch64 ;;
    x86_64|amd64) arch=x86_64 ;;
    *) fail "Unsupported architecture: $machine" ;;
esac
suffix='.tar.gz'
binary_name=stool
case "$system" in
    Darwin) target="$arch-apple-darwin" ;;
    Linux)
        libc=gnu
        if command -v ldd >/dev/null 2>&1; then
            case "$(ldd --version 2>&1 || true)" in *musl*) libc=musl ;; esac
        elif compgen -G '/lib/ld-musl-*.so.1' >/dev/null; then
            libc=musl
        fi
        if [ "$arch" = aarch64 ] && [ "$libc" = musl ]; then
            fail 'Linux ARM64 musl is not supported by the release assets.'
        fi
        target="$arch-unknown-linux-$libc"
        ;;
    MINGW*|MSYS*|CYGWIN*)
        [ "$arch" = x86_64 ] || fail 'Only Windows x86_64 release assets are available.'
        target=x86_64-pc-windows-msvc
        suffix='.zip'
        binary_name=stool.exe
        ;;
    *) fail "Unsupported operating system: $system" ;;
esac
if [ "$suffix" = .zip ]; then
    command -v unzip >/dev/null 2>&1 || fail 'Install unzip before installing stool.'
else
    command -v tar >/dev/null 2>&1 || fail 'Install tar before installing stool.'
fi

work_dir=$(mktemp -d "${TMPDIR:-/tmp}/stool-install.XXXXXX")
version=${VERSION:-latest}
if [ "$version" = latest ]; then
    latest_url="https://github.com/$repo/releases/latest"
    if [ "$downloader" = curl ]; then
        resolved=$(curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 --retry 2 --connect-timeout 15 "$latest_url" --output /dev/null --write-out '%{url_effective}') || fail 'Could not find the latest stool release.'
    else
        resolved=$(wget --https-only --server-response --spider "$latest_url" 2>&1 | awk 'tolower($1) == "location:" { url = $2 } END { print url }') || fail 'Could not find the latest stool release.'
    fi
    case "$resolved" in
        */releases/tag/*) version=${resolved##*/} ;;
        *) fail 'Could not determine the latest stool release tag.' ;;
    esac
fi
case "$version" in v*) ;; *) version="v$version" ;; esac
[[ "$version" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] || fail 'VERSION must be latest or vMAJOR.MINOR.PATCH (stable releases only).'
base="stool-$version-$target"
package="$base$suffix"
url="https://github.com/$repo/releases/download/$version"
download "$url/$package" "$work_dir/$package" || fail "Could not download $package."
download "$url/SHA256SUMS.txt" "$work_dir/SHA256SUMS.txt" || fail 'Could not download SHA256SUMS.txt; installation stopped.'
expected=$(awk -v name="$package" '$2 == name { count++; hash = $1 } END { if (count != 1) exit 1; print tolower(hash) }' "$work_dir/SHA256SUMS.txt") || fail "Expected one SHA256 checksum for $package."
[[ "$expected" =~ ^[0-9a-f]{64}$ ]] || fail 'The SHA256 checksum is malformed.'
if [ "$hasher" = sha256sum ]; then
    actual=$(sha256sum "$work_dir/$package" | awk '{ print tolower($1) }')
else
    actual=$(shasum -a 256 "$work_dir/$package" | awk '{ print tolower($1) }')
fi
[ "$actual" = "$expected" ] || fail 'SHA256 checksum mismatch; the existing stool was preserved.'

mkdir -p "$work_dir/extract"
if [ "$suffix" = .zip ]; then
    unzip -q "$work_dir/$package" "$base/$binary_name" -d "$work_dir/extract" || fail 'Could not extract the stool executable.'
else
    tar -xzf "$work_dir/$package" -C "$work_dir/extract" "$base/$binary_name" || fail 'Could not extract the stool executable.'
fi
binary="$work_dir/extract/$base/$binary_name"
[ -f "$binary" ] && [ ! -L "$binary" ] || fail 'The release archive has no regular stool executable.'
chmod 755 "$binary"
reported_version=$("$binary" --version) || fail 'The downloaded executable could not run on this machine.'
[ "$reported_version" = "stool ${version#v}" ] || fail "Executable version does not match $version: $reported_version"

install_dir="${PREFIX:-${HOME:?Set HOME or PREFIX}/.local}/bin"
mkdir -p "$install_dir" || fail "Cannot create $install_dir; set PREFIX to a directory you own."
staged_binary=$(mktemp "$install_dir/.stool-install.XXXXXX")
cp "$binary" "$staged_binary"
chmod 755 "$staged_binary"
mv -f -- "$staged_binary" "$install_dir/$binary_name"
staged_binary=''
printf 'Installed stool %s to %s/%s\n' "${version#v}" "$install_dir" "$binary_name"
case ":${PATH:-}:" in
    *":$install_dir:"*) ;;
    *) printf 'Add %s to your PATH to run stool.\n' "$install_dir" ;;
esac
