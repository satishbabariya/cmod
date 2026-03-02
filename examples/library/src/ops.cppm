/// @file ops.cppm
/// Module partition: local.math:ops
///
/// Provides basic constexpr arithmetic operations.

export module local.math:ops;

export namespace math {

constexpr auto add(int a, int b) -> int { return a + b; }
constexpr auto sub(int a, int b) -> int { return a - b; }
constexpr auto mul(int a, int b) -> int { return a * b; }
constexpr auto div(int a, int b) -> int { return a / b; }

} // namespace math
