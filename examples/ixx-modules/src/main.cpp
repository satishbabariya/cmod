#include <iostream>
#include <cassert>

import local.ixx_modules;

int main() {
    // Split and join
    auto words = text::split("hello,world,from,ixx", ',');
    assert(words.size() == 4);
    assert(words[0] == "hello");

    auto joined = text::join(words, " ");
    assert(joined == "hello world from ixx");
    std::cout << "Split+Join: " << joined << "\n";

    // Case conversion
    assert(text::to_upper("hello") == "HELLO");
    assert(text::to_lower("WORLD") == "world");
    std::cout << "Upper: " << text::to_upper("hello") << "\n";
    std::cout << "Lower: " << text::to_lower("WORLD") << "\n";

    // Trim
    assert(text::trim("  hello  ") == "hello");
    assert(text::trim("") == "");
    std::cout << "Trim: \"" << text::trim("  spaced  ") << "\"\n";

    return 0;
}
