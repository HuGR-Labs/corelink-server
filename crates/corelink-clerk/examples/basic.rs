//! Basic ClerkAdapter wiring example.
//!
//! Demonstrates the canonical construction pattern: build a
//! `ClerkConfig`, wire a `StaticJwksFetcher` (production replaces
//! this with the `worker::Fetch` shim from `corelink-worker`), wrap
//! it with an `InMemoryKvCache` (production replaces with a
//! `worker::kv::Store` shim), and call `validate(raw_jwt)`.
//!
//! Run with: `cargo run --example basic -p corelink-clerk`.

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

    // A real call would feed in a Clerk-issued JWT. Here we just show
    // that the adapter rejects malformed input with no panic.
    let result = adapter.validate("not.a.real.jwt").await;
    println!("validate result on garbage input: {result:?}");
    Ok(())
}
