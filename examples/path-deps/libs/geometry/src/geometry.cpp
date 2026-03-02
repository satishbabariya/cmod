/// @file geometry.cpp
/// Module implementation for local.geometry

module local.geometry;

import <cmath>;

namespace geometry {

auto distance(Vec2 a, Vec2 b) -> float {
    float dx = b.x - a.x;
    float dy = b.y - a.y;
    return std::sqrt(dx * dx + dy * dy);
}

auto lerp(Vec2 a, Vec2 b, float t) -> Vec2 {
    return Vec2{
        a.x + (b.x - a.x) * t,
        a.y + (b.y - a.y) * t,
    };
}

} // namespace geometry
