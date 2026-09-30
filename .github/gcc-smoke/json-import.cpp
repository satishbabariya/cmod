// Imports the json port as a module instead of including its header, so the
// build has to hand g++ the dependency's BMI (#114).
import nlohmann.json;
#include <cstdio>
int main() {
    auto j = nlohmann::json::parse("{\"gcc\":14}");
    std::printf("import nlohmann.json works: %s\n", j.dump().c_str());
    return 0;
}
