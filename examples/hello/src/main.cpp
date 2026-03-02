/// @file main.cpp
/// Entry point for the hello example

import local.hello;

import <iostream>;

int main() {
    std::cout << hello::greet("world") << std::endl;
    return 0;
}
