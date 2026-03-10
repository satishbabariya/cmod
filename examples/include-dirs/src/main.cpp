import local.include_dirs;

#include <iostream>
#include <cassert>

int main() {
    auto config = app::default_config();
    auto settings = app::load_settings(config);

    assert(settings.app_name == "include-dirs-example");
    assert(settings.port == 3000);
    assert(settings.debug == true);

    std::cout << "App:     " << settings.app_name << "\n";
    std::cout << "Version: " << settings.version << "\n";
    std::cout << "Port:    " << settings.port << "\n";
    std::cout << "Debug:   " << (settings.debug ? "true" : "false") << "\n";

    return 0;
}
