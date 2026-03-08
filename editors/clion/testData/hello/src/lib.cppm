module;

export module com.example.hello;

import <iostream>;
import <string>;

export namespace hello {

auto greet(const std::string& name) -> std::string {
    return "Hello, " + name + "!";
}

auto say_hello() -> void {
    std::cout << greet("world") << std::endl;
}

} // namespace hello
