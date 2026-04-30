//! Explicit opt-out path: emits a `tracing::warn!` event + bumps the
//! `opt_out_total` counter when the verifier is constructed.
//!
//! Run via `cargo run --example disable_verify`.

#![allow(clippy::expect_used, clippy::print_stdout)]

use corelink_client_verify::{opt_out_total, ClientVerifier, VerifyConfig};
use tracing::Level;

fn main() {
    // Wire a stderr tracing subscriber so the warn shows up in the
    // demo run; in real SDK code the subscriber is configured by the
    // host application.
    tracing_subscriber::fmt().with_max_level(Level::WARN).init();

    let before = opt_out_total();
    let v = ClientVerifier::new(VerifyConfig::disabled());
    let after = opt_out_total();
    println!("opt-out counter: {before} -> {after}");
    println!("verifier.config.enabled() == {}", v.config().enabled());
}
