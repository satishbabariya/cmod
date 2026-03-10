module;

#include <string>

export module local.formatter;

export namespace formatter {

/// Format a greeting message.
std::string greet(const std::string& name) {
    return "Hello, " + name + "!";
}

/// Format a key-value pair.
std::string key_value(const std::string& key, const std::string& value) {
    return key + " = " + value;
}

/// Format a list as "[a, b, c]".
std::string format_list(const std::string items[], std::size_t count) {
    std::string result = "[";
    for (std::size_t i = 0; i < count; ++i) {
        if (i > 0) result += ", ";
        result += items[i];
    }
    result += "]";
    return result;
}

} // namespace formatter
