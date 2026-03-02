/// @file main.cpp
/// Entry point for the hello example

#include <iostream>

import local.hello;

int main() {
    std::cout << hello::greet("world") << std::endl;
    return 0;
}
