#!/bin/bash
# Push cmod.toml to all cmod-ecosystem repos
# Usage: ./push-all.sh
#
# Prerequisites:
#   - gh auth login (or git credentials configured)
#   - Write access to github.com/cmod-ecosystem repos

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WORK_DIR=$(mktemp -d)
trap 'rm -rf "$WORK_DIR"' EXIT

declare -A BRANCHES=(
  [boost]=master
  [fmt]=master
  [json]=develop
  [spdlog]=v1.x
  [googletest]=main
  [abseil-cpp]=master
  [Catch2]=devel
  [CLI11]=main
  [rapidjson]=master
  [folly]=main
)

for repo in "${!BRANCHES[@]}"; do
  branch="${BRANCHES[$repo]}"
  config_file="$SCRIPT_DIR/${repo}.cmod.toml"

  if [ ! -f "$config_file" ]; then
    echo "SKIP $repo: no config file found at $config_file"
    continue
  fi

  echo "=== $repo ($branch) ==="
  cd "$WORK_DIR"
  git clone --depth 1 -b "$branch" "https://github.com/cmod-ecosystem/${repo}.git" "$repo"
  cd "$repo"
  cp "$config_file" cmod.toml
  git add cmod.toml
  git commit -m "Add cmod.toml for cmod package manager integration

Generated via \`cmod migrate cmake\` from existing CMakeLists.txt.
Enables this library to be used as a cmod dependency."
  git push origin "$branch"
  cd "$WORK_DIR"
  rm -rf "$repo"
  echo
done

echo "Done! All cmod.toml files pushed."
