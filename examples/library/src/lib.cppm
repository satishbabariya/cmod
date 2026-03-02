/// @file lib.cppm
/// Primary module interface for local.math
///
/// Re-exports all partitions so consumers only need `import local.math;`

export module local.math;

export import :ops;
export import :stats;
