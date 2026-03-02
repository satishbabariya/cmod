/// @file app.cpp
/// Module implementation for local.app

module local.app;

import local.core;
import local.utils;
import fmt;

import <string>;
import <vector>;
import <iostream>;

namespace app {

auto run() -> int {
    std::vector<core::Record> records = {
        core::make_record("alpha", 10),
        core::make_record("beta", 20),
        core::make_record("gamma", 30),
    };

    for (const auto& r : records) {
        auto doubled = utils::doubled_value(r);
        std::cout << fmt::format("{}: {} -> {}", r.key, r.value, doubled.value)
                  << std::endl;
    }

    std::string search_key = "beta";
    if (utils::has_key(records, search_key)) {
        auto found = core::lookup(records, search_key);
        std::cout << fmt::format("Found '{}' with value {}", found->key, found->value)
                  << std::endl;
    }

    return 0;
}

} // namespace app
