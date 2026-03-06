/// @file rust_dlang.cppm
/// Module partition: llvm.demangle:rust_dlang
///
/// Wraps Rust v0 and D-language name demangling functions.

module;

#include "llvm/Demangle/Demangle.h"

export module llvm.demangle:rust_dlang;

export namespace llvm_demangle::rust {

/// Demangle a Rust v0 mangled symbol name.
/// Returns a malloc'd string on success (caller must free), or nullptr.
inline char *demangle(std::string_view mangled_name) {
    return llvm::rustDemangle(mangled_name);
}

} // namespace llvm_demangle::rust

export namespace llvm_demangle::dlang {

/// Demangle a D-language mangled symbol name.
/// Returns a malloc'd string on success (caller must free), or nullptr.
inline char *demangle(std::string_view mangled_name) {
    return llvm::dlangDemangle(mangled_name);
}

} // namespace llvm_demangle::dlang
