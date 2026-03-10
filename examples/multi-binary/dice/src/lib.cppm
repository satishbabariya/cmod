module;

#include <string>
#include <vector>
#include <random>
#include <algorithm>
#include <chrono>
#include <stdexcept>

export module local.dice;

export namespace dice {

/// Roll a single die with the given number of sides.
int roll(int sides) {
    static std::mt19937 rng(
        static_cast<unsigned>(
            std::chrono::steady_clock::now().time_since_epoch().count()));
    std::uniform_int_distribution<int> dist(1, sides);
    return dist(rng);
}

/// Roll multiple dice and return all results.
std::vector<int> roll_many(int count, int sides) {
    std::vector<int> results;
    results.reserve(count);
    for (int i = 0; i < count; ++i) {
        results.push_back(roll(sides));
    }
    return results;
}

/// Sum of all dice in a roll.
int total(const std::vector<int>& rolls) {
    int sum = 0;
    for (int r : rolls) sum += r;
    return sum;
}

/// Highest value in a roll.
int highest(const std::vector<int>& rolls) {
    if (rolls.empty()) throw std::invalid_argument("highest: rolls must not be empty");
    return *std::max_element(rolls.begin(), rolls.end());
}

/// Lowest value in a roll.
int lowest(const std::vector<int>& rolls) {
    if (rolls.empty()) throw std::invalid_argument("lowest: rolls must not be empty");
    return *std::min_element(rolls.begin(), rolls.end());
}

/// Format a roll as "3d6: [2, 5, 1] = 8".
std::string format_roll(int count, int sides, const std::vector<int>& rolls) {
    std::string result = std::to_string(count) + "d" + std::to_string(sides) + ": [";
    for (std::size_t i = 0; i < rolls.size(); ++i) {
        if (i > 0) result += ", ";
        result += std::to_string(rolls[i]);
    }
    result += "] = " + std::to_string(total(rolls));
    return result;
}

} // namespace dice
