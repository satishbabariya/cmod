# Hosting a Remote Cache Server

`cmod` can share compiled artifacts (PCMs and object files) across machines
through any HTTP server that speaks a three-verb protocol. There is no
required server software — a static file server with PUT support is enough.
This page documents the protocol and gives tested recipes.

See [Caching](caching.md) for client-side configuration (`[cache]` in
`cmod.toml`, TTL, compression).

## The protocol

All requests are rooted at `<shared_url>/cache/`:

| Request | Meaning | Expected response |
|---------|---------|-------------------|
| `HEAD /cache/<module>/<key>` | Does this cache entry exist? | `200` if present, `404` otherwise |
| `GET /cache/<module>/<key>/<artifact>` | Fetch one artifact file | `200` + bytes, or `404` |
| `PUT /cache/<module>/<key>/<artifact>` | Store one artifact file | any `2xx`; body is `application/octet-stream` |

- `<module>` is the module id (e.g. `local.mylib`, `github.fmtlib.fmt`).
  Partition modules contain a colon (`local.mathlib:ops`) — the server must
  accept `:` in paths (nginx, Caddy, and plain filesystems all do).
- `<key>` is a 64-char hex SHA-256 cache key.
- `<artifact>` is a flat filename (`<module>.pcm`, `<module>.o`,
  `metadata.json`).

If `[cache] auth_token_env` is configured, every request carries
`Authorization: Bearer <token>`.

How the client uses it:

- **`cmod build`** (with `[cache] shared_url` set, or `--remote-cache <URL>`)
  transparently GETs artifacts on a local cache miss and PUTs them after a
  successful compile — this is the main path and needs no extra commands.
- **`cmod cache push [--remote <URL>]`** uploads the entire local cache.
- **`cmod cache pull [--remote <URL>]`** pre-fetches artifacts for the
  dependencies pinned in `cmod.lock` (it is lockfile-scoped; it does not
  mirror the whole remote cache).

## Recipe: nginx (read-write)

A complete team cache on one nginx server backed by a directory:

```nginx
server {
    listen 8787;
    # server_name cache.internal.example.com;

    root /srv/cmod-cache;

    location /cache/ {
        # GET/HEAD: serve files; autoindex makes HEAD on an entry
        # directory return 200 (cmod's existence probe)
        autoindex on;

        # PUT: accept uploads, creating intermediate directories
        dav_methods PUT;
        create_full_put_path on;
        client_max_body_size 512m;

        # Optional bearer-token auth (single shared token)
        # if ($http_authorization != "Bearer YOUR-SECRET-TOKEN") {
        #     return 401;
        # }
    }
}
```

```bash
sudo mkdir -p /srv/cmod-cache/cache
sudo chown www-data /srv/cmod-cache/cache   # nginx worker user
```

Client side:

```toml
[cache]
shared_url = "http://cache.internal.example.com:8787"
auth_token_env = "CMOD_CACHE_AUTH_TOKEN"    # if the token check is enabled
```

Notes:

- `dav_methods` is in the stock `ngx_http_dav_module` (built into the
  distro packages of nginx; no third-party module needed).
- Put the server behind TLS (or a private network) before enabling it for a
  team — see [Security notes](#security-notes).

## Recipe: Caddy (read-only distribution)

Stock Caddy has no PUT handler, so use it for the common
"CI writes, developers read" topology: CI pushes over rsync/scp (or runs the
nginx recipe internally), developers consume over HTTP.

```caddyfile
cache.example.com {
    root * /srv/cmod-cache
    file_server browse
}
```

Developers configure:

```toml
[cache]
shared_url = "https://cache.example.com"
```

Reads work — `cmod build` restores artifacts on a local cache miss.
`cmod cache push` against this server fails (Caddy's file server answers
`404` to PUT) and cmod reports the failed uploads — that is the intended
read-only behavior.

Note: Caddy answers `HEAD` on a directory URL with a `308` redirect to the
trailing-slash form; cmod's HTTP client follows redirects, so the existence
probe still works with `browse` enabled.

## Recipe: local test server (Python)

For trying the protocol out or debugging, this stdlib-only script implements
all three verbs:

```python
#!/usr/bin/env python3
"""Minimal cmod remote-cache server for local testing."""
import http.server
import os
import sys

ROOT = sys.argv[1] if len(sys.argv) > 1 else "./cache-data"


class Handler(http.server.BaseHTTPRequestHandler):
    def _path(self):
        p = self.path.lstrip("/")
        parts = [s for s in p.split("/") if s and s != ".."]
        return os.path.join(ROOT, *parts[1:]) if parts[:1] == ["cache"] else None

    def do_HEAD(self):
        p = self._path()
        self.send_response(200 if p and os.path.exists(p) else 404)
        self.end_headers()

    def do_GET(self):
        p = self._path()
        if p and os.path.isfile(p):
            with open(p, "rb") as f:
                data = f.read()
            self.send_response(200)
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)
        else:
            self.send_response(404)
            self.end_headers()

    def do_PUT(self):
        p = self._path()
        if not p:
            self.send_response(400)
            self.end_headers()
            return
        data = self.rfile.read(int(self.headers.get("Content-Length", 0)))
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, "wb") as f:
            f.write(data)
        self.send_response(201)
        self.end_headers()


if __name__ == "__main__":
    http.server.HTTPServer(("127.0.0.1", 8787), Handler).serve_forever()
```

```bash
python3 cache-server.py /tmp/cmod-cache &

cmod build --remote-cache http://127.0.0.1:8787   # populates on first build
cmod cache clean                                   # drop the local cache
cmod build --remote-cache http://127.0.0.1:8787   # restores from the server
```

## Security notes

- **Only trusted writers.** Anyone who can PUT can serve compiled objects to
  every consumer — treat write access like commit access. Use the bearer
  token for writers and keep read-only mirrors for wider distribution.
- **Use TLS** for anything crossing a machine boundary; the bearer token is
  sent on every request.
- **Secrets stay in the environment.** `auth_token_env` names an environment
  variable; the token itself never appears in `cmod.toml`.
- **Cache keys are content-addressed** (sources, dependencies, compiler
  version, target, flags — see [Caching](caching.md)), so a stale server
  never causes wrong-input reuse; the risk model is malicious content, not
  staleness.
- Eviction is the server operator's job. A cron'd
  `find /srv/cmod-cache -type f -atime +30 -delete` (plus a pass to prune
  empty directories) is usually all a team cache needs.
