//! Example: opt into FastCDC content-defined chunking.
//!
//! Run with `cargo run --example fastcdc_optin -p corelink-chunker`.

#![allow(
    clippy::print_stdout,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "example: demonstrative output + simplified error handling"
)]

use corelink_cas::chunker::{Chunker, ChunkerConfig, ChunkerKind, ChunkerStep};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 8 MiB synthetic payload — exercises FastCDC anchor search across
    // multiple chunks under the canonical `(min, avg, max) = (1, 2, 4) MiB` defaults.
    let payload: Vec<u8> = (0u32..(8 * 1024 * 1024))
        .map(|i| ((i.wrapping_mul(31).wrapping_add(7)) % 256) as u8)
        .collect();

    let mut chunker = ChunkerKind::new(ChunkerConfig::fastcdc_default())?;
    let mut cursor = 0;
    let mut count = 0;
    while cursor < payload.len() {
        match chunker.feed(&payload[cursor..]) {
            ChunkerStep::Chunk { chunk, consumed } => {
                println!(
                    "chunk {}: offset={} size={} digest={}",
                    count,
                    chunk.offset_in_blob,
                    chunk.size_bytes,
                    chunk.digest_hex()
                );
                count += 1;
                cursor += consumed;
            }
            ChunkerStep::NeedMore { consumed } => cursor += consumed,
            ChunkerStep::Error(e) => return Err(e.into()),
        }
    }
    if let Some(c) = chunker.finalize() {
        println!(
            "chunk {} (final partial): offset={} size={} digest={}",
            count,
            c.offset_in_blob,
            c.size_bytes,
            c.digest_hex()
        );
    }
    Ok(())
}
