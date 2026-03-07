export module local.with_tests;

export namespace math {

/// Add two integers.
int add(int a, int b) {
    return a + b;
}

/// Multiply two integers.
int multiply(int a, int b) {
    return a * b;
}

/// Compute the factorial of n (n >= 0).
/// Returns 1 for n == 0.
unsigned long long factorial(unsigned int n) {
    unsigned long long result = 1;
    for (unsigned int i = 2; i <= n; ++i) {
        result *= i;
    }
    return result;
}

} // namespace math
