# RFC-0015: Cmod Ecosystem Governance & Community Standards

## Status
Draft

## Summary
This RFC defines **governance and community standards** for the **cmod** ecosystem, ensuring consistency, trust, and sustainability across module creation, sharing, and consumption.

Goals:
- Establish naming and versioning conventions
- Define module publishing standards
- Encourage security and supply chain best practices
- Support community collaboration and contribution
- Provide guidelines for CI/CD and package verification

---

## Motivation

A healthy module ecosystem requires consistent rules to prevent:
- Naming collisions
- Incompatible versioning schemes
- Unsafe or unverified modules
- Fragmented community practices

Governance ensures modules remain interoperable, secure, and maintainable.

---

## Module Naming Conventions

- Module names must be globally unique (Git URL or namespace-based)
- Lowercase with dash-separated words recommended
- Example: `github.com/acme/math-utils`
- Reserved prefixes may be established for official modules
- Avoid ambiguous or misleading names

---

## Versioning Standards

- Follow RFC-0006 SemVer + ABI rules
- MAJOR.MINOR.PATCH mandatory
- Pre-1.0 versions treated conservatively
- Explicit ABI compatibility must be declared
- Lockfiles (`cmod.lock`) enforce reproducibility

---

## Publishing & Distribution

- Modules hosted via Git, optionally mirrored via HTTP/S
- Precompiled BMIs may be distributed (RFC-0011) but must include signatures
- CI pipelines encouraged for automated testing and verification
- Module metadata (`cmod.toml`) must include:
  - Name
  - Version
  - Dependencies
  - Toolchain constraints
  - ABI / target info
  - Optional license

---

## Security & Supply Chain Best Practices

- Commit signing recommended (RFC-0009)
- Artifact verification required for distributed BMIs
- Trusted mirrors encouraged
- Continuous auditing of dependencies
- Access control for private modules

---

## Contribution Guidelines

- Clear repository and branching model
- Pull requests and code reviews required
- CI validation of module interface compatibility
- Documentation of API and ABI changes
- Encouragement of reusable module design and partitioning

---

## CI/CD & Tooling Integration

- Use cmod build graphs and lockfiles to verify module correctness
- Automate dependency resolution and verification in pipelines
- Provide developer feedback on dependency or ABI issues early

---

## Community Governance

- Maintain a steering group for core standards and RFC approval
- RFC process open to community contributions
- Encourage transparency and reproducibility
- Regular reviews of ecosystem health and security

---

## Open Questions

- Should a central registry exist for official modules or purely decentralized?
- How to enforce naming and versioning compliance programmatically?
- Policies for deprecating or retiring modules?

---

## Next RFCs
- RFC-0016: Optional Language Server Protocol Enhancements & Advanced IDE Features
- RFC-0017: Module Metadata Extensions & Advanced Dependency Features