#!/bin/bash
# Push cmod.toml to all cmod-ecosystem repos on the cmod-support branch
# Usage: ./push-all.sh
#
# Prerequisites:
#   - gh auth login (or git credentials configured)
#   - Write access to github.com/cmod-ecosystem repos

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WORK_DIR=$(mktemp -d)
trap 'rm -rf "$WORK_DIR"' EXIT

TARGET_BRANCH="cmod-support"

# Map repo name -> default branch (used as base for cmod-support)
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
  default_branch="${BRANCHES[$repo]}"
  config_file="$SCRIPT_DIR/${repo}.cmod.toml"

  if [ ! -f "$config_file" ]; then
    echo "SKIP $repo: no config file found at $config_file"
    continue
  fi

  echo "=== $repo (${default_branch} -> ${TARGET_BRANCH}) ==="
  cd "$WORK_DIR"

  if ! git clone --depth 1 -b "$default_branch" "https://github.com/cmod-ecosystem/${repo}.git" "$repo" 2>&1; then
    echo "SKIP $repo: clone failed"
    continue
  fi

  cd "$repo"

  # Create or update the cmod-support branch
  if git ls-remote --exit-code --heads origin "$TARGET_BRANCH" >/dev/null 2>&1; then
    git fetch origin "$TARGET_BRANCH"
    git checkout "$TARGET_BRANCH"
  else
    git checkout -b "$TARGET_BRANCH"
  fi

  cp "$config_file" cmod.toml
  git add cmod.toml
  if ! git diff --staged --quiet --exit-code cmod.toml; then
    git commit -m "Add cmod.toml for cmod package manager integration

Generated via \`cmod migrate cmake\` from existing CMakeLists.txt.
Enables this library to be used as a cmod dependency."
    git push origin "$TARGET_BRANCH"
    echo "OK $repo: pushed to $TARGET_BRANCH"
  else
    echo "SKIP $repo: no changes to commit"
  fi
  cd "$WORK_DIR"
  rm -rf "$repo"
  echo
done

echo "Done! All cmod.toml files pushed to $TARGET_BRANCH."
