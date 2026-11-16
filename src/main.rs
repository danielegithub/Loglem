use clap::Parser;

fn main() {
    if let Err(error) = loglem::cli::run(loglem::cli::Cli::parse()) {
        eprintln!("Loglem: {}", loglem::util::safe_text(&format!("{error:#}")));
        std::process::exit(1);
    }
}
