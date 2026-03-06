/// @file main.cpp
/// Test driver for the llvm.demangle C++20 module.
///
/// Exercises the demangling API through the module interface to validate
/// that cmod can build a real-world LLVM component wrapped as C++20 modules.

import llvm.demangle;

#include <cstdlib>
#include <iostream>
#include <string>
#include <string_view>
#include <vector>

struct TestCase {
    std::string_view mangled;
    std::string_view expected_contains;
    std::string_view description;
};

int main() {
    // Itanium (GCC/Clang) mangled names
    std::vector<TestCase> itanium_tests = {
        {"_Z3foov", "foo", "simple function"},
        {"_ZN3Bar3bazEi", "Bar", "namespaced function"},
        {"_ZNSt6vectorIiSaIiEEC1Ev", "vector", "std::vector constructor"},
        {"_Z8identityIiET_S0_", "identity", "template function"},
        {"_ZN5llvm12itanium_demangle10NodeOrBoolE", "llvm", "LLVM symbol"},
    };

    // Rust v0 mangled names
    std::vector<TestCase> rust_tests = {
        {"_RNvCs9ltgdHTiPiY_5hello4main", "hello", "Rust main function"},
    };

    int passed = 0;
    int failed = 0;
    int total = 0;

    auto run_test = [&](const TestCase &tc, const char *scheme) {
        total++;
        std::string result = llvm_demangle::demangle(tc.mangled);
        bool ok = result.find(tc.expected_contains) != std::string::npos;
        if (ok) {
            passed++;
            std::cout << "  PASS [" << scheme << "] " << tc.description
                      << ": " << tc.mangled << " -> " << result << "\n";
        } else {
            failed++;
            std::cout << "  FAIL [" << scheme << "] " << tc.description
                      << ": " << tc.mangled << " -> " << result
                      << " (expected to contain '" << tc.expected_contains
                      << "')\n";
        }
    };

    std::cout << "=== LLVM Demangle Module Test Suite ===\n\n";

    // Test the unified demangle() API
    std::cout << "--- Unified demangle() ---\n";
    for (const auto &tc : itanium_tests)
        run_test(tc, "itanium");
    for (const auto &tc : rust_tests)
        run_test(tc, "rust");

    // Test partition-specific APIs
    std::cout << "\n--- Itanium partition ---\n";
    {
        total++;
        char *result = llvm_demangle::itanium::demangle("_Z3foov");
        if (result) {
            std::string s(result);
            std::free(result);
            if (s.find("foo") != std::string::npos) {
                passed++;
                std::cout << "  PASS itanium::demangle: _Z3foov -> " << s << "\n";
            } else {
                failed++;
                std::cout << "  FAIL itanium::demangle: unexpected result: " << s << "\n";
            }
        } else {
            failed++;
            std::cout << "  FAIL itanium::demangle: returned nullptr\n";
        }
    }

    // Test ItaniumPartialDemangler
    std::cout << "\n--- Itanium PartialDemangler ---\n";
    {
        total++;
        llvm_demangle::itanium::PartialDemangler pd;
        bool err = pd.partialDemangle("_ZN3Foo3barEi");
        if (!err && pd.isFunction()) {
            passed++;
            std::cout << "  PASS PartialDemangler: _ZN3Foo3barEi is a function\n";
        } else {
            failed++;
            std::cout << "  FAIL PartialDemangler: unexpected result\n";
        }
    }

    // Summary
    std::cout << "\n=== Results: " << passed << "/" << total << " passed";
    if (failed > 0)
        std::cout << " (" << failed << " failed)";
    std::cout << " ===\n";

    return failed > 0 ? 1 : 0;
}
