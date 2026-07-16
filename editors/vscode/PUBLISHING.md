# Publishing the cmod VS Code Extension

The packaging and publish automation is **fully built** — the
`Release VS Code Extension` workflow (`.github/workflows/release-vscode.yml`)
runs on any `vscode-v*` tag and:

1. builds platform-specific VSIXes for 7 targets (linux x64/arm64, alpine,
   darwin x64/arm64, win32 x64/arm64), each bundling the matching `cmod`
   binary;
2. builds a universal VSIX;
3. attaches all VSIXes to a GitHub Release;
4. publishes to the **VS Code Marketplace** if the `VSCE_PAT` secret is set,
   and to **Open VSX** if `OVSX_PAT` is set — both steps skip gracefully
   when the secret is absent, so releasing without marketplace accounts
   still produces installable VSIXes.

Local packaging is verified working: `npm ci && npm run compile &&
npx vsce package` produces `cmod-<version>.vsix`.

## One-time owner setup (cannot be automated)

- [ ] **VS Code Marketplace publisher**: create the `cmod` publisher at
      <https://marketplace.visualstudio.com/manage> (requires a Microsoft
      account), generate an Azure DevOps PAT with the *Marketplace →
      Manage* scope, and add it as the `VSCE_PAT` repo secret.
      The publisher id must match `"publisher": "cmod"` in `package.json` —
      if `cmod` is unavailable, change both together.
- [ ] **Open VSX** (optional, serves VSCodium/Gitpod users): create a
      namespace at <https://open-vsx.org>, generate a token, add it as
      `OVSX_PAT`.

## Releasing a version

```bash
# bump "version" in editors/vscode/package.json + update its CHANGELOG.md
git tag vscode-v<version>       # e.g. vscode-v0.1.0
git push origin vscode-v<version>
```

The tag version and `package.json` version should match. Marketplace
uploads are idempotent per version — re-releasing requires a version bump.

## Verifying a release locally

```bash
code --install-extension cmod-<version>.vsix
```

The extension activates on `cmod.toml` presence and C++ files. Binary
resolution order: the `cmod.path` setting, the bundled binary (platform
VSIXes), then `cmod` on `PATH` (universal VSIX).
