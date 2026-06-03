//! Example: demonstrate the per-tenant concurrency budget.
//!
//! 8 simultaneous `upload_part` calls flow through the
//! semaphore; the 9th `try_acquire` (non-blocking probe) trips
//! `ConcurrencyLimitReached` and the handler maps it to HTTP 429
//! with `Retry-After`.
//!
//! Run with `cargo run -p corelink-r2-multipart --example concurrency_limit`.

#![allow(
    clippy::print_stdout,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "example: surface the budget-tripped path to stdout"
)]

use corelink_r2_multipart::{InMemoryMultipartAdapter, MultipartError};
use uuid::Uuid;

#[tokio::main]
async fn main() {
    let adapter = InMemoryMultipartAdapter::with_concurrency(8);
    let tenant = Uuid::nil();

    let mut held = Vec::new();
    for slot in 0..8 {
        let permit = adapter
            .semaphore()
            .try_acquire(tenant)
            .expect("budget not yet tripped");
        held.push(permit);
        println!("acquired permit {}", slot + 1);
    }

    match adapter.semaphore().try_acquire(tenant) {
        Err(MultipartError::ConcurrencyLimitReached { tenant_id, limit }) => {
            println!("9th try_acquire tripped: tenant={tenant_id} limit={limit}");
        }
        other => panic!("expected ConcurrencyLimitReached; got {other:?}"),
    }

    // Drop one permit → next call succeeds.
    held.pop();
    let _ = adapter
        .semaphore()
        .try_acquire(tenant)
        .expect("budget healed after drop");
    println!("budget healed: 8 permits in flight again");
}
