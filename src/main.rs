fn main() {
    std::process::exit(searchfor::run(std::env::args_os().collect()));
}
