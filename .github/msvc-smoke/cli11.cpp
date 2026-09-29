#include <CLI/CLI.hpp>
#include <cstdio>
int main(int argc, char** argv) {
    CLI::App app{"smoke"};
    int count = 1;
    app.add_option("--count", count);
    CLI11_PARSE(app, argc, argv);
    std::printf("cli11 works: %d\n", count);
    return 0;
}
