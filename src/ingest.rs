pub fn startup_message() -> &'static str {
    "Aegis AI Worker Ingest started."
}

pub fn start_ingest() {
    println!("{}", startup_message());
}

#[cfg(test)]
mod tests {
    use super::startup_message;

    #[test]
    fn startup_message_matches_expected_banner() {
        assert_eq!(startup_message(), "Aegis AI Worker Ingest started.");
    }
}
