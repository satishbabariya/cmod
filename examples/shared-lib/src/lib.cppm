module;

#include <cstdint>
#include <string>
#include <vector>
#include <algorithm>

export module local.shared_lib;

export namespace codec {

/// Simple run-length encoding.
std::vector<std::pair<char, int>> rle_encode(const std::string& input) {
    std::vector<std::pair<char, int>> result;
    if (input.empty()) return result;

    char current = input[0];
    int count = 1;
    for (std::size_t i = 1; i < input.size(); ++i) {
        if (input[i] == current) {
            ++count;
        } else {
            result.emplace_back(current, count);
            current = input[i];
            count = 1;
        }
    }
    result.emplace_back(current, count);
    return result;
}

/// Decode run-length encoded data.
std::string rle_decode(const std::vector<std::pair<char, int>>& encoded) {
    std::string result;
    for (const auto& [ch, count] : encoded) {
        result.append(count, ch);
    }
    return result;
}

/// XOR cipher for simple obfuscation.
std::string xor_cipher(const std::string& input, uint8_t key) {
    std::string output = input;
    for (auto& ch : output) {
        ch ^= key;
    }
    return output;
}

} // namespace codec
