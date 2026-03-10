#include <iostream>
#include <cassert>

import local.shared_lib;

int main() {
    // Run-length encoding
    auto encoded = codec::rle_encode("aaabbbccdddddd");
    auto decoded = codec::rle_decode(encoded);
    assert(decoded == "aaabbbccdddddd");

    std::cout << "RLE encode \"aaabbbccdddddd\":\n";
    for (const auto& [ch, count] : encoded) {
        std::cout << "  '" << ch << "' x " << count << "\n";
    }
    std::cout << "Decoded: " << decoded << "\n";

    // XOR cipher (encrypt then decrypt)
    std::string message = "Hello, shared library!";
    auto encrypted = codec::xor_cipher(message, 42);
    auto decrypted = codec::xor_cipher(encrypted, 42);
    assert(decrypted == message);

    std::cout << "XOR round-trip passed for: " << message << "\n";

    return 0;
}
