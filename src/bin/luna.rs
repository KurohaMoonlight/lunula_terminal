//! `luna` — Ultra-lightweight nano-style terminal text editor built for Lunula Terminal.

#[path = "../luna/mod.rs"]
mod luna;

fn main() {
    if let Err(e) = luna::run() {
        eprintln!("luna error: {}", e);
        std::process::exit(1);
    }
}
