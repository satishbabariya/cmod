module;

#include <utils/config.h>
#include <string>

export module local.include_dirs;

export namespace app {

/// Application settings using the header-based Config utility.
struct Settings {
    std::string app_name;
    std::string version;
    int port;
    bool debug;
};

/// Load settings from a Config object.
Settings load_settings(const utils::Config& config) {
    return Settings{
        .app_name = config.get("app.name", "my-app"),
        .version = config.get("app.version", "0.0.0"),
        .port = std::stoi(config.get("app.port", "8080")),
        .debug = config.get("app.debug", "false") == "true",
    };
}

/// Create a default configuration.
utils::Config default_config() {
    utils::Config config;
    config.set("app.name", "include-dirs-example");
    config.set("app.version", "0.1.0");
    config.set("app.port", "3000");
    config.set("app.debug", "true");
    return config;
}

} // namespace app
