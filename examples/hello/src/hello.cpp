/// @file hello.cpp
/// Module implementation unit for local.hello

module local.hello;

import <string>;

namespace hello {

auto greet(std::string_view name) -> std::string {
    return std::string("Hello, ") + std::string(name) + "!";
}

} // namespace hello
