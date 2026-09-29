#!/usr/bin/env bash
# Runs install.sh end-to-end against a fixture GitHub release list, so CI
# catches regressions in "pick the newest cmod release" without depending
# on the real GitHub API or real release binaries.
#
# The fixture lists the vscode-v0.1.0 extension tag ahead of the actual
# cmod release (v0.1.0-alpha.4) — install.sh must skip it. Every v* cmod
# release is a prerelease, so a naive /releases/latest lookup hands back
# whatever non-prerelease extension tag exists instead.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fixture_root="$(mktemp -d)"
fake_home="$(mktemp -d)"
server_pid=""

cleanup() {
    [ -n "$server_pid" ] && kill "$server_pid" 2>/dev/null || true
    rm -rf "$fixture_root" "$fake_home"
}
trap cleanup EXIT

case "$(uname -m)" in
    x86_64|amd64)  target="x86_64-unknown-linux-gnu" ;;
    aarch64|arm64) target="aarch64-unknown-linux-gnu" ;;
    *)             echo "unsupported test architecture: $(uname -m)" >&2; exit 1 ;;
esac
version="v0.1.0-alpha.4"
archive="cmod-${version}-${target}.tar.gz"

api_dir="${fixture_root}/repos/satishbabariya/cmod"
download_dir="${fixture_root}/satishbabariya/cmod/releases/download/${version}"
mkdir -p "$api_dir" "$download_dir"

# Ordered like a real GitHub response (newest first): the extension tag is
# not a prerelease, so a naive /releases/latest lookup would pick it.
cat > "${api_dir}/releases" <<'JSON'
[
  {"tag_name": "vscode-v0.1.0", "prerelease": false},
  {"tag_name": "v0.1.0-alpha.4", "prerelease": true},
  {"tag_name": "v0.1.0-alpha.3", "prerelease": true}
]
JSON

fake_bin_dir="$(mktemp -d)"
printf '#!/bin/sh\necho fake-cmod\n' > "${fake_bin_dir}/cmod"
chmod +x "${fake_bin_dir}/cmod"
tar -czf "${download_dir}/${archive}" -C "$fake_bin_dir" cmod
(cd "$download_dir" && sha256sum "$archive" > "checksums-${version}.sha256")
rm -rf "$fake_bin_dir"

port="$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')"
base_url="http://127.0.0.1:${port}"

python3 -m http.server "$port" --bind 127.0.0.1 --directory "$fixture_root" \
    > "${fixture_root}/server.log" 2>&1 &
server_pid=$!

ready=""
for _ in $(seq 1 50); do
    if curl -sSf "${base_url}/repos/satishbabariya/cmod/releases" > /dev/null 2>&1; then
        ready=1
        break
    fi
    sleep 0.1
done
if [ -z "$ready" ]; then
    echo "fixture server never came up" >&2
    cat "${fixture_root}/server.log" >&2
    exit 1
fi

set +e
output="$(HOME="$fake_home" CMOD_API_BASE="$base_url" CMOD_DOWNLOAD_BASE="$base_url" \
    sh "${repo_root}/install.sh" 2>&1)"
status=$?
set -e

echo "$output"

if [ "$status" -ne 0 ]; then
    echo "FAIL: install.sh exited $status" >&2
    exit 1
fi

if ! printf '%s' "$output" | grep -q "installed cmod ${version}"; then
    echo "FAIL: expected install.sh to report installing ${version}" >&2
    exit 1
fi

if printf '%s' "$output" | grep -qi '_tmpdir'; then
    echo "FAIL: install.sh leaked the _tmpdir scope error" >&2
    exit 1
fi

if [ ! -x "${fake_home}/.cmod/bin/cmod" ]; then
    echo "FAIL: cmod binary was not installed to \$HOME/.cmod/bin" >&2
    exit 1
fi

echo "OK: install.sh picked ${version} over the vscode-v* tag and exited cleanly"
