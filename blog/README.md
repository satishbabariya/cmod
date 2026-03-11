# cmod Blog & Marketing Content

This directory contains blog posts and marketing materials for cmod.

## Blog Posts

| # | Title | Audience | Purpose |
|---|-------|----------|---------|
| 01 | [Introducing cmod](01-introducing-cmod.md) | General / C++ community | Launch announcement |
| 02 | [Why C++ Needs a Modern Package Manager](02-why-cpp-needs-a-modern-package-manager.md) | Technical decision-makers | Problem/solution deep-dive |
| 03 | [cmod vs Existing Tools](03-cmod-vs-existing-tools.md) | Developers evaluating options | Honest competitive comparison |
| 04 | [Getting Started with cmod](04-getting-started-with-cmod.md) | New users | Hands-on tutorial |
| 05 | [Supply Chain Security for C++](05-supply-chain-security-for-cpp.md) | Security-conscious teams | Security features deep-dive |
| 06 | [Git as a Package Registry](06-git-as-registry.md) | Technical architects | Design rationale for Git-native deps |
| 07 | [Building a C++ Tool in Rust](07-building-cmod-in-rust.md) | Rust & C++ developers | Why Rust, architecture, lessons learned |
| 08 | [Why Deterministic Builds Matter](08-deterministic-builds-matter.md) | DevOps / CI engineers | Lockfiles, pinned commits, reproducibility |
| 09 | [C++20 Modules Explained](09-cpp-modules-explained.md) | C++ developers new to modules | Practical module tutorial |
| 10 | [Understanding cmod's Cache](10-understanding-cmod-cache.md) | Performance-focused developers | Content-addressed caching deep-dive |
| 11 | [Monorepo Patterns with Workspaces](11-workspace-monorepo-patterns.md) | Teams scaling C++ projects | Workspace setup and patterns |
| 12 | [Migrating CMake to cmod](12-migrating-cmake-to-cmod.md) | CMake users considering migration | Step-by-step migration guide |
| 13 | [cmod and AI-Assisted Development](13-cmod-and-ai-assisted-development.md) | AI-curious developers | How cmod enables AI coding tools |

## Marketing Materials

Located in the [`marketing/`](marketing/) subdirectory:

| File | Purpose |
|------|---------|
| [Landing Page](marketing/landing-page.md) | Website / project page content |
| [Social Media Snippets](marketing/social-media-snippets.md) | Twitter/X, LinkedIn, Reddit, HN, conference talks |

## Published HTML

Blog posts 01–13 are published as HTML in [`website/blog/`](../website/blog/). The HTML files follow the same numbering scheme (e.g., `01-introducing-cmod.html`).

## Suggested Publishing Order

1. **Landing page** — Update the project website/README
2. **Launch announcement** (01) — The flagship blog post
3. **Social media** — Amplify across platforms
4. **Getting started** (04) — Give people a path to try it
5. **Technical deep-dive** (02) — For the curious and skeptical
6. **Comparison** (03) — For teams evaluating alternatives
7. **Security** (05) — For enterprise and compliance audiences

## Key Updates (March 2026)

All blog posts and marketing materials have been updated to reflect:
- **All 6 phases complete** (including Phase 5 ecosystem and Phase 6 IDE/DX)
- **780+ passing tests** (up from 750+)
- **IDE extensions** for VS Code and CLion/IntelliJ with LSP integration
- **12+ example projects** (header-only, shared-lib, ixx-modules, multi-binary, nested-deps, include-dirs added)
- **Remote caching and distributed builds** are shipped
- GitHub URLs updated to `satishbabariya/cmod`

## Target Platforms

- **Blog/Website:** Posts 01–13
- **GitHub README:** Adapted from marketing/landing-page.md
- **Twitter/X:** Thread from marketing/social-media-snippets.md
- **LinkedIn:** Long-form posts from marketing/social-media-snippets.md
- **Reddit (r/cpp, r/rust, r/programming):** Adapted from marketing/social-media-snippets.md
- **Hacker News:** Show HN from marketing/social-media-snippets.md
- **Conference CFPs:** Talk abstracts from marketing/social-media-snippets.md
