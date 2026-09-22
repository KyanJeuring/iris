fn main() {
    if let Err(error) = iris_reader::run() {
        eprintln!("iris: {error:#}");
        std::process::exit(1);
    }
}
