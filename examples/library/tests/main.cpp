/// @file tests/main.cpp
/// Assert-based tests for local.math (ops + stats partitions)

#include <array>
#include <cassert>
#include <iostream>

import local.math;

void test_ops() {
    static_assert(math::add(2, 3) == 5);
    static_assert(math::sub(10, 4) == 6);
    static_assert(math::mul(3, 7) == 21);
    static_assert(math::div(15, 3) == 5);

    assert(math::add(-1, 1) == 0);
    assert(math::mul(0, 100) == 0);
    assert(math::div(7, 2) == 3); // integer division

    std::cout << "  ops: all tests passed" << std::endl;
}

void test_stats() {
    std::array<int, 5> data = {3, 1, 4, 1, 5};

    assert(math::sum(data) == 14);
    assert(math::mean(data) == 2); // 14 / 5 = 2 (integer)
    assert(math::min_val(data) == 1);
    assert(math::max_val(data) == 5);

    // Edge case: single element
    std::array<int, 1> single = {42};
    assert(math::sum(single) == 42);
    assert(math::mean(single) == 42);
    assert(math::min_val(single) == 42);
    assert(math::max_val(single) == 42);

    std::cout << "  stats: all tests passed" << std::endl;
}

int main() {
    std::cout << "Running local.math tests..." << std::endl;
    test_ops();
    test_stats();
    std::cout << "All tests passed." << std::endl;
    return 0;
}
