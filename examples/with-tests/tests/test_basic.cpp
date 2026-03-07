import local.with_tests;
#include <cassert>
#include <iostream>

int main() {
    // Basic addition
    assert(math::add(0, 0) == 0);
    assert(math::add(1, 2) == 3);
    assert(math::add(-1, 1) == 0);
    assert(math::add(-3, -7) == -10);

    // Basic multiplication
    assert(math::multiply(0, 5) == 0);
    assert(math::multiply(1, 1) == 1);
    assert(math::multiply(3, 4) == 12);
    assert(math::multiply(-2, 3) == -6);

    std::cout << "test_basic: all assertions passed\n";
    return 0;
}
