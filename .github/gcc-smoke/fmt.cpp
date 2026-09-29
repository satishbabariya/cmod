#include <fmt/format.h>
#include <fmt/ranges.h>
#include <vector>
int main() {
    std::vector<int> v{1, 2, 3};
    fmt::print("fmt {} works: {}\n", FMT_VERSION, v);
    return 0;
}
