//! Binary entry. Logic lives in `cli`.

fn main() {
    if let Err(e) = syom::cli::main() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
