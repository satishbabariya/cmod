import local.nested_deps;

int main() {
    app::greet("World");
    app::print_config("version", "0.1.0");
    app::print_config("build", "debug");

    return 0;
}
