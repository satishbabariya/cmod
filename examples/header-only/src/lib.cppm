module;

#include <math/constants.h>
#include <cmath>

export module local.header_only;

export namespace app {

/// Calculate circle area using the header-only math library constants.
constexpr double circle_area(double radius) {
    return math::pi * radius * radius;
}

/// Calculate circle circumference.
constexpr double circle_circumference(double radius) {
    return math::tau * radius;
}

/// Smooth interpolation using the math library's lerp and clamp.
constexpr double smoothstep(double edge0, double edge1, double x) {
    double t = math::clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

/// Convert polar coordinates (angle in degrees, radius) to x coordinate.
double polar_to_x(double angle_deg, double radius) {
    return radius * std::cos(math::to_radians(angle_deg));
}

/// Convert polar coordinates (angle in degrees, radius) to y coordinate.
double polar_to_y(double angle_deg, double radius) {
    return radius * std::sin(math::to_radians(angle_deg));
}

} // namespace app
