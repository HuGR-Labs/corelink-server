//! Example: Standalone Rekor log entry lookup by log index (WI-S12-001).
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::uninlined_format_args,
    clippy::format_in_format_args,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic
)]
//!
//! Demonstrates the `lookup` mode: given a `rekor_log_index` from a `VerifiedProvenance`,
//! generate the public Rekor entry URL and print instructions for `rekor-cli` verification.
//!
//! This is used in the paranoid mode and for audit evidence collection.
//!
//! ```sh
//! cargo run --example lookup -- --log-index 99000000
//! ```

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let mut log_index: u64 = 0;
    let mut rekor_server = "https://rekor.sigstore.dev".to_string();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--log-index" => {
                if let Some(v) = args.next() {
                    log_index = v.parse::<u64>().unwrap_or(0);
                }
            }
            "--rekor-server" => rekor_server = args.next().unwrap_or(rekor_server),
            _ => {}
        }
    }

    if log_index == 0 {
        eprintln!("Usage: lookup --log-index <N> [--rekor-server <URL>]");
        eprintln!();
        eprintln!("Example:");
        eprintln!("  cargo run --example lookup -- --log-index 99000000");
        std::process::exit(1);
    }

    let entry_url = format!("{}/api/v1/log/entries?logIndex={}", rekor_server, log_index);

    println!("Rekor log entry lookup:");
    println!("  server:     {}", rekor_server);
    println!("  log_index:  {}", log_index);
    println!("  entry URL:  {}", entry_url);
    println!();
    println!("Verify with rekor-cli:");
    println!(
        "  rekor-cli get --rekor_server {} --log-index {}",
        rekor_server, log_index
    );
    println!(
        "  rekor-cli verify --rekor_server {} --log-index {}",
        rekor_server, log_index
    );
    println!();
    println!("Fetch entry via curl:");
    println!("  curl -s '{}' | jq .", entry_url);
    println!();
    println!("Note: Rekor log entries are immutable (append-only Merkle tree).");
    println!("      Inclusion proof can be verified independently of the vendor.");
}
