/// @file main.cpp
/// Entry point for the with-deps example

import local.with_deps;

import <iostream>;

int main() {
    std::cout << with_deps::make_greeting("Alice", 30) << std::endl;
    return 0;
}
