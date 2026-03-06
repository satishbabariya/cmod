/// @file microsoft.cppm
/// Module partition: llvm.demangle:microsoft
///
/// Wraps Microsoft Visual C++ name demangling functions.

module;

#include "llvm/Demangle/Demangle.h"

export module llvm.demangle:microsoft;

export namespace llvm_demangle::microsoft {

/// Flags controlling Microsoft demangling output.
enum DemangleFlags {
    None              = llvm::MSDF_None,
    DumpBackrefs      = llvm::MSDF_DumpBackrefs,
    NoAccessSpecifier = llvm::MSDF_NoAccessSpecifier,
    NoCallingConv     = llvm::MSDF_NoCallingConvention,
    NoReturnType      = llvm::MSDF_NoReturnType,
    NoMemberType      = llvm::MSDF_NoMemberType,
    NoVariableType    = llvm::MSDF_NoVariableType,
};

/// Demangle a Microsoft-mangled symbol name.
/// Returns a malloc'd string on success (caller must free), or nullptr.
inline char *demangle(std::string_view mangled_name,
                      size_t *n_read = nullptr,
                      int *status = nullptr,
                      llvm::MSDemangleFlags flags = llvm::MSDF_None) {
    return llvm::microsoftDemangle(mangled_name, n_read, status, flags);
}

} // namespace llvm_demangle::microsoft
