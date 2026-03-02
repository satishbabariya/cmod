/// @file lib.cppm
/// Module interface for local.core
///
/// Provides the Record data type and basic operations.

export module local.core;

import <string>;
import <vector>;
import <optional>;

export namespace core {

/// A key-value record.
struct Record {
    std::string key;
    int value;
};

/// Create a new record.
auto make_record(std::string key, int value) -> Record {
    return Record{std::move(key), value};
}

/// Look up a record by key. Returns nullopt if not found.
auto lookup(const std::vector<Record>& records,
            std::string_view key) -> std::optional<Record>;

} // namespace core
