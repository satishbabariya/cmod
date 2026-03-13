#!/bin/sh
# cmod installer — downloads the latest (or specified) cmod binary from GitHub Releases.
#
# Usage:
#   curl -sSf https://raw.githubusercontent.com/satishbabariya/cmod/main/install.sh | sh
#   curl -sSf https://raw.githubusercontent.com/satishbabariya/cmod/main/install.sh | sh -s -- --version v0.1.0
#   curl -sSf https://raw.githubusercontent.com/satishbabariya/cmod/main/install.sh | sh -s -- --to /usr/local/bin

set -eu

REPO="satishbabariya/cmod"
INSTALL_DIR="${HOME}/.cmod/bin"
VERSION=""

usage() {
    cat <<EOF
cmod installer

Usage:
    install.sh [OPTIONS]

Options:
    --version <VERSION>   Install a specific version (e.g., v0.1.0)
    --to <DIR>            Install to a custom directory (default: ~/.cmod/bin)
    -h, --help            Show this help message
EOF
}

say() {
    printf 'cmod-install: %s\n' "$1"
}

err() {
    say "ERROR: $1" >&2
    exit 1
}

need_cmd() {
    if ! command -v "$1" > /dev/null 2>&1; then
        err "need '$1' (command not found)"
    fi
}

detect_target() {
    local _os _arch _target

    _os="$(uname -s)"
    _arch="$(uname -m)"

    case "$_os" in
        Linux)
            case "$_arch" in
                x86_64|amd64)   _target="x86_64-unknown-linux-gnu" ;;
                aarch64|arm64)  _target="aarch64-unknown-linux-gnu" ;;
                *)              err "unsupported architecture: $_arch" ;;
            esac
            ;;
        Darwin)
            case "$_arch" in
                x86_64|amd64)   _target="x86_64-apple-darwin" ;;
                aarch64|arm64)  _target="aarch64-apple-darwin" ;;
                *)              err "unsupported architecture: $_arch" ;;
            esac
            ;;
        MINGW*|MSYS*|CYGWIN*)
            err "use PowerShell on Windows: see https://github.com/${REPO}#install"
            ;;
        *)
            err "unsupported OS: $_os"
            ;;
    esac

    echo "$_target"
}

get_latest_version() {
    local _url _version

    _url="https://api.github.com/repos/${REPO}/releases/latest"

    if command -v curl > /dev/null 2>&1; then
        _version="$(curl -sSf "$_url" | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"//;s/".*//')"
    elif command -v wget > /dev/null 2>&1; then
        _version="$(wget -qO- "$_url" | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"//;s/".*//')"
    else
        err "need 'curl' or 'wget' to download"
    fi

    if [ -z "$_version" ]; then
        err "could not determine latest version"
    fi

    echo "$_version"
}

download() {
    local _url="$1" _output="$2"

    if command -v curl > /dev/null 2>&1; then
        curl -sSfL "$_url" -o "$_output"
    elif command -v wget > /dev/null 2>&1; then
        wget -qO "$_output" "$_url"
    else
        err "need 'curl' or 'wget' to download"
    fi
}

verify_checksum() {
    local _archive="$1" _checksums="$2" _filename

    _filename="$(basename "$_archive")"

    if command -v sha256sum > /dev/null 2>&1; then
        grep "$_filename" "$_checksums" | (cd "$(dirname "$_archive")" && sha256sum -c --quiet -)
    elif command -v shasum > /dev/null 2>&1; then
        grep "$_filename" "$_checksums" | (cd "$(dirname "$_archive")" && shasum -a 256 -c --quiet -)
    else
        say "warning: cannot verify checksum (sha256sum/shasum not found)"
        return 0
    fi
}

main() {
    # Parse arguments
    while [ $# -gt 0 ]; do
        case "$1" in
            --version)
                VERSION="$2"
                shift 2
                ;;
            --to)
                INSTALL_DIR="$2"
                shift 2
                ;;
            -h|--help)
                usage
                exit 0
                ;;
            *)
                err "unknown option: $1"
                ;;
        esac
    done

    local _target _version _archive _url _checksum_url _tmpdir

    say "detecting platform..."
    _target="$(detect_target)"
    say "target: $_target"

    if [ -n "$VERSION" ]; then
        _version="$VERSION"
    else
        say "fetching latest version..."
        _version="$(get_latest_version)"
    fi
    say "version: $_version"

    _archive="cmod-${_version}-${_target}.tar.gz"
    _url="https://github.com/${REPO}/releases/download/${_version}/${_archive}"
    _checksum_url="https://github.com/${REPO}/releases/download/${_version}/checksums-${_version}.sha256"

    _tmpdir="$(mktemp -d)"
    trap 'rm -rf "$_tmpdir"' EXIT

    say "downloading ${_archive}..."
    download "$_url" "${_tmpdir}/${_archive}"

    say "downloading checksums..."
    download "$_checksum_url" "${_tmpdir}/checksums.sha256"

    say "verifying checksum..."
    if verify_checksum "${_tmpdir}/${_archive}" "${_tmpdir}/checksums.sha256"; then
        say "checksum verified"
    else
        err "checksum verification failed"
    fi

    say "extracting..."
    tar xzf "${_tmpdir}/${_archive}" -C "${_tmpdir}"

    say "installing to ${INSTALL_DIR}..."
    mkdir -p "$INSTALL_DIR"
    mv "${_tmpdir}/cmod" "${INSTALL_DIR}/cmod"
    chmod +x "${INSTALL_DIR}/cmod"

    say "installed cmod ${_version} to ${INSTALL_DIR}/cmod"

    # Check if INSTALL_DIR is in PATH
    case ":${PATH}:" in
        *":${INSTALL_DIR}:"*)
            ;;
        *)
            say ""
            say "add cmod to your PATH by adding this to your shell profile:"
            say ""
            say "  export PATH=\"${INSTALL_DIR}:\$PATH\""
            say ""
            ;;
    esac

    say "run 'cmod --help' to get started"
}

main "$@"
