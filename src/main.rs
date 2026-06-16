mod cli;

fn main() {
    if let Err(err) = cli::run() {
        eprintln!("Error[{}]: {err}", err.code());
        std::process::exit(1);
    }
}
