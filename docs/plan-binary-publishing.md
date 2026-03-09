# Binary Publishing Plan for cmod + Editor Extensions

## Overview

This plan covers three publishing pipelines:
1. **cmod CLI binaries** — 7 platform targets via GitHub Releases
2. **VS Code extension** — VS Code Marketplace, Open VSX Registry, GitHub Releases
3. **CLion/IntelliJ plugin** — JetBrains Marketplace, GitHub Releases

All extensions use **auto-download**: on first activation (or version mismatch), the extension downloads the matching cmod binary from GitHub Releases.

---

## 1. cmod CLI Binary Publishing

### Targets (7 total)

| Target Triple | OS | Arch | Runner | Notes |
|---|---|---|---|---|
| `x86_64-unknown-linux-gnu` | Linux | x64 | `ubuntu-latest` | Existing |
| `aarch64-unknown-linux-gnu` | Linux | ARM64 | `ubuntu-latest` | Existing, cross-compiled |
| `x86_64-unknown-linux-musl` | Linux | x64 | `ubuntu-latest` | **New**, static binary |
| `x86_64-apple-darwin` | macOS | x64 | `macos-13` | Existing |
| `aarch64-apple-darwin` | macOS | ARM64 | `macos-14` | Existing |
| `x86_64-pc-windows-msvc` | Windows | x64 | `windows-latest` | **New** |
| `aarch64-pc-windows-msvc` | Windows | ARM64 | `windows-latest` | **New**, cross-compiled |

### Artifact naming

```
cmod-{version}-{target}.tar.gz      # Linux, macOS
cmod-{version}-{target}.zip         # Windows
```

### Checksums

Generate `checksums-{version}.sha256` containing SHA-256 hashes of all artifacts. Attach to the GitHub Release.

### Changes to `release.yml`

- Expand the build matrix to 7 targets
- Install `musl-tools` for the musl target
- Add Windows targets with `.zip` packaging (not `.tar.gz`)
- Add a checksum generation job that runs after all builds complete
- Add code signing steps (placeholder, configurable via secrets):
  - macOS: `codesign` with Developer ID (optional)
  - Windows: `signtool` with Authenticode cert (optional)

---

## 2. VS Code Extension Publishing

### Build & Packaging

The VS Code extension is a standard TypeScript/Webpack project. It produces a `.vsix` package.

**Platform-specific VSIX:** Use `@vscode/vsce`'s `--target` flag to produce platform-specific extensions. This enables the VS Code Marketplace to serve the right package per platform. Targets:
- `linux-x64`, `linux-arm64`, `alpine-x64` (musl libc)
- `darwin-x64`, `darwin-arm64`
- `win32-x64`, `win32-arm64`

Each platform-specific VSIX bundles the matching cmod binary for zero-config experience on marketplace installs. For GitHub Release `.vsix` files, a universal (non-platform-specific) VSIX is also produced that uses auto-download.

### Auto-download mechanism (for universal VSIX / fallback)

On activation, the extension:
1. Checks if `cmod` exists at the expected location (`~/.cmod/bin/cmod` or extension storage)
2. Checks version matches (`cmod --version` vs extension's expected version)
3. If missing or outdated:
   - Detects platform/arch via `process.platform` + `process.arch`
   - Fetches the GitHub Releases API: `GET /repos/satishbabariya/cmod/releases/tags/v{version}`
   - Downloads the matching tarball/zip
   - Verifies SHA-256 checksum against `checksums-{version}.sha256`
   - Extracts binary to `globalStorageUri/bin/cmod`
   - Sets executable permissions on Unix
4. Adds the binary path to the extension's LSP server configuration

### Publishing targets

| Target | Method | Secrets Required |
|---|---|---|
| VS Code Marketplace | `vsce publish` | `VSCE_PAT` (Personal Access Token) |
| Open VSX Registry | `ovsx publish` | `OVSX_PAT` (Personal Access Token) |
| GitHub Release | Upload `.vsix` as release asset | `GITHUB_TOKEN` (automatic) |

### Workflow: `.github/workflows/release-vscode.yml`

**Trigger:** Tag push matching `vscode-v*` (e.g., `vscode-v0.1.0`)

**Jobs:**

1. **build-platform-vsix** (matrix: 7 VS Code targets)
   - Checkout
   - Setup Node.js 20
   - `npm ci` in `editors/vscode/`
   - Download matching cmod binary from the GitHub Release (requires cmod release first)
   - Place binary in `editors/vscode/bin/cmod` (or `cmod.exe`)
   - `vsce package --target {target}` → produces `cmod-{target}-{version}.vsix`
   - Upload as artifact

2. **build-universal-vsix**
   - Same as above but no `--target` flag, no bundled binary
   - Produces `cmod-{version}.vsix` (universal, uses auto-download)

3. **publish**
   - Download all VSIX artifacts
   - `vsce publish` (each platform-specific VSIX) → VS Code Marketplace
   - `ovsx publish` (universal VSIX) → Open VSX
   - Upload all `.vsix` files to GitHub Release

### Version coordination

The VS Code extension's `package.json` version and its expected cmod version are kept in sync. The extension stores the compatible cmod version in `package.json` under a custom field:

```json
{
  "version": "0.1.0",
  "cmod": {
    "binaryVersion": "0.1.0"
  }
}
```

---

## 3. CLion/IntelliJ Plugin Publishing

### Build & Packaging

The CLion plugin is a Gradle/Kotlin project. It produces a `.zip` file via `./gradlew buildPlugin`.

Unlike VS Code, JetBrains plugins are platform-independent (JVM-based), so a single `.zip` is published. The auto-download mechanism handles platform-specific binary acquisition at runtime.

### Auto-download mechanism

On IDE startup (or plugin activation), the plugin:
1. Checks if `cmod` exists at the expected location (`~/.cmod/bin/cmod`)
2. Checks version via `cmod --version`
3. If missing/outdated:
   - Detects OS/arch via `System.getProperty("os.name")` + `System.getProperty("os.arch")`
   - Downloads from GitHub Releases (same API as VS Code)
   - Verifies SHA-256 checksum
   - Extracts to plugin data directory (`PathManager.getPluginDataPath()`)
   - Sets executable bit on Unix via `File.setExecutable(true)`
4. Configures the LSP server to use the downloaded binary

### Publishing targets

| Target | Method | Secrets Required |
|---|---|---|
| JetBrains Marketplace | `./gradlew publishPlugin` | `JETBRAINS_PUBLISH_TOKEN` |
| GitHub Release | Upload `.zip` as release asset | `GITHUB_TOKEN` |

### Workflow: `.github/workflows/release-clion.yml`

**Trigger:** Tag push matching `clion-v*` (e.g., `clion-v0.1.0`)

**Jobs:**

1. **build**
   - Checkout
   - Setup JDK 17
   - `./gradlew buildPlugin` in `editors/clion/`
   - Upload `.zip` artifact

2. **publish**
   - Download artifact
   - `./gradlew publishPlugin` → JetBrains Marketplace
   - Upload `.zip` to GitHub Release

### Version coordination

The plugin's `build.gradle.kts` stores the compatible cmod version:

```kotlin
val cmodBinaryVersion = "0.1.0"
```

This is embedded in the plugin resources and used by the auto-download logic.

---

## 4. Release Orchestration

### Tag conventions

| Component | Tag pattern | Example |
|---|---|---|
| cmod CLI | `v*` | `v0.1.0` |
| VS Code extension | `vscode-v*` | `vscode-v0.1.0` |
| CLion plugin | `clion-v*` | `clion-v0.1.0` |

### Release order

For a coordinated release:
1. Push `v0.1.0` tag → builds + publishes cmod binaries to GitHub Release
2. Wait for release to complete
3. Push `vscode-v0.1.0` tag → builds VS Code extension (downloads cmod from step 1), publishes
4. Push `clion-v0.1.0` tag → builds CLion plugin, publishes

Extensions can also be released independently (e.g., bug fixes that don't require a new cmod binary).

### Future: unified release workflow

A single `release-all.yml` workflow triggered by `v*` tags could orchestrate all three in sequence using workflow dispatch or job dependencies. Defer this until the independent workflows are proven.

---

## 5. Auto-download Implementation Details

### Shared logic

Both extensions implement the same download logic. Key design decisions:

**Download URL pattern:**
```
https://github.com/satishbabariya/cmod/releases/download/v{version}/cmod-v{version}-{target}.tar.gz
https://github.com/satishbabariya/cmod/releases/download/v{version}/cmod-v{version}-{target}.zip
```

**Platform mapping:**

| Extension platform | cmod target triple |
|---|---|
| VS Code `linux-x64` / CLion `Linux x86_64` | `x86_64-unknown-linux-gnu` |
| VS Code `linux-arm64` / CLion `Linux aarch64` | `aarch64-unknown-linux-gnu` |
| VS Code `alpine-x64` | `x86_64-unknown-linux-musl` |
| VS Code `darwin-x64` / CLion `Mac OS X x86_64` | `x86_64-apple-darwin` |
| VS Code `darwin-arm64` / CLion `Mac OS X aarch64` | `aarch64-apple-darwin` |
| VS Code `win32-x64` / CLion `Windows x86_64` | `x86_64-pc-windows-msvc` |
| VS Code `win32-arm64` / CLion `Windows aarch64` | `aarch64-pc-windows-msvc` |

**Checksum verification:**
1. Download `checksums-{version}.sha256` from the release
2. Compute SHA-256 of downloaded archive
3. Compare against expected hash
4. Reject on mismatch with user-facing error

**User experience:**
- Show progress notification during download
- Allow cancellation
- Provide a "Download failed" error with link to manual install docs
- Respect proxy settings (`http.proxy` in VS Code, IDE proxy in CLion)
- Support `cmod.binaryPath` setting to override auto-download with a user-supplied path

**Binary storage location:**
- VS Code: `globalStorageUri/bin/cmod` (managed by VS Code, survives extension updates)
- CLion: `PathManager.getPluginDataPath()/cmod/bin/cmod`

---

## 6. Files to Create/Modify

### New files

| File | Purpose |
|---|---|
| `.github/workflows/release-vscode.yml` | VS Code extension release workflow |
| `.github/workflows/release-clion.yml` | CLion plugin release workflow |
| `editors/vscode/src/utils/binaryManager.ts` | Auto-download logic for VS Code |
| `editors/clion/src/main/kotlin/com/cmod/intellij/binary/BinaryManager.kt` | Auto-download logic for CLion |

### Modified files

| File | Changes |
|---|---|
| `.github/workflows/release.yml` | Add Windows + musl targets, checksum job, code signing placeholders |
| `editors/vscode/package.json` | Add `cmod.binaryVersion`, `cmod.binaryPath` setting |
| `editors/vscode/src/extension.ts` | Call binary manager on activation |
| `editors/clion/build.gradle.kts` | Add `cmodBinaryVersion` property |
| `editors/clion/src/main/resources/META-INF/plugin.xml` | Register binary manager service |

---

## 7. Security Considerations

- **Checksum verification** on every download (SHA-256)
- **HTTPS only** for all downloads
- **No arbitrary code execution** — only the verified cmod binary is executed
- **User consent** — show notification before downloading, allow opt-out via `cmod.binaryPath`
- **Future:** GPG signature verification on release artifacts (cmod-security already has the primitives)

---

## 8. Open Questions

1. **Homebrew / winget / APT?** Should we also publish to OS package managers? (Deferred — GitHub Releases is sufficient for now)
2. **cargo-binstall support?** Adding `[package.metadata.binstall]` to Cargo.toml enables `cargo binstall cmod`. Low effort, nice-to-have.
3. **Nightly/pre-release channel?** Could publish from `main` branch pushes. Deferred.
4. **Extension telemetry?** Track auto-download success/failure rates. Deferred.
