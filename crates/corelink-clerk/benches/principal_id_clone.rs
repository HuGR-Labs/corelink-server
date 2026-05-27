//! DEBT-013 OPT-07 — Clerk principal-id clone micro-bench.
//!
//! Measures the per-request clone cost of [`ClerkUserId`],
//! [`ClerkOrgId`], and [`ClerkSessionId`] post-`Arc<str>` migration
//! vs the `String::clone` baseline of the pre-OPT-07 storage shape.
//!
//! Audit reference: `specs/_audits/sealed/2026-05-15-perf-optimization-audit.md
//! §4 anti-pattern #1` — "String::clone() in tower middleware ...
//! convert hot-path String fields on shared contexts to Arc<str>".
//!
//! Acceptance: the `arc_clone` group must measure ≤ 30 ns per clone
//! AND must be observably faster than the `string_clone` baseline
//! group at the same payload length (an `Arc<str>` refcount bump is
//! constant-time in the payload length; `String::clone` allocates
//! and `memcpy`s the buffer).

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::missing_docs_in_private_items,
    reason = "bench harness; criterion macros generate items we do not own"
)]

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use corelink_clerk::principal::test_principal_helpers::{
    make_org_id, make_session_id, make_user_id,
};

// A representative Clerk identifier — Clerk user/session IDs are
// typically ~32 characters (prefix + 24-char base62 body).
const CLERK_ID_SAMPLE: &str = "user_2abcDEFghi123456789jkLMN";

fn bench_string_clone_baseline(c: &mut Criterion) {
    let s: String = CLERK_ID_SAMPLE.to_owned();
    c.bench_function("clerk/principal_id_clone/string_clone_baseline", |b| {
        b.iter(|| {
            let c = black_box(&s).clone();
            black_box(c);
        });
    });
}

fn bench_clerk_user_id_clone(c: &mut Criterion) {
    let id = make_user_id(CLERK_ID_SAMPLE);
    c.bench_function("clerk/principal_id_clone/clerk_user_id_arc_clone", |b| {
        b.iter(|| {
            let c = black_box(&id).clone();
            black_box(c);
        });
    });
}

fn bench_clerk_org_id_clone(c: &mut Criterion) {
    let id = make_org_id("org_2xyzABC012345DEFghi67jKL89mn");
    c.bench_function("clerk/principal_id_clone/clerk_org_id_arc_clone", |b| {
        b.iter(|| {
            let c = black_box(&id).clone();
            black_box(c);
        });
    });
}

fn bench_clerk_session_id_clone(c: &mut Criterion) {
    let id = make_session_id("sess_2foobar12345abc67890XYZ");
    c.bench_function(
        "clerk/principal_id_clone/clerk_session_id_arc_clone",
        |b| {
            b.iter(|| {
                let c = black_box(&id).clone();
                black_box(c);
            });
        },
    );
}

criterion_group!(
    benches,
    bench_string_clone_baseline,
    bench_clerk_user_id_clone,
    bench_clerk_org_id_clone,
    bench_clerk_session_id_clone,
);
criterion_main!(benches);
