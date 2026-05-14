#include <cstdlib>
#include <iostream>
#include <string>

#include "greeter.h"

int main(int argc, char* argv[]) {
    const std::string name = (argc > 1) ? argv[1] : "World";
    std::cout << corelink_starter::greet(name) << "\n";
    return EXIT_SUCCESS;
}
