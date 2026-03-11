#include <iostream>
#include <cmath>
#include <cassert>

import local.header_only;

int main() {
    // Circle calculations
    double area = app::circle_area(5.0);
    double circumference = app::circle_circumference(5.0);

    std::cout << "Circle (r=5):\n";
    std::cout << "  Area:          " << area << "\n";
    std::cout << "  Circumference: " << circumference << "\n";

    // Smoothstep
    std::cout << "\nSmoothstep(0, 1, x):\n";
    for (double x = 0.0; x <= 1.0; x += 0.25) {
        std::cout << "  x=" << x << " -> " << app::smoothstep(0.0, 1.0, x) << "\n";
    }

    // Polar to cartesian (90 degrees, radius 1 -> should be ~(0, 1))
    double x = app::polar_to_x(90.0, 1.0);
    double y = app::polar_to_y(90.0, 1.0);
    assert(std::abs(x) < 1e-10);
    assert(std::abs(y - 1.0) < 1e-10);
    std::cout << "\nPolar(90deg, r=1) -> (" << x << ", " << y << ")\n";

    return 0;
}
