// Imports the json port as a module instead of including its header, so the
// build has to hand g++ the dependency's BMI (#114).
//
// The std headers come first on purpose. g++ 14 cannot instantiate the
// port's templates in this file when the only std declarations it has come
// through the module's global module fragment (`no match for operator!=` on
// `std::vector@nlohmann.json<...>::const_iterator`). Including them here
// first works around that; it does not touch the module mapping under test.
#include <cstdio>
#include <map>
#include <memory>
#include <string>
#include <typeinfo>
#include <vector>
import nlohmann.json;
int main() {
    auto j = nlohmann::json::parse("{\"gcc\":14}");
    std::printf("import nlohmann.json works: %s\n", j.dump().c_str());
    return 0;
}
