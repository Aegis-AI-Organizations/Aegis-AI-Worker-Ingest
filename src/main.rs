mod ingest;

fn main() {
    println!("Hello from Aegis AI Worker Ingest!");
    ingest::start_ingest();
}
