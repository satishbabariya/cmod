/// @file main.cpp
/// Entry point for the path-deps example
///
/// Demonstrates using two local path dependencies: geometry and colors.

import local.geometry;
import local.colors;

import <iostream>;

int main() {
    // Geometry: compute distance and interpolation
    auto a = geometry::Vec2{0.0f, 0.0f};
    auto b = geometry::Vec2{3.0f, 4.0f};

    std::cout << "Point A: (" << a.x << ", " << a.y << ")" << std::endl;
    std::cout << "Point B: (" << b.x << ", " << b.y << ")" << std::endl;
    std::cout << "Distance: " << geometry::distance(a, b) << std::endl;

    auto mid = geometry::lerp(a, b, 0.5f);
    std::cout << "Midpoint: (" << mid.x << ", " << mid.y << ")" << std::endl;

    // Colors: blend two colors
    auto red = colors::Color{255, 0, 0, 255};
    auto blue = colors::Color{0, 0, 255, 255};
    auto purple = colors::lerp(red, blue, 0.5f);

    std::cout << "Red:    ARGB = 0x" << std::hex << colors::to_argb(red) << std::endl;
    std::cout << "Blue:   ARGB = 0x" << colors::to_argb(blue) << std::endl;
    std::cout << "Purple: ARGB = 0x" << colors::to_argb(purple) << std::endl;

    return 0;
}
