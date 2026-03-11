module;

#include <iostream>
#include <map>
#include <iomanip>

export module local.stats;

import local.dice;

export namespace stats {

void run() {
    std::cout << "=== Dice Statistics (10000 rolls of 2d6) ===\n\n";

    std::map<int, int> histogram;
    const int trials = 10000;

    for (int i = 0; i < trials; ++i) {
        auto rolls = dice::roll_many(2, 6);
        histogram[dice::total(rolls)]++;
    }

    std::cout << "Sum  Count  Distribution\n";
    std::cout << "---  -----  ------------\n";
    for (const auto& [sum, count] : histogram) {
        int bar_len = count * 50 / trials;
        std::cout << std::setw(3) << sum << "  "
                  << std::setw(5) << count << "  ";
        for (int i = 0; i < bar_len; ++i) std::cout << '#';
        std::cout << "\n";
    }
}

} // namespace stats
