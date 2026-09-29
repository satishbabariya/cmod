#include <nlohmann/json.hpp>
#include <cstdio>
int main() {
    auto j = nlohmann::json::parse("{\"msvc\":42}");
    std::printf("json works: %d\n", j["msvc"].get<int>());
    return 0;
}
