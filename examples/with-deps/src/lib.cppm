/// @file lib.cppm
/// Module interface for local.with_deps
///
/// Demonstrates importing external Git dependencies:
///   - fmt (from github.com/satishbabariya/fmt-cmod)
///   - nlohmann.json (from github.com/satishbabariya/json-cmod)

export module local.with_deps;

import fmt;
import nlohmann.json;

import <string>;

export namespace with_deps {

/// Creates a JSON object and returns it as a formatted string.
auto make_greeting(std::string_view name, int age) -> std::string {
    nlohmann::json obj;
    obj["name"] = std::string(name);
    obj["age"] = age;
    obj["greeting"] = fmt::format("Hello, {}! You are {} years old.", name, age);
    return obj.dump(2);
}

} // namespace with_deps
