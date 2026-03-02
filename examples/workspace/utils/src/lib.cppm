/// @file lib.cppm
/// Module interface for local.utils
///
/// Provides utility functions that build on local.core.

module;

#include <string>
#include <vector>

export module local.utils;

import local.core;

export namespace utils {

/// Double the value of a record, returning a new record.
inline auto doubled_value(const core::Record& r) -> core::Record {
    return core::make_record(std::string(r.key), r.value * 2);
}

/// Check whether a key exists in the records.
inline auto has_key(const std::vector<core::Record>& records,
             std::string_view key) -> bool {
    return core::lookup(records, key).has_value();
}

} // namespace utils
