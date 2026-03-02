/// @file lib.cppm
/// Module interface for local.app

module;

#include <iostream>
#include <string>
#include <vector>

export module local.app;

import local.core;
import local.utils;

export namespace app {

/// Run the application: create records, look up values, and print results.
inline auto run() -> int {
    std::vector<core::Record> records = {
        core::make_record("alpha", 10),
        core::make_record("beta", 20),
        core::make_record("gamma", 30),
    };

    for (const auto& r : records) {
        auto doubled = utils::doubled_value(r);
        std::cout << r.key << ": " << r.value << " -> " << doubled.value
                  << std::endl;
    }

    std::string search_key = "beta";
    if (utils::has_key(records, search_key)) {
        auto found = core::lookup(records, search_key);
        std::cout << "Found '" << found->key << "' with value " << found->value
                  << std::endl;
    }

    return 0;
}

} // namespace app
