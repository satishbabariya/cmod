//! LSP server for cmod — module-aware IDE integration.
//!
//! Implements RFC-0010 and RFC-0016:
//! - Language Server Protocol (LSP) communication over stdio
//! - Module-aware code completion
//! - Real-time build diagnostics
//! - Module graph navigation
//! - BMI prefetching for IDE responsiveness

pub mod completion;
pub mod diagnostics;
pub mod server;
