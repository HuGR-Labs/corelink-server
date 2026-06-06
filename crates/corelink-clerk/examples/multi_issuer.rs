//! Multi-issuer ClerkAdapter wiring (staging + prod simultaneously).
//!
//! Run with: `cargo run --example multi_issuer -p corelink-clerk`.

#![allow(clippy::print_stdout, reason = "example demonstrates console output")]

use corelink_clerk::fakes::{InMemoryKvCache, StaticJwksFetcher};
use corelink_clerk::{ClerkAdapter, ClerkConfig};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = ClerkConfig::builder()
        .jwks_url("https://clerk.prod.example.dev/.well-known/jwks.json")
        .issuer_allowlist([
            "https://clerk.staging.example.dev".to_string(),
            "https://clerk.prod.example.dev".to_string(),
        ])
        .audience("corelink-api")
        .build()?;
    let fetcher = StaticJwksFetcher::empty();
    let cache = InMemoryKvCache::new();
    let adapter = ClerkAdapter::new(cfg, fetcher, cache);
    println!(
        "adapter accepts {} canonical issuers",
        adapter.config().issuer_allowlist().len()
    );
    Ok(())
}
