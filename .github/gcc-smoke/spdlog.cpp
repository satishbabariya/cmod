#include <spdlog/spdlog.h>
int main() {
    spdlog::info("spdlog {}.{}.{} works", SPDLOG_VER_MAJOR, SPDLOG_VER_MINOR,
                 SPDLOG_VER_PATCH);
    spdlog::warn("fmt passthrough: {:>5}", 42);
    return 0;
}
