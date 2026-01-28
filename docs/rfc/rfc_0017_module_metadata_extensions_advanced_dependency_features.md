# RFC-0017: Module Metadata Extensions & Advanced Dependency Features

## Status
Draft

## Summary
This RFC defines **extended module metadata and advanced dependency features** for **cmod**, enhancing the expressiveness of `cmod.toml` and improving dependency resolution, versioning, and cross-platform support.

Goals:
- Extend module metadata schema
- Support conditional and platform-specific dependencies
- Improve dependency resolution with advanced constraints
- Enable richer tooling insights and verification
- Maintain backward compatibility with existing modules

---

## Motivation

Simple module metadata is insufficient for complex projects:
- Platform-specific dependencies need explicit declaration
- Optional dependencies for features or build configurations
- Conflict resolution requires version and toolchain awareness
- Metadata must be machine-readable for IDEs, build tools, and CI pipelines

---

## Extended Metadata Schema

Example `cmod.toml`:
```toml
name = "github.com/acme/math-utils"
version = "1.2.3"
license = "MIT"
toolchain = "clang-18"
target = ["x86_64-linux-gnu", "arm64-linux-gnu"]

[dependencies]
math-core = { version = ">=1.0.0 <2.0.0", optional = false }
algebra = { version = "^0.9.1", optional = true, platforms = ["x86_64-linux-gnu"] }

[features]
fast-math = { requires = ["algebra"] }

[metadata]
description = "High-performance math utilities"
authors = ["Acme Corp"]
repository = "https://github.com/acme/math-utils"
```

### Key Enhancements
- **Conditional dependencies**: Optional dependencies, features, or platform-specific modules
- **Target array**: Explicit support for multiple platforms per module
- **Feature flags**: Enable or disable optional module capabilities
- **Rich metadata**: Authors, description, repository, license, and ABI info

---

## Advanced Dependency Resolution

- Respect optional and platform-specific dependencies
- Resolve transitive dependency versions considering semver and ABI constraints
- Merge feature flags from dependent modules
- Detect conflicts and provide clear diagnostics

### Example
```
Module A depends on algebra v0.9.1 optional
Module B depends on algebra v0.9.2 required
Resolution fails: version conflict
```

- Resolution can suggest overrides or alternative compatible versions

---

## Tooling & IDE Integration

- Metadata used by IDE for module browsing, feature selection, and code completion
- Build tools leverage metadata for target-aware compilation and cache management
- IDEs can visualize optional/conditional dependencies and feature usage

---

## Backward Compatibility

- Existing `cmod.toml` files remain valid
- New features are optional and additive
- Old tooling can ignore new fields safely

---

## Open Questions

- How to handle dynamic feature selection at build time?
- Should optional dependencies be automatically fetched or on-demand?
- Policies for feature flag conflicts in transitive dependencies?

---

## Next RFCs
- RFC-0018: Optional Tooling Plugins & Ecosystem Utilities
- RFC-0019: Monorepo Management & Multi-Project Coordination

