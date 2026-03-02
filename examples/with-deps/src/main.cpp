/// @file main.cpp
/// Entry point for the with-deps example

#include <iostream>

import local.with_deps;

int main() {
    std::cout << with_deps::make_greeting("Alice", 30) << std::endl;
    return 0;
}
