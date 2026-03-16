mod ingest;

fn startup_banner() -> &'static str {
    "Hello from Aegis AI Worker Ingest!"
}

fn main() {
    println!("{}", startup_banner());
    ingest::start_ingest();
}

#[cfg(test)]
mod tests {
    use super::startup_banner;

    #[test]
    fn startup_banner_matches_expected_message() {
        assert_eq!(startup_banner(), "Hello from Aegis AI Worker Ingest!");
    }
}
