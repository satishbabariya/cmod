module;

#include <iostream>

export module local.roller;

import local.dice;

export namespace roller {

void run() {
    std::cout << "=== Dice Roller ===\n\n";

    auto d6 = dice::roll_many(3, 6);
    std::cout << dice::format_roll(3, 6, d6) << "\n";

    auto d20 = dice::roll_many(1, 20);
    std::cout << dice::format_roll(1, 20, d20) << "\n";

    auto d12 = dice::roll_many(4, 12);
    std::cout << dice::format_roll(4, 12, d12) << "\n";

    std::cout << "\nHighest of 4d12: " << dice::highest(d12) << "\n";
    std::cout << "Lowest of 4d12:  " << dice::lowest(d12) << "\n";
}

} // namespace roller
