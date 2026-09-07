#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_concurrent_over_cap_reservations_cannot_both_pass() {
    // The concurrency-correctness assertion: a tenant with room for exactly
    // ONE 10-byte blob (cap 10, used 0) faces TWO concurrent 10-byte writes.
    // The atomic single-statement reservation guarantees AT MOST ONE
    // succeeds; the other is refused over-cap. (Two non-atomic check-then-act
    // writes could both read used=0 and both pass → counter 20 > cap 10.)
    let (dec, _inner, store) = cas_fixture(Some(("t-race", Row { used: 0, quota: 10 })));
    let d1 = dec.clone();
    let d2 = dec.clone();
    let b1 = b"0123456789".to_vec(); // 10 bytes
    let b2 = b"abcdefghij".to_vec(); // 10 bytes, distinct hash
    let h1 = hash_for(&b1);
    let h2 = hash_for(&b2);
    let t1 =
        tokio::spawn(
            async move { d1.write(CasWriteRequest::new("t-race", h1, b1, "p", "t-race", 1)) },
        );
    let t2 =
        tokio::spawn(
            async move { d2.write(CasWriteRequest::new("t-race", h2, b2, "p", "t-race", 2)) },
        );
    let r1 = t1.await.unwrap();
    let r2 = t2.await.unwrap();
    let successes = [r1.is_ok(), r2.is_ok()].iter().filter(|b| **b).count();
    assert_eq!(
        successes, 1,
        "exactly ONE of two concurrent over-cap writes may pass (atomic reserve)"
    );
    assert_eq!(
        store.used("t-race", REGION),
        10,
        "the counter must never exceed the cap under concurrency"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_write_vs_delete_same_key_nets_to_truth() {
    // rt-nuclear C2: a `delete` of CAS key K (size L) racing a concurrent
    // overwrite-`write` of the SAME (tenant, hash) must leave `bytes_used`
    // EQUAL to the on-disk reality — L when the write wins (blob present), 0
    // when the delete wins (blob absent). Before the fix the delete's
    // release(L) and the write's independent reserve/release could interleave
    // so the two did NOT net to the true on-disk total, UNDER-counting
    // `bytes_used` by up to L (storage-quota evasion). The per-(tenant, hash)
    // decorator lock now serializes the full reserve/commit/release of WRITE
    // against the full delete/release of DELETE for one key, so the counter
    // always tracks the truth (and never underflows).
    //
    // Many iterations exercise the scheduler so an unserialized interleaving
    // would be hit with overwhelming probability.
    let body = b"race-the-same-key".to_vec();
    let n = body.len() as i64;
    let hash = hash_for(&body);
    for iter in 0..200u64 {
        // Fresh fixture per iteration with the key already present + the
        // counter already reflecting it (the on-disk truth at the start).
        let (dec, inner, store) = cas_fixture(Some(("t", Row { used: n, quota: 0 })));
        // Seed the blob on disk so a `delete` actually reclaims `n` bytes.
        dec.write(
            CasWriteRequest::new("t", hash.clone(), body.clone(), "p", "t", iter)
                .with_storage_quota_bytes(Some(0)),
        )
        .expect("seed write");
        // The seed write was a fresh insert (durable), so it accrued another
        // `n`; normalise the counter back to the single-copy on-disk truth so
        // the race starts from a consistent (counter == on-disk) state.
        store.seed("t", REGION, Row { used: n, quota: 0 });

        let dw = dec.clone();
        let dd = dec.clone();
        let hw = hash.clone();
        let hd = hash.clone();
        let bw = body.clone();
        // Concurrent overwrite-WRITE and DELETE of the SAME (tenant, hash).
        let tw = tokio::spawn(async move {
            let _ = dw.write(
                CasWriteRequest::new("t", hw, bw, "p", "t", 1).with_storage_quota_bytes(Some(0)),
            );
        });
        let td = tokio::spawn(async move {
            let _ = dd.delete(CasDeleteRequest::new("t", hd, "p", "t", 2));
        });
        tw.await.unwrap();
        td.await.unwrap();

        // The on-disk truth after the race: is the blob present?
        let present = inner
            .read(CasReadRequest::new("t", hash.clone(), "p", "t", 3))
            .is_ok();
        let counter = store.used("t", REGION);
        let expected = if present { n } else { 0 };
        assert_eq!(
            counter, expected,
            "iter {iter}: bytes_used ({counter}) must equal the on-disk truth \
                 ({expected}; present={present}) — write-vs-delete accounting must net exactly"
        );
        assert!(
            counter >= 0,
            "iter {iter}: bytes_used must never underflow below zero"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn distinct_keys_write_and_delete_stay_concurrent() {
    // The lock must serialize only the SAME (tenant, hash): a write of key A
    // and a delete of key B must both proceed and account independently
    // (different shards / no false dependency on the happy path).
    let (dec, _inner, store) = cas_fixture(None);
    let ba = b"key-a-bytes".to_vec();
    let bb = b"key-b-different".to_vec();
    let na = ba.len() as i64;
    let nb = bb.len() as i64;
    let ha = hash_for(&ba);
    let hb = hash_for(&bb);
    // Pre-seed key B so its delete reclaims real bytes; counter reflects B.
    dec.write(
        CasWriteRequest::new("t", hb.clone(), bb, "p", "t", 1).with_storage_quota_bytes(Some(0)),
    )
    .expect("seed B");
    assert_eq!(store.used("t", REGION), nb, "seed of B accrues B's bytes");

    let dw = dec.clone();
    let dd = dec.clone();
    let tw = tokio::spawn(async move {
        dw.write(CasWriteRequest::new("t", ha, ba, "p", "t", 2).with_storage_quota_bytes(Some(0)))
    });
    let td = tokio::spawn(async move { dd.delete(CasDeleteRequest::new("t", hb, "p", "t", 3)) });
    tw.await.unwrap().expect("write A");
    td.await.unwrap().expect("delete B");

    // Net effect: +na (A written) and −nb (B deleted) over the seeded nb →
    // exactly na. Distinct keys never block each other and account cleanly.
    assert_eq!(
        store.used("t", REGION),
        na,
        "distinct-key write + delete must account independently (only A's bytes remain)"
    );
}

#[allow(dead_code)]
const B126_M2_TEST_3_1_REANCHOR: () = ();
