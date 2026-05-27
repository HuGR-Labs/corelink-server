//! Example: fixed-size chunker with the canonical defaults (`Fixed2MiB`).
//!
//! Run with `cargo run --example fixed_default -p corelink-chunker`.

#![allow(clippy::print_stdout, clippy::expect_used, clippy::panic, clippy::indexing_slicing, reason = "example: demonstrative output + simplified error handling")]

use corelink_cas::chunker::{Chunker, ChunkerConfig, ChunkerKind, ChunkerStep};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 5 MiB synthetic payload — exercises the partial-final-chunk path.
    let payload = vec![0xA7u8; 5 * 1024 * 1024];

    let mut chunker = ChunkerKind::new(ChunkerConfig::default())?;
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
