/// @file lib.cppm
/// Primary module interface for local.geometry
///
/// Re-exports the :vec2 partition and provides geometry operations.

export module local.geometry;

export import :vec2;

import <cmath>;

export namespace geometry {

/// Euclidean distance between two points.
auto distance(Vec2 a, Vec2 b) -> float;

/// Linear interpolation between two points.
auto lerp(Vec2 a, Vec2 b, float t) -> Vec2;

} // namespace geometry
