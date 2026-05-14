#include "greeter.h"

#include <string>

namespace corelink_starter {

std::string greet(const std::string& name) {
    return "Hello, " + name + "! (cached by CoreLink)";
}

}  // namespace corelink_starter
