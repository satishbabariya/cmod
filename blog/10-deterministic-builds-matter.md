# Why Deterministic Builds Matter

*How cmod's mandatory lockfiles, pinned commits, and toolchain versioning eliminate "works on my machine" forever.*

---

"It works on my machine." These five words have caused more wasted engineering hours than perhaps any other phrase in software development. And in C++, where builds depend on compiler versions, system headers, link order, and preprocessor state, the problem is especially severe.

cmod was designed from day one to make deterministic builds the default, not an aspirational goal.

## What Makes a Build Non-Deterministic?

- **Floating dependency versions:** `>=10.0` resolves to `10.2.1` today and `10.3.0` tomorrow
- **Unpinned compiler versions:** Clang 17 and Clang 18 can produce different code for the same source
- **System header differences:** glibc on Ubuntu 22.04 differs from Ubuntu 24.04
- **Implicit dependencies:** A library works because a transitive header was included — until it isn't
- **Build order sensitivity:** Parallel builds can produce different results if ordering isn't determined by a DAG

## cmod's Approach: Pin Everything

### Mandatory lockfiles

`cmod resolve` generates a `cmod.lock` that records exact Git commit hashes and content hashes for every dependency, plus toolchain version information.

### The --locked flag

In CI, always build with `--locked`:

```bash
cmod build --locked --release
```

If the lockfile is missing or out of date, the build fails immediately instead of silently resolving to different versions.

### Toolchain pinning

The `[toolchain]` section specifies exact compiler requirements. cmod verifies the installed compiler matches before starting a build.

## The Module DAG: Determined Build Order

C++20 modules introduce explicit import relationships. cmod constructs a DAG and compiles in topological order — deterministic by construction.

## Content-Addressed Caching

Cache keys include source content, compiler version, flags, dependency BMI hashes, and target triple. A cache hit is a proof of equivalence.

## Real-World Impact

- **CI reliability:** Failures mean something — not flaky builds from floating deps
- **Security auditing:** Reproducible builds enable meaningful source-to-binary verification
- **Regulatory compliance:** ISO 26262, IEC 62304, DO-178C all require reproducible builds

## The CI Recipe

```yaml
- name: Build
  run: cmod build --locked --release
- name: Test
  run: cmod test --locked --release
- name: Verify
  run: cmod verify
- name: SBOM
  run: cmod sbom --output sbom.json
```

## Updating Dependencies Deliberately

```bash
cmod update              # Update all
cmod update --patch      # Patch-level only
cmod update fmt          # Update specific dep
git diff cmod.lock       # Review what changed
```

Updates are deliberate actions, not side effects of building.

[Get started with cmod →](https://github.com/satishbabariya/cmod)
