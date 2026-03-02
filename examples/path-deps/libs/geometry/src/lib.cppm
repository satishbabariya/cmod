/// @file lib.cppm
/// Primary module interface for local.geometry
///
/// Re-exports the :vec2 partition and provides geometry operations.

module;

#include <cmath>

export module local.geometry;

export import :vec2;

export namespace geometry {

/// Euclidean distance between two points.
inline auto distance(Vec2 a, Vec2 b) -> float {
    float dx = b.x - a.x;
    float dy = b.y - a.y;
    return std::sqrt(dx * dx + dy * dy);
}

/// Linear interpolation between two points.
inline auto lerp(Vec2 a, Vec2 b, float t) -> Vec2 {
    return Vec2{
        a.x + (b.x - a.x) * t,
        a.y + (b.y - a.y) * t,
    };
}

} // namespace geometry
