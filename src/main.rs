mod ingest;

fn startup_banner() -> &'static str {
    "Hello from Aegis AI Worker Ingest!"
}

fn run() {
    println!("{}", startup_banner());
    ingest::start_ingest();
}

fn main() {
    run();
}

#[cfg(test)]
mod tests {
    use super::{run, startup_banner};

    #[test]
    fn startup_banner_matches_expected_message() {
        assert_eq!(startup_banner(), "Hello from Aegis AI Worker Ingest!");
    }

    #[test]
    fn run_executes_without_panic() {
        run();
    }
}
