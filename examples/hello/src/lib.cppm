/// @file lib.cppm
/// Module interface unit for local.hello

module;

#include <string>
#include <string_view>

export module local.hello;

export namespace hello {

/// Returns a greeting string for the given name.
inline auto greet(std::string_view name) -> std::string {
    return std::string("Hello, ") + std::string(name) + "!";
}

} // namespace hello
