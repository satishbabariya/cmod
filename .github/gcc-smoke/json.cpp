#include <nlohmann/json.hpp>
#include <cstdio>
int main() {
    auto j = nlohmann::json::parse("{\"gcc\":14}");
    std::printf("json %d.%d.%d works: %s\n", NLOHMANN_JSON_VERSION_MAJOR,
                NLOHMANN_JSON_VERSION_MINOR, NLOHMANN_JSON_VERSION_PATCH,
                j.dump().c_str());
    return 0;
}
