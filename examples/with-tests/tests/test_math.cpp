import local.with_tests;
#include <cassert>
#include <iostream>

int main() {
    // Factorial base cases
    assert(math::factorial(0) == 1);
    assert(math::factorial(1) == 1);

    // Factorial known values
    assert(math::factorial(5) == 120);
    assert(math::factorial(6) == 720);
    assert(math::factorial(10) == 3628800);
    assert(math::factorial(12) == 479001600ULL);

    // Addition commutativity
    assert(math::add(3, 7) == math::add(7, 3));
    assert(math::add(-5, 10) == math::add(10, -5));

    // Multiplication commutativity
    assert(math::multiply(6, 9) == math::multiply(9, 6));
    assert(math::multiply(-4, 3) == math::multiply(3, -4));

    // Multiplication identity
    assert(math::multiply(42, 1) == 42);
    assert(math::multiply(1, 99) == 99);

    std::cout << "test_math: all assertions passed\n";
    return 0;
}
