/// @file lib.cppm
/// Module interface unit for local.hello

export module local.hello;

import <string>;

export namespace hello {

/// Returns a greeting string for the given name.
auto greet(std::string_view name) -> std::string;

} // namespace hello
