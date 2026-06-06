//! Rotation handling example — invokes `refresh_jwks` explicitly.
//!
//! Run with: `cargo run --example rotation -p corelink-clerk`.

#![allow(clippy::print_stdout, reason = "example demonstrates console output")]

use corelink_clerk::fakes::{InMemoryKvCache, StaticJwksFetcher};
use corelink_clerk::{ClerkAdapter, ClerkConfig};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = ClerkConfig::builder()
        .jwks_url("https://clerk.example.dev/.well-known/jwks.json")
        .issuer_allowlist(["https://clerk.example.dev"])
        .audience("corelink-api")
        .build()?;
    let fetcher = StaticJwksFetcher::empty();
    let cache = InMemoryKvCache::new();
    let adapter = ClerkAdapter::new(cfg, fetcher, cache);

    // In production, a control-plane signal (e.g. webhook from Clerk)
    // would invoke `refresh_jwks` on rotation events.
    adapter.refresh_jwks().await?;
    println!(
        "manual refresh complete; counters: {:?}",
        adapter.counters()
    );
    Ok(())
}
