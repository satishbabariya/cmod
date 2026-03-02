/// @file core.cpp
/// Module implementation for local.core

module local.core;

import <string>;
import <vector>;
import <optional>;
import <algorithm>;

namespace core {

auto lookup(const std::vector<Record>& records,
            std::string_view key) -> std::optional<Record> {
    auto it = std::find_if(records.begin(), records.end(),
        [&](const Record& r) { return r.key == key; });
    if (it != records.end()) {
        return *it;
    }
    return std::nullopt;
}

} // namespace core
