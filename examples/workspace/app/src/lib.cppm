/// @file lib.cppm
/// Module interface for local.app

export module local.app;

import local.core;
import local.utils;
import fmt;

import <string>;
import <vector>;

export namespace app {

/// Run the application: create records, look up values, and print results.
auto run() -> int;

} // namespace app
