//! Example: streaming chunker with arbitrary-sized incoming batches.
//!
//! Demonstrates that feeding the same payload in differently-sized
//! batches yields the same chunks (deterministic streaming — see
//! `tests/prop_chunker.rs::prop_split_invariance_*`).
//!
//! Run with `cargo run --example streaming -p corelink-chunker`.

#![allow(clippy::print_stdout, clippy::expect_used, clippy::panic, clippy::indexing_slicing, reason = "example: demonstrative output + simplified error handling")]

use corelink_chunker::{Chunker, ChunkerConfig, ChunkerKind, ChunkerStep, OwnedChunk};

fn drive(config: ChunkerConfig, payload: &[u8], batch_size: usize) -> Vec<OwnedChunk> {
    let mut chunker = ChunkerKind::new(config).expect("config valid");
    let mut out = Vec::new();
    let mut cursor = 0;
    while cursor < payload.len() {
        let upper = (cursor + batch_size).min(payload.len());
        match chunker.feed(&payload[cursor..upper]) {
            ChunkerStep::Chunk { chunk, consumed } => {
                out.push(chunk.to_owned());
                cursor += consumed;
            }
            ChunkerStep::NeedMore { consumed } => cursor += consumed,
            ChunkerStep::Error(e) => panic!("error: {e:?}"),
        }
    }
    if let Some(c) = chunker.finalize() {
        out.push(c.to_owned());
    }
    out
}

fn main() {
    let payload = vec![0x42u8; 6 * 1024 * 1024];
    let cfg = ChunkerConfig::default();

    let one_shot = drive(cfg.clone(), &payload, payload.len());
    let small_batches = drive(cfg.clone(), &payload, 4 * 1024); // 4 KiB batches
    let large_batches = drive(cfg, &payload, 256 * 1024); // 256 KiB batches

    assert_eq!(one_shot.len(), small_batches.len());
    assert_eq!(one_shot.len(), large_batches.len());
    for ((a, b), c) in one_shot
        .iter()
        .zip(small_batches.iter())
        .zip(large_batches.iter())
    {
        assert_eq!(a.digest, b.digest);
        assert_eq!(a.digest, c.digest);
        assert_eq!(a.offset_in_blob, b.offset_in_blob);
        assert_eq!(a.offset_in_blob, c.offset_in_blob);
    }

    println!(
        "deterministic streaming verified: {} chunks across all 3 batch profiles",
        one_shot.len()
    );
}
