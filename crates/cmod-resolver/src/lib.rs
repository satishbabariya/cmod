pub mod features;
pub mod git;
pub mod resolver;
pub mod version;

pub use resolver::{Resolver, VersionConflict};
