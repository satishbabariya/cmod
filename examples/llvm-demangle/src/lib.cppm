/// @file lib.cppm
/// Primary module interface for llvm.demangle
///
/// Wraps the LLVM Demangle library as a C++20 module. The original LLVM
/// headers are included via the global module fragment, then re-exported
/// through the module interface.

module;

// Global module fragment: include traditional headers here.
// Macros and preprocessor directives work normally in this section.
#include "llvm/Demangle/Demangle.h"

export module llvm.demangle;

export import :itanium;
export import :microsoft;
export import :rust_dlang;

/// Top-level demangling: attempt all known schemes via heuristics.
export namespace llvm_demangle {

/// Attempt to demangle a symbol using all known demangling schemes.
/// Returns the demangled string, or a copy of the input if demangling fails.
inline std::string demangle(std::string_view mangled_name) {
    return llvm::demangle(mangled_name);
}

/// Attempt non-Microsoft demangling schemes (Itanium, Rust, D).
/// Returns true on success, writing the result to `out`.
inline bool non_microsoft_demangle(std::string_view mangled_name,
                                   std::string &out,
                                   bool can_have_leading_dot = true,
                                   bool parse_params = true) {
    return llvm::nonMicrosoftDemangle(mangled_name, out,
                                      can_have_leading_dot, parse_params);
}

// Re-export status codes
using llvm::demangle_success;
using llvm::demangle_memory_alloc_failure;
using llvm::demangle_invalid_mangled_name;
using llvm::demangle_invalid_args;
using llvm::demangle_unknown_error;

} // namespace llvm_demangle
