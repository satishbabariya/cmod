#pragma once

namespace math {

inline constexpr double pi = 3.14159265358979323846;
inline constexpr double e = 2.71828182845904523536;
inline constexpr double tau = 2.0 * pi;
inline constexpr double sqrt2 = 1.41421356237309504880;

/// Convert degrees to radians.
inline constexpr double to_radians(double degrees) {
    return degrees * pi / 180.0;
}

/// Convert radians to degrees.
inline constexpr double to_degrees(double radians) {
    return radians * 180.0 / pi;
}

/// Linear interpolation.
inline constexpr double lerp(double a, double b, double t) {
    return a + t * (b - a);
}

/// Clamp a value between min and max.
inline constexpr double clamp(double value, double min_val, double max_val) {
    return value < min_val ? min_val : (value > max_val ? max_val : value);
}

} // namespace math
