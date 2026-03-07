# Build Cache

cmod uses a content-addressed artifact cache to avoid redundant compilation. This guide covers how caching works, cache management, and remote caching.

## How Caching Works

cmod caches compiled artifacts (BMIs, object files) using content-addressed SHA-256 keys. When a module needs to be compiled, cmod checks the cache first — if a matching entry exists, it's reused instead of recompiling.

### Cache Key Composition

A cache key is computed from:

- **Module source hash** — SHA-256 of the source file content
- **Dependency lock hash** — hash of imported BMIs and their versions
- **Compiler identity** — compiler name and version (e.g., `clang-18.1.0`)
- **C++ standard** — e.g., `20` or `23`
- **Standard library** — e.g., `libc++` or `libstdc++`
- **Target triple** — e.g., `x86_64-unknown-linux-gnu`
- **Build flags** — any extra compiler flags

The key format is: `<compiler>-<version>-std<standard>-<stdlib>-<target>`

If any component changes, a new cache entry is created.

## Local Cache

### Default location

The local cache is stored at:

- **Linux:** `~/.cache/cmod/`
- **macOS:** `~/Library/Caches/cmod/`
- **Windows:** `%LOCALAPPDATA%\cmod\`

Or wherever `$XDG_CACHE_HOME` or the system cache directory points.

### Custom cache path

Override in `cmod.toml`:

```toml
[cache]
local_path = "/custom/cache/path"
```

### Disabling the cache

Skip cache lookups for a single build:

```bash
cmod build --no-cache
```

## Cache Commands

### View cache status

```bash
cmod cache status           # Human-readable status and size
cmod cache status-json      # Machine-readable JSON output
```

### Clean the cache

```bash
cmod cache clean            # Remove all cache entries
```

### Garbage collection

Evict old and oversized entries based on TTL and size limits:

```bash
cmod cache gc
```

### Inspect a cache entry

```bash
cmod cache inspect <MODULE> <KEY>
```

### Export a cached module

Export a cached module as a BMI package:

```bash
cmod cache export <MODULE> <KEY> -o /path/to/output
```

### Import a BMI package

```bash
cmod cache import /path/to/package
```

## Remote Caching

Share build artifacts across team members and CI systems.

### Configuration

```toml
[cache]
shared_url = "https://cache.example.com"
auth_token_env = "CMOD_CACHE_AUTH_TOKEN"
timeout = 30           # HTTP timeout in seconds (default: 30)
retries = 3            # Retry attempts (default: 3)
compression = true     # Compress with zstd before upload (default: true)
```

The authentication token is read from the environment variable specified by `auth_token_env` — never store secrets in `cmod.toml`.

### Override from CLI

```bash
cmod build --remote-cache https://cache.example.com
```

### Push and pull

```bash
cmod cache push    # Upload local cache entries to remote
cmod cache pull    # Download cache entries from remote
```

## Cache Size Management

### TTL (Time-to-Live)

Set how long cache entries are kept:

```toml
[cache]
ttl = "7d"         # 7 days
# ttl = "24h"      # 24 hours
# ttl = "30m"      # 30 minutes
```

### Maximum size

Limit the total cache size:

```toml
[cache]
max_size = "1G"    # 1 gigabyte
# max_size = "500M"  # 500 megabytes
```

When the limit is reached, `cmod cache gc` evicts the oldest entries (LRU).

## CI/CD Integration

For CI pipelines, a typical cache workflow:

```bash
# Pull shared cache before building
cmod cache pull

# Build with locked dependencies
cmod build --locked --verify

# Push new artifacts to shared cache
cmod cache push
```

Use `--no-cache` to ensure a clean build when needed:

```bash
cmod build --no-cache --locked
```
