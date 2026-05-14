#pragma once

#include <string>

namespace corelink_starter {

// Returns a greeting string for the given name.
// Used as the single transitive dependency in the Buck2 starter fixture
// (mirrors bazel-starter greeter.h for apples-to-apples DX parity).
std::string greet(const std::string& name);

}  // namespace corelink_starter
