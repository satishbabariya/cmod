#!/usr/bin/env bash
# Edit a header that a module interface and a plain TU both include, and
# check that the next build, and a --force build that goes through the
# cache, link the new value. Alpha.6 printed the old one.
#
# Usage: scripts/header-rebuild-smoke.sh <path-to-cmod> <clang|gcc|msvc>
set -euo pipefail

CMOD=$1
COMPILER=$2

dir=$(mktemp -d)
cd "$dir"
"$CMOD" init --name hdrsmoke
sed -i.bak "s/compiler = \"clang\"/compiler = \"$COMPILER\"/" cmod.toml

printf '#define VALUE 1\n' > src/value.h
cat > src/lib.cppm <<'EOF'
module;
#include "value.h"
export module local.hdrsmoke;
export int module_value() { return VALUE; }
EOF
cat > src/plain.cpp <<'EOF'
#include "value.h"
int plain_value() { return VALUE * 10; }
EOF
cat > src/main.cpp <<'EOF'
import local.hdrsmoke;
#include <cstdio>
int plain_value();
int main() { std::printf("%d %d\n", module_value(), plain_value()); }
EOF

check() {
  local out
  out=$(./build/debug/hdrsmoke | tr -d '\r')
  if [ "$out" != "$1" ]; then
    echo "::error::$COMPILER: expected '$1', got '$out' ($2)"
    exit 1
  fi
  echo "ok: $2 -> $out"
}

"$CMOD" build --verbose
check "1 10" "first build"

sleep 1
printf '#define VALUE 2\n' > src/value.h
"$CMOD" build --verbose
check "2 20" "build after header edit"

"$CMOD" build --force --verbose
check "2 20" "--force build (cache)"
