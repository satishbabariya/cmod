module;

#include <string>
#include <sstream>
#include <vector>
#include <algorithm>

export module local.ixx_modules;

export namespace text {

/// Split a string by a delimiter character.
std::vector<std::string> split(const std::string& input, char delimiter) {
    std::vector<std::string> tokens;
    std::istringstream stream(input);
    std::string token;
    while (std::getline(stream, token, delimiter)) {
        if (!token.empty()) {
            tokens.push_back(token);
        }
    }
    return tokens;
}

/// Join a vector of strings with a separator.
std::string join(const std::vector<std::string>& parts, const std::string& separator) {
    std::string result;
    for (std::size_t i = 0; i < parts.size(); ++i) {
        if (i > 0) result += separator;
        result += parts[i];
    }
    return result;
}

/// Convert a string to uppercase.
std::string to_upper(std::string input) {
    std::transform(input.begin(), input.end(), input.begin(),
        [](unsigned char c) { return std::toupper(c); });
    return input;
}

/// Convert a string to lowercase.
std::string to_lower(std::string input) {
    std::transform(input.begin(), input.end(), input.begin(),
        [](unsigned char c) { return std::tolower(c); });
    return input;
}

/// Trim whitespace from both ends of a string.
std::string trim(const std::string& input) {
    auto start = input.find_first_not_of(" \t\n\r");
    if (start == std::string::npos) return "";
    auto end = input.find_last_not_of(" \t\n\r");
    return input.substr(start, end - start + 1);
}

} // namespace text
