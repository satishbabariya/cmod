/// @file lib.cppm
/// Module interface for local.colors
///
/// RGBA color type with constexpr blending and packing operations.

export module local.colors;

import <cstdint>;
import <algorithm>;

export namespace colors {

/// An RGBA color with 8-bit channels.
struct Color {
    uint8_t r = 0;
    uint8_t g = 0;
    uint8_t b = 0;
    uint8_t a = 255;
};

/// Linear interpolation between two colors.
inline auto lerp(Color c1, Color c2, float t) -> Color {
    auto mix = [t](uint8_t a, uint8_t b) -> uint8_t {
        float result = static_cast<float>(a) + (static_cast<float>(b) - static_cast<float>(a)) * t;
        return static_cast<uint8_t>(std::clamp(result, 0.0f, 255.0f));
    };
    return Color{mix(c1.r, c2.r), mix(c1.g, c2.g), mix(c1.b, c2.b), mix(c1.a, c2.a)};
}

/// Pack a color into a 32-bit ARGB integer.
constexpr auto to_argb(Color c) -> uint32_t {
    return (static_cast<uint32_t>(c.a) << 24) |
           (static_cast<uint32_t>(c.r) << 16) |
           (static_cast<uint32_t>(c.g) << 8) |
           static_cast<uint32_t>(c.b);
}

} // namespace colors
