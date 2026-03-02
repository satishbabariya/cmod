/// @file lib.cppm
/// Module interface for local.core
///
/// Provides the Record data type and basic operations.

module;

#include <algorithm>
#include <optional>
#include <string>
#include <vector>

export module local.core;

export namespace core {

/// A key-value record.
struct Record {
    std::string key;
    int value;
};

/// Create a new record.
inline auto make_record(std::string key, int value) -> Record {
    return Record{std::move(key), value};
}

/// Look up a record by key. Returns nullopt if not found.
inline auto lookup(const std::vector<Record>& records,
                   std::string_view key) -> std::optional<Record> {
    auto it = std::find_if(records.begin(), records.end(),
        [&](const Record& r) { return r.key == key; });
    if (it != records.end()) {
        return *it;
    }
    return std::nullopt;
}

} // namespace core
