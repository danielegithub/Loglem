fn main() {
    loglem::memory::init();
    let cli = match loglem::arguments::parse() {
        Ok(Some(cli)) => cli,
        Ok(None) => return,
        Err(e) => {
            eprintln!("Loglem: {}", loglem::util::safe_text(&e.to_string()));
            std::process::exit(2);
        }
    };
    if let Err(error) = loglem::cli::run(cli) {
        eprintln!("Loglem: {}", loglem::util::safe_text(&format!("{error:#}")));
        std::process::exit(1);
    }
}
