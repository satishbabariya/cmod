/// @file itanium.cppm
/// Module partition: llvm.demangle:itanium
///
/// Wraps the Itanium (GCC/Clang) C++ ABI name demangling functions.

module;

#include "llvm/Demangle/Demangle.h"

export module llvm.demangle:itanium;

export namespace llvm_demangle::itanium {

/// Demangle an Itanium-mangled C++ symbol name.
/// Returns a malloc'd string on success (caller must free), or nullptr.
inline char *demangle(std::string_view mangled_name,
                      bool parse_params = true) {
    return llvm::itaniumDemangle(mangled_name, parse_params);
}

/// Partial demangler: parse into an AST and query properties.
using PartialDemangler = llvm::ItaniumPartialDemangler;

} // namespace llvm_demangle::itanium
