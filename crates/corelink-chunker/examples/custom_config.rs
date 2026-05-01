//! Example: build a custom chunker config (smaller chunks for testing
//! pipelines / specialised workloads). Validates the config + walks the
//! resulting chunker.
//!
//! Run with `cargo run --example custom_config -p corelink-chunker`.

#![allow(clippy::print_stdout, clippy::expect_used, clippy::panic, clippy::indexing_slicing, reason = "example: demonstrative output + simplified error handling")]

use corelink_chunker::{
    Chunker, ChunkerAlgorithm, ChunkerConfig, ChunkerKind, ChunkerStep,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = ChunkerConfig::default()
        .with_algorithm(ChunkerAlgorithm::FastCDC2MiB)
        .with_fastcdc_bounds(64 * 1024, 256 * 1024, 1024 * 1024); // 64 KiB / 256 KiB / 1 MiB

    cfg.validate()?;
    let mut chunker = ChunkerKind::new(cfg)?;

    let payload: Vec<u8> = (0u32..(2 * 1024 * 1024))
        .map(|i| ((i.wrapping_mul(0x9E37_79B9) >> 11) & 0xFF) as u8)
        .collect();

    let mut cursor = 0;
    let mut count = 0;
    while cursor < payload.len() {
        match chunker.feed(&payload[cursor..]) {
            ChunkerStep::Chunk { chunk, consumed } => {
                println!("chunk {} size={}", count, chunk.size_bytes);
                count += 1;
                cursor += consumed;
            }
            ChunkerStep::NeedMore { consumed } => cursor += consumed,
            ChunkerStep::Error(e) => return Err(e.into()),
        }
    }
    if let Some(c) = chunker.finalize() {
        println!("final chunk {} size={}", count, c.size_bytes);
    }
    Ok(())
}
