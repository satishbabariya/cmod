// main() comes from the port's catch_main.cpp.
#include <catch2/catch_test_macros.hpp>
#include <catch2/catch_version_macros.hpp>
#include <cstdio>
TEST_CASE("gcc smoke") {
    std::printf("Catch2 %d.%d.%d works\n", CATCH_VERSION_MAJOR,
                CATCH_VERSION_MINOR, CATCH_VERSION_PATCH);
    REQUIRE(1 + 1 == 2);
}
