# llvm-demangle — Scale Test Example

This example wraps LLVM's Demangle library as a C++20 module to validate that
cmod can handle real-world LLVM code at scale.

## What This Tests

- **Module graph construction:** 4 module interface files (1 primary + 3 partitions)
- **Legacy file handling:** 6 LLVM `.cpp` implementation files compiled as legacy TUs
- **Include path resolution:** LLVM-style nested include directories
- **Parallel compilation:** Multiple independent translation units
- **Mixed module/legacy linking:** Module objects + legacy objects linked together
- **Real-world code:** ~12,700 lines of production LLVM C++ code

## Structure

```
llvm-demangle/
├── cmod.toml                  # Project configuration
├── include/
│   └── llvm/
│       ├── Config/
│       │   └── llvm-config.h  # Minimal stub (normally CMake-generated)
│       └── Demangle/
│           ├── Demangle.h     # Public API header
│           └── ...            # Internal headers
└── src/
    ├── lib.cppm               # Primary module interface
    ├── itanium.cppm           # Partition: Itanium ABI demangling
    ├── microsoft.cppm         # Partition: Microsoft demangling
    ├── rust_dlang.cppm        # Partition: Rust + D-lang demangling
    ├── Demangle.cpp           # LLVM: unified demangle dispatch
    ├── ItaniumDemangle.cpp    # LLVM: Itanium demangler (~600 LOC)
    ├── MicrosoftDemangle.cpp  # LLVM: Microsoft demangler (~2500 LOC)
    ├── MicrosoftDemangleNodes.cpp
    ├── RustDemangle.cpp       # LLVM: Rust v0 demangler
    ├── DLangDemangle.cpp      # LLVM: D-lang demangler
    └── main.cpp               # Test driver exercising the module API
```

## Building

```bash
cd examples/llvm-demangle
cmod build
cmod run
```

## Source

The Demangle sources are from the [LLVM Project](https://github.com/llvm/llvm-project)
(`llvm/lib/Demangle` and `llvm/include/llvm/Demangle`), licensed under
Apache-2.0 WITH LLVM-exception.
