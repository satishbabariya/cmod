# RFC-IMPLEMENTATION: Status Hierarchy & Implementation Phases

## Status
Draft (Meta RFC)

## Summary
This document establishes a clear status hierarchy for all RFCs and defines implementation phases to guide the development of **cmod**. It replaces the current "Draft" status for all RFCs with a structured approach.

## RFC Status Hierarchy

### Core RFCs (Foundation)
These RFCs form the foundation and must be implemented first:
- **RFC-0001**: Cargo-style Tooling for C++ Modules (LLVM-based) - **Core**
- **RFC-0002**: Module Identity & Import Rules - **Core** 
- **RFC-0003**: Lockfiles & Reproducible Builds - **Core**
- **RFC-0004**: Build Plan IR & Module Graph Execution - **Core**
- **RFC-UNIFIED**: Unified cmod.toml Schema Specification - **Core**

### Tier 1 RFCs (Essential Features)
These build directly on Core RFCs:
- **RFC-0005**: Binary Artifacts & Distributed Build Caches - **Active**
- **RFC-0006**: Versioning, Compatibility & Lockfiles - **Active**
- **RFC-0007**: Build Graph, Incremental Compilation & Caching - **Active**
- **RFC-0008**: Toolchains, Targets & Cross-compilation - **Active**

### Tier 2 RFCs (Developer Experience)
These enhance usability but aren't required for basic functionality:
- **RFC-0009**: Security, Trust & Supply-Chain Integrity - **Draft**
- **RFC-0010**: IDE Integration & Developer Experience - **Draft**
- **RFC-0015**: cmod Ecosystem Governance & Community Standards - **Draft**
- **RFC-0016**: LSP Enhancements & Advanced IDE Features - **Draft**

### Tier 3 RFCs (Advanced Features)
These provide advanced capabilities for complex scenarios:
- **RFC-0011**: Precompiled Module Distribution - **Draft**
- **RFC-0012**: Advanced Build Strategies & Performance Optimizations - **Draft**
- **RFC-0013**: Distributed Builds & Remote Execution - **Draft**
- **RFC-0014**: Module Graph Visualization & Developer Tools Enhancements - **Draft**

### Tier 4 RFCs (Ecosystem Extensions)
These extend the ecosystem for specific use cases:
- **RFC-0017**: Module Metadata Extensions & Advanced Dependency Features - **Draft**
- **RFC-0018**: Tooling Plugins, Ecosystem & Utilities - **Draft**
- **RFC-0019**: Workspaces, Monorepos & Multi-Module Projects - **Draft**

## Implementation Phases

### Phase 1: Foundation (MVP)
**Duration: 3-4 months**
**Goal**: Basic working cmod that can build simple C++ module projects

**Implement**: Core RFCs
- RFC-0001: Basic CLI and project structure
- RFC-0002: Module naming and import resolution  
- RFC-0003: Simple lockfile support
- RFC-0004: Basic build plan execution
- RFC-UNIFIED: Core schema sections only

**Deliverables**:
- `cmod init`, `cmod build`, `cmod add`
- Basic module dependency resolution
- Simple caching
- Reproducible builds with lockfiles

### Phase 2: Production Ready
**Duration: 2-3 months**
**Goal**: Robust build system suitable for real projects

**Implement**: Tier 1 RFCs
- RFC-0005: Artifact caching and distribution
- RFC-0006: Advanced versioning and compatibility
- RFC-0007: Incremental compilation and caching
- RFC-0008: Multi-toolchain and cross-compilation support

**Deliverables**:
- Incremental builds
- Multi-platform support
- Advanced dependency management
- Shared build caches

### Phase 3: Developer Experience
**Duration: 2-3 months**
**Goal**: Excellent developer experience and IDE integration

**Implement**: Tier 2 RFCs
- RFC-0009: Security and trust features
- RFC-0010: Basic IDE integration
- RFC-0015: Basic governance standards
- RFC-0016: Enhanced LSP features

**Deliverables**:
- Language server protocol support
- Security verification
- Code completion and diagnostics
- Package publishing workflow

### Phase 4: Advanced Features
**Duration: 3-4 months**
**Goal**: Support for complex enterprise scenarios

**Implement**: Tier 3 RFCs
- RFC-0011: Precompiled module distribution
- RFC-0012: Advanced build strategies
- RFC-0013: Distributed build support
- RFC-0014: Visualization tools

**Deliverables**:
- Binary module distribution
- Distributed build systems
- Build optimization strategies
- Graph visualization tools

### Phase 5: Ecosystem Extensions
**Duration: 2-3 months**  
**Goal**: Complete ecosystem with extensibility

**Implement**: Tier 4 RFCs
- RFC-0017: Advanced metadata and features
- RFC-0018: Plugin system
- RFC-0019: Workspace and monorepo support

**Deliverables**:
- Plugin ecosystem
- Workspace support
- Advanced dependency features
- Rich metadata support

## Status Updates Required

The following RFCs need their status updated from "Draft":

**Core RFCs** → **Core**:
- RFC-0001, RFC-0002, RFC-0003, RFC-0004, RFC-UNIFIED

**Tier 1 RFCs** → **Active**:
- RFC-0005, RFC-0006, RFC-0007, RFC-0008

**Tier 2-4 RFCs** → remain **Draft**

## Dependency Graph

```
Core Foundation:
├── RFC-0001 (Basic Tooling)
├── RFC-0002 (Module Identity)
├── RFC-0003 (Lockfiles)
├── RFC-0004 (Build Plan)
└── RFC-UNIFIED (Schema)

Tier 1 (depends on Core):
├── RFC-0005 (Caching) → RFC-0003, RFC-0004
├── RFC-0006 (Versioning) → RFC-0002, RFC-0003
├── RFC-0007 (Build Graph) → RFC-0004, RFC-0005
└── RFC-0008 (Toolchains) → RFC-0004

Tier 2 (depends on Tier 1):
├── RFC-0009 (Security) → RFC-0003, RFC-0006
├── RFC-0010 (IDE) → RFC-0004, RFC-0007
├── RFC-0015 (Governance) → RFC-0006
└── RFC-0016 (LSP) → RFC-0010

Tier 3 (depends on Tier 2):
├── RFC-0011 (Distribution) → RFC-0005, RFC-0006
├── RFC-0012 (Optimization) → RFC-0004, RFC-0007
├── RFC-0013 (Distributed) → RFC-0005, RFC-0012
└── RFC-0014 (Visualization) → RFC-0004, RFC-0013

Tier 4 (depends on Tier 3):
├── RFC-0017 (Metadata) → RFC-0006, RFC-0010
├── RFC-0018 (Plugins) → RFC-0010, RFC-0016
└── RFC-0019 (Workspaces) → RFC-0003, RFC-0006
```

## Implementation Guidelines

### Status Promotion Rules
- **Core**: Implemented and battle-tested in production
- **Active**: Implementation in progress, API stable
- **Draft**: Design phase, not yet implemented
- **Deprecated**: Superseded by newer RFCs

### Phase Entry Criteria
Each phase requires:
1. Previous phase completed
2. Core dependencies stable
3. Test coverage adequate
4. Documentation updated

### Quality Gates
- All changes must pass existing tests
- New features require comprehensive tests
- Documentation updates mandatory
- Backward compatibility preserved

This phased approach ensures **cmod** develops incrementally while maintaining quality and consistency across all components.