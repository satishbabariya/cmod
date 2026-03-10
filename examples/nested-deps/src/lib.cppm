module;

#include <string>
#include <iostream>

export module local.nested_deps;

import local.formatter;

export namespace app {

/// Print a formatted greeting.
void greet(const std::string& name) {
    std::cout << formatter::greet(name) << "\n";
}

/// Print formatted key-value pairs.
void print_config(const std::string& key, const std::string& value) {
    std::cout << formatter::key_value(key, value) << "\n";
}

} // namespace app
