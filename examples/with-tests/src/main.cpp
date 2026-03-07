import local.with_tests;
#include <iostream>

int main() {
    std::cout << "add(2, 3) = " << math::add(2, 3) << "\n";
    std::cout << "multiply(4, 5) = " << math::multiply(4, 5) << "\n";
    std::cout << "factorial(6) = " << math::factorial(6) << "\n";
    return 0;
}
