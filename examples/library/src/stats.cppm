/// @file stats.cppm
/// Module partition: local.math:stats
///
/// Provides statistical operations over integer spans.

export module local.math:stats;

import <span>;
import <algorithm>;
import <numeric>;

export namespace math {

/// Sum all elements in the span.
auto sum(std::span<const int> values) -> int {
    return std::accumulate(values.begin(), values.end(), 0);
}

/// Compute the arithmetic mean (integer division).
auto mean(std::span<const int> values) -> int {
    if (values.empty()) return 0;
    return sum(values) / static_cast<int>(values.size());
}

/// Return the minimum value in the span.
auto min_val(std::span<const int> values) -> int {
    if (values.empty()) return 0;
    return *std::min_element(values.begin(), values.end());
}

/// Return the maximum value in the span.
auto max_val(std::span<const int> values) -> int {
    if (values.empty()) return 0;
    return *std::max_element(values.begin(), values.end());
}

} // namespace math
