use super::*;
use corelink_audit_chain::link_chain_hash_from_canonical;

fn rows(payloads: &[Value]) -> Vec<(String, Value)> {
    payloads
        .iter()
        .enumerate()
        .map(|(i, p)| (format!("row-{i:04}"), p.clone()))
        .collect()
}

// A forwarded UNSET secret arrives as Some(""), NOT None (the DO forward-list
// sends `this.env.X ?? ""`). The empty dedicated slot MUST fall through to the
// reused erasure-attestation seed, or CF-6 head-signing is silently OFF in
// prod (the observed `signed=0` regression).
#[test]
fn resolve_seed_empty_dedicated_falls_back_to_reused() {
    let reused = "a".repeat(64);
    let got = resolve_seed(Some(String::new()), Some(reused.clone()))
        .expect("empty dedicated must fall back to the reused seed");
    assert_eq!(got.as_slice(), &[0xaau8; 32]);
    // Whitespace-only is also empty.
    assert!(resolve_seed(Some("   ".into()), Some(reused)).is_some());
}

#[test]
fn resolve_seed_prefers_dedicated_and_rejects_malformed() {
    let dedicated = "b".repeat(64);
    let reused = "c".repeat(64);
    // Dedicated wins when present + non-empty.
    assert_eq!(
        resolve_seed(Some(dedicated), Some(reused))
            .unwrap()
            .as_slice(),
        &[0xbbu8; 32]
    );
    // Both empty ⇒ None (advance UNSIGNED).
    assert!(resolve_seed(Some(String::new()), Some(String::new())).is_none());
    assert!(resolve_seed(None, None).is_none());
    // Wrong length / non-hex ⇒ None.
    assert!(resolve_seed(Some("dead".into()), None).is_none());
    assert!(resolve_seed(Some("zz".repeat(32)), None).is_none());
}

#[test]
fn resolve_key_id_empty_dedicated_falls_back_then_defaults() {
    // Empty dedicated ⇒ use the reused id.
    assert_eq!(resolve_key_id(Some(String::new()), Some("7".into())), 7);
    // Dedicated wins when present.
    assert_eq!(resolve_key_id(Some("3".into()), Some("7".into())), 3);
    // Neither / unparseable ⇒ launch default 1.
    assert_eq!(resolve_key_id(Some(String::new()), Some(String::new())), 1);
    assert_eq!(resolve_key_id(None, None), 1);
    assert_eq!(resolve_key_id(Some("nope".into()), None), 1);
}

#[test]
fn limit_prefix_seal_then_resume_equals_one_shot_seal() {
    // The batched-drain fix seals only an ordered PREFIX per call (LIMIT) and
    // the next call resumes from the advanced head. This MUST yield the exact
    // same chain as sealing the whole partition in one shot — a prefix of a
    // deterministic order is still a deterministic order.
    let all = rows(&[
        json!({"a": 1}),
        json!({"b": 2}),
        json!({"c": 3}),
        json!({"d": 4}),
        json!({"e": 5}),
    ]);
    let (one_shot, one_head, one_seq) = seal_rows(ChainHash::genesis(), 0, &all).unwrap();

    // Call 1: seal the first 2 rows (LIMIT 2), from genesis.
    let (p1, head1, seq1) = seal_rows(ChainHash::genesis(), 0, &all[0..2]).unwrap();
    // Call 2: resume from call 1's advanced head + seq, seal the remaining 3.
    let (p2, head2, seq2) = seal_rows(head1, seq1, &all[2..5]).unwrap();

    // Same final head + next-sequence.
    assert_eq!(head1.to_hex(), one_shot[1].chain_hash_hex);
    assert_eq!(head2.to_hex(), one_head.to_hex());
    assert_eq!(seq2, one_seq);
    // Every sealed row is byte-identical to the one-shot chain.
    let chained: Vec<_> = p1.into_iter().chain(p2).collect();
    assert_eq!(chained.len(), one_shot.len());
    for (got, want) in chained.iter().zip(one_shot.iter()) {
        assert_eq!(got.sequence_number, want.sequence_number);
        assert_eq!(got.prev_hash_hex, want.prev_hash_hex);
        assert_eq!(got.chain_hash_hex, want.chain_hash_hex);
        assert_eq!(got.canonical_jcs, want.canonical_jcs);
    }
}

#[test]
fn fresh_partition_seals_from_genesis_monotonic_and_linked() {
    let input = rows(&[json!({"a": 1}), json!({"b": 2}), json!({"c": 3})]);
    let (sealed, head, next_seq) = seal_rows(ChainHash::genesis(), 0, &input).unwrap();

    assert_eq!(sealed.len(), 3);
    // Monotonic sequence from genesis.
    assert_eq!(sealed[0].sequence_number, 0);
    assert_eq!(sealed[1].sequence_number, 1);
    assert_eq!(sealed[2].sequence_number, 2);
    assert_eq!(next_seq, 3);
    // Genesis prev_hash is 64 zeros.
    assert_eq!(sealed[0].prev_hash_hex, "0".repeat(64));
    // Each row's prev_hash == the previous row's chain_hash (the link).
    assert_eq!(sealed[1].prev_hash_hex, sealed[0].chain_hash_hex);
    assert_eq!(sealed[2].prev_hash_hex, sealed[1].chain_hash_hex);
    // The returned head is the last link.
    assert_eq!(head.to_hex(), sealed[2].chain_hash_hex);
    // Distinct links.
    assert_ne!(sealed[0].chain_hash_hex, sealed[1].chain_hash_hex);
    assert_ne!(sealed[1].chain_hash_hex, sealed[2].chain_hash_hex);
}

#[test]
fn canonical_jcs_stored_is_exactly_what_was_hashed() {
    let payload = json!({"z": 1, "a": 2, "m": {"y": 9, "b": 8}});
    let input = rows(std::slice::from_ref(&payload));
    let (sealed, _, _) = seal_rows(ChainHash::genesis(), 0, &input).unwrap();
    let s = &sealed[0];

    // The stored canonical_jcs is byte-for-byte the RFC-8785 JCS of the payload.
    let expected_jcs = serde_jcs::to_vec(&payload).unwrap();
    assert_eq!(s.canonical_jcs.as_bytes(), expected_jcs.as_slice());

    // And the chain_hash is exactly BLAKE3(prev || canonical_jcs) over THOSE bytes.
    let recomputed =
        link_chain_hash_from_canonical(&ChainHash::genesis(), s.canonical_jcs.as_bytes());
    assert_eq!(recomputed.to_hex(), s.chain_hash_hex);
}

#[test]
fn rerun_is_idempotent_and_deterministic_no_fork() {
    // Two concurrent drains resuming from the SAME state over the SAME rows
    // compute byte-identical seals — so overlapping row writes are identical,
    // never a fork. (The DB-level idempotency is the `emitted_at IS NULL`
    // guard on the UPDATE; the determinism proven here is its foundation.)
    let input = rows(&[json!({"e": "x"}), json!({"e": "y"})]);
    let a = seal_rows(ChainHash::genesis(), 0, &input).unwrap();
    let b = seal_rows(ChainHash::genesis(), 0, &input).unwrap();
    assert_eq!(a.0, b.0);
    assert_eq!(a.1.to_hex(), b.1.to_hex());
    assert_eq!(a.2, b.2);
}

#[test]
fn split_drain_does_not_fork_the_sequence() {
    // Drain A sees [r0]; a new row r1 arrives; Drain B (resuming from the
    // sealed tail after A sealed r0) sees [r1]. The split must produce the
    // SAME chain as a single drain over [r0, r1].
    let r0 = json!({"n": 0});
    let r1 = json!({"n": 1});

    // Single drain over both.
    let combined = seal_rows(ChainHash::genesis(), 0, &rows(&[r0.clone(), r1.clone()])).unwrap();

    // Drain A: just r0 from genesis.
    let a = seal_rows(ChainHash::genesis(), 0, &rows(std::slice::from_ref(&r0))).unwrap();
    assert_eq!(a.0[0], combined.0[0]);

    // Drain B resumes from the sealed tail A produced (head=a.1, seq=a.2),
    // then seals r1.
    let b = seal_rows(a.1, a.2, &rows(std::slice::from_ref(&r1))).unwrap();
    // r1's sealed link is identical to the single-drain result.
    assert_eq!(b.0[0].sequence_number, combined.0[1].sequence_number);
    assert_eq!(b.0[0].prev_hash_hex, combined.0[1].prev_hash_hex);
    assert_eq!(b.0[0].chain_hash_hex, combined.0[1].chain_hash_hex);
    assert_eq!(b.1.to_hex(), combined.1.to_hex());
}

#[test]
fn resolve_resume_genesis_when_no_state() {
    assert_eq!(resolve_resume(None, None), None);
}

#[test]
fn resolve_resume_uses_checkpoint_when_no_sealed_tail() {
    let h = ChainHash([0x11; 32]);
    assert_eq!(resolve_resume(Some((h, 5)), None), Some((h, 5)));
}

#[test]
fn resolve_resume_prefers_sealed_tail_when_ahead_of_checkpoint() {
    // Crash/drift recovery: the checkpoint lagged behind the durable sealed
    // rows (a prior drain crashed after sealing but before advancing the head).
    // The sealed tail is authoritative.
    let cp = ChainHash([0x11; 32]);
    let tail = ChainHash([0x22; 32]);
    // Checkpoint says next_sequence = 3; sealed tail row has seq = 4 → tail
    // next = 5 > 3 → resume from the tail.
    assert_eq!(
        resolve_resume(Some((cp, 3)), Some((tail, 4))),
        Some((tail, 5))
    );
}

#[test]
fn resolve_resume_keeps_checkpoint_when_tail_not_ahead() {
    let cp = ChainHash([0x11; 32]);
    let tail = ChainHash([0x22; 32]);
    // Checkpoint next_sequence = 5; sealed tail seq = 4 → tail next = 5, NOT
    // strictly greater → keep the (faster) checkpoint.
    assert_eq!(
        resolve_resume(Some((cp, 5)), Some((tail, 4))),
        Some((cp, 5))
    );
}

#[test]
fn builder_resume_seeds_start_state() {
    // Genesis path: new().
    let g = HashChainBuilder::new();
    assert_eq!(g.head().as_bytes(), &[0u8; 32]);
    assert_eq!(g.next_sequence(), 0);
    // Resume path: resume(head, seq).
    let h = ChainHash([0xAB; 32]);
    let r = HashChainBuilder::resume(h, 7);
    assert_eq!(r.head(), &h);
    assert_eq!(r.next_sequence(), 7);
}

#[test]
fn empty_partition_seals_nothing_and_leaves_head_unchanged() {
    // No pending rows → no seals, head + sequence pass through unchanged.
    let h = ChainHash([0x42; 32]);
    let (sealed, head, next_seq) = seal_rows(h, 9, &[]).unwrap();
    assert!(sealed.is_empty());
    assert_eq!(head, h);
    assert_eq!(next_seq, 9);
}

#[test]
fn chain_hash_from_hex_round_trips_and_rejects_bad() {
    let h = ChainHash([0x5A; 32]);
    assert_eq!(chain_hash_from_hex(&h.to_hex()), Some(h));
    assert_eq!(chain_hash_from_hex("zz"), None);
    assert_eq!(chain_hash_from_hex(&"ab".repeat(31)), None); // 62 hex chars
}

#[test]
fn sealed_tail_metadata_accepts_only_complete_shapes() {
    for row in [
        json!({"algorithm_id": null, "epoch_id": null, "link_key_id": null}),
        json!({"algorithm_id": 0, "epoch_id": 0, "link_key_id": null}),
        json!({"algorithm_id": 1, "epoch_id": 1, "link_key_id": 7}),
    ] {
        assert!(parse_sealed_epoch_metadata(&row).is_ok(), "valid: {row}");
    }
    for row in [
        json!({"algorithm_id": 0, "epoch_id": null, "link_key_id": null}),
        json!({"algorithm_id": null, "epoch_id": 0, "link_key_id": null}),
        json!({"algorithm_id": 1, "epoch_id": 1, "link_key_id": null}),
        json!({"algorithm_id": 1, "epoch_id": 0, "link_key_id": 7}),
        json!({"algorithm_id": 0, "epoch_id": 0, "link_key_id": 7}),
        json!({"algorithm_id": 1, "epoch_id": -1, "link_key_id": 7}),
    ] {
        assert!(
            parse_sealed_epoch_metadata(&row).is_err(),
            "partial/downgrade must fail closed: {row}"
        );
    }
}

#[test]
fn checkpoint_metadata_distinguishes_legacy_null_from_malformed_values() {
    let legacy = json!({"epoch_id": null});
    assert_eq!(checkpoint_nullable_u64(&legacy, "epoch_id"), Ok(None));

    for value in [json!("x"), json!(-1), json!(1.5)] {
        let row = json!({"epoch_id": value});
        assert!(
            checkpoint_nullable_u64(&row, "epoch_id").is_err(),
            "non-integer/negative checkpoint metadata must fail closed: {row}"
        );
    }
    assert!(checkpoint_nullable_u64(&json!({}), "epoch_id").is_err());
    assert!(checkpoint_nullable_text(&json!({"head_signature": 7}), "head_signature").is_err());
}

// ---- CF-6: keyed (Ed25519-signed) chain head ----

const SEED_A: [u8; 32] = [0x11; 32];
const SEED_B: [u8; 32] = [0x22; 32];
const TENANT: &str = "00000000-0000-7000-8000-00000000aaaa";
const REGION: &str = "weur";
const HEAD_HEX: &str = "ab"; // expanded to 64 hex below via repeat
const KID: u64 = 1;

fn head_hex() -> String {
    HEAD_HEX.repeat(32) // 64 lowercase hex chars
}

/// Build a checkpoint whose stored signature is a GENUINE signature over its
/// own (tenant, region, head_hex, next_sequence) tuple under `seed`/`kid`.
fn signed_checkpoint(
    seed: &[u8; 32],
    kid: u64,
    head_hex: &str,
    next_sequence: u64,
) -> HeadCheckpoint {
    let sig = sign_head(seed, kid, TENANT, REGION, head_hex, next_sequence).unwrap();
    HeadCheckpoint {
        head: chain_hash_from_hex(head_hex).unwrap(),
        head_hex: head_hex.to_string(),
        next_sequence,
        head_signature: Some(sig),
        signing_key_id: Some(kid),
        epoch_id: None,
        head_message_version: None,
        epoch_ledger_sequence: None,
        epoch_ledger_hash: None,
        head_witness_sequence: None,
        head_witness_hash: None,
    }
}

#[test]
fn canonical_head_bytes_is_deterministic_and_jcs_key_ordered() {
    let a = canonical_head_bytes(TENANT, REGION, &head_hex(), 7).unwrap();
    let b = canonical_head_bytes(TENANT, REGION, &head_hex(), 7).unwrap();
    assert_eq!(a, b, "canonical head bytes must be deterministic");
    let s = String::from_utf8(a).unwrap();
    // RFC-8785 JCS sorts keys lexicographically: head_hash < next_sequence <
    // region < tenant_id.
    let pos = |k: &str| s.find(k).unwrap();
    assert!(pos("head_hash") < pos("next_sequence"));
    assert!(pos("next_sequence") < pos("region"));
    assert!(pos("region") < pos("tenant_id"));
}

#[test]
fn versioned_head_binds_epoch_and_ledger_facts() {
    let head = head_hex();
    let ledger = "cd".repeat(32);
    let bytes = canonical_head_v2_bytes(TENANT, REGION, &head, 7, 1, 2, &ledger, KID).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains("\"head_message_version\":2"));
    let sig = sign_head_v2(&SEED_A, KID, TENANT, REGION, &head, 7, 1, 2, &ledger).unwrap();
    assert!(verify_head_v2(
        &SEED_A, KID, TENANT, REGION, &head, 7, 1, 2, &ledger, &sig
    ));
    assert!(!verify_head_v2(
        &SEED_A, KID, TENANT, REGION, &head, 7, 2, 2, &ledger, &sig
    ));
    assert!(!verify_head_v2(
        &SEED_A, KID, TENANT, REGION, &head, 7, 1, 3, &ledger, &sig
    ));
}

#[test]
fn legacy_drain_refuses_partial_or_v2_epoch_metadata() {
    let mut legacy = signed_checkpoint(&SEED_A, KID, &head_hex(), 9);
    assert!(reject_unwired_epoch_checkpoint(Some(&legacy)).is_ok());
    legacy.epoch_id = Some(0);
    assert!(reject_unwired_epoch_checkpoint(Some(&legacy)).is_err());
    legacy.head_message_version = Some(2);
    legacy.epoch_ledger_sequence = Some(0);
    legacy.epoch_ledger_hash = Some("00".repeat(32));
    legacy.head_witness_sequence = Some(0);
    legacy.head_witness_hash = Some("00".repeat(32));
    let err = reject_unwired_epoch_checkpoint(Some(&legacy)).unwrap_err();
    assert!(err.contains("refusing legacy drain downgrade"));
}

/// A genuine advance signs a head whose signature verifies (round-trip).
#[test]
fn genuine_head_signature_verifies() {
    let sig = sign_head(&SEED_A, KID, TENANT, REGION, &head_hex(), 42).unwrap();
    assert!(
        verify_head(&SEED_A, KID, TENANT, REGION, &head_hex(), 42, &sig),
        "a genuine head signature must verify"
    );
}

/// A tampered head_hash with the ORIGINAL signature must NOT verify (the core
/// CF-6 tamper-detection: a D1 writer who rewrites the head is caught).
#[test]
fn tampered_head_hash_fails_verification() {
    let sig = sign_head(&SEED_A, KID, TENANT, REGION, &head_hex(), 42).unwrap();
    let forged = "cd".repeat(32);
    assert_ne!(forged, head_hex());
    assert!(
        !verify_head(&SEED_A, KID, TENANT, REGION, &forged, 42, &sig),
        "a rewritten head_hash must fail verification"
    );
}

/// Tampering with any other bound field (sequence / region / tenant) also
/// fails — the whole tuple is signed.
#[test]
fn tampered_tuple_fields_fail_verification() {
    let sig = sign_head(&SEED_A, KID, TENANT, REGION, &head_hex(), 42).unwrap();
    // Rewound sequence.
    assert!(!verify_head(
        &SEED_A,
        KID,
        TENANT,
        REGION,
        &head_hex(),
        41,
        &sig
    ));
    // Different region.
    assert!(!verify_head(
        &SEED_A,
        KID,
        TENANT,
        "enam",
        &head_hex(),
        42,
        &sig
    ));
    // Different tenant.
    assert!(!verify_head(
        &SEED_A,
        KID,
        "00000000-0000-7000-8000-00000000bbbb",
        REGION,
        &head_hex(),
        42,
        &sig
    ));
}

/// A different seed (forged signer) must NOT verify — the head cannot be
/// re-signed without the write-only seed.
#[test]
fn wrong_seed_fails_verification() {
    let sig = sign_head(&SEED_A, KID, TENANT, REGION, &head_hex(), 42).unwrap();
    assert!(!verify_head(
        &SEED_B,
        KID,
        TENANT,
        REGION,
        &head_hex(),
        42,
        &sig
    ));
}

/// Malformed signature material is rejected (treated as tamper / fail-CLOSED).
#[test]
fn malformed_signature_fails_verification() {
    assert!(!verify_head(
        &SEED_A,
        KID,
        TENANT,
        REGION,
        &head_hex(),
        42,
        "not-base64!!!"
    ));
    let short = base64::engine::general_purpose::STANDARD.encode([0u8; 32]);
    assert!(!verify_head(
        &SEED_A,
        KID,
        TENANT,
        REGION,
        &head_hex(),
        42,
        &short
    ));
}

/// Resume re-verifies: a genuine signed checkpoint under the current key id →
/// Verified.
#[test]
fn resume_verifies_genuine_signed_head() {
    let cp = signed_checkpoint(&SEED_A, KID, &head_hex(), 9);
    assert_eq!(
        check_head_on_resume(Some(&cp), Some(&SEED_A), KID, TENANT, REGION, false),
        HeadResumeCheck::Verified
    );
}

/// Resume catches tampering: a checkpoint whose head_hex was rewritten after
/// signing → FailClosed (the drain refuses to extend). This is ALWAYS tamper —
/// the migration escape never softens a verify failure under the current key.
#[test]
fn resume_fails_closed_on_tampered_head() {
    let mut cp = signed_checkpoint(&SEED_A, KID, &head_hex(), 9);
    // Insider rewrites the head (keeping the original signature).
    cp.head_hex = "cd".repeat(32);
    cp.head = chain_hash_from_hex(&cp.head_hex).unwrap();
    assert_eq!(
        check_head_on_resume(Some(&cp), Some(&SEED_A), KID, TENANT, REGION, false),
        HeadResumeCheck::FailClosed
    );
    // Even with the migration escape ON, a current-key verify failure is tamper.
    assert_eq!(
        check_head_on_resume(Some(&cp), Some(&SEED_A), KID, TENANT, REGION, true),
        HeadResumeCheck::FailClosed
    );
}

/// THE LAUNDERING EXPLOIT (backend-audit §3), now CLOSED: an insider with D1
/// write rewrites the sealed rows + head and STRIPS the signature to NULL (they
/// lack the write-only seed, so they cannot re-sign). With the signing regime
/// active, a NULL signature is UNVERIFIABLE → FailClosed by default (was
/// silently `Proceed`, which let the honest drain re-sign the forged head).
#[test]
fn resume_fails_closed_on_stripped_signature() {
    let cp = HeadCheckpoint {
        head: chain_hash_from_hex(&head_hex()).unwrap(),
        head_hex: head_hex(),
        next_sequence: 9,
        head_signature: None,
        signing_key_id: None,
        epoch_id: None,
        head_message_version: None,
        epoch_ledger_sequence: None,
        epoch_ledger_hash: None,
        head_witness_sequence: None,
        head_witness_hash: None,
    };
    assert_eq!(
        check_head_on_resume(Some(&cp), Some(&SEED_A), KID, TENANT, REGION, false),
        HeadResumeCheck::FailClosed,
        "a NULL signature under an active signing regime must be tamper (fail-CLOSED)"
    );
}

/// A NULL / foreign-key-id head is tolerated ONLY under the explicit operator
/// migration escape (`trust_unsigned_resume = true`) — the pre-0080 legacy
/// bootstrap + coordinated seed-rotation re-sign path.
#[test]
fn resume_tolerates_unsigned_only_under_migration_escape() {
    let legacy = HeadCheckpoint {
        head: chain_hash_from_hex(&head_hex()).unwrap(),
        head_hex: head_hex(),
        next_sequence: 9,
        head_signature: None,
        signing_key_id: None,
        epoch_id: None,
        head_message_version: None,
        epoch_ledger_sequence: None,
        epoch_ledger_hash: None,
        head_witness_sequence: None,
        head_witness_hash: None,
    };
    assert_eq!(
        check_head_on_resume(Some(&legacy), Some(&SEED_A), KID, TENANT, REGION, true),
        HeadResumeCheck::Proceed
    );
    // Foreign key id (rotation) also tolerated only under the escape.
    let rotated = signed_checkpoint(&SEED_A, 1, &head_hex(), 9);
    assert_eq!(
        check_head_on_resume(Some(&rotated), Some(&SEED_B), 2, TENANT, REGION, true),
        HeadResumeCheck::Proceed
    );
}

/// The legacy head is RE-SIGNED on the next advance: signing it with the
/// configured seed yields a signature that verifies.
#[test]
fn legacy_head_gets_signed_on_next_advance() {
    // Simulate the advance signing the new head computed from the legacy state.
    let sig = sign_head(&SEED_A, KID, TENANT, REGION, &head_hex(), 10).unwrap();
    assert!(verify_head(
        &SEED_A,
        KID,
        TENANT,
        REGION,
        &head_hex(),
        10,
        &sig
    ));
    // And a checkpoint carrying that fresh signature now Verifies on resume.
    let cp = signed_checkpoint(&SEED_A, KID, &head_hex(), 10);
    assert_eq!(
        check_head_on_resume(Some(&cp), Some(&SEED_A), KID, TENANT, REGION, false),
        HeadResumeCheck::Verified
    );
}

/// A head signed under a DIFFERENT key id (seed/key rotation) is TAMPER by
/// default — it cannot be verified with the current seed, so a foreign key id is
/// as much a laundering vector as a stripped signature. Legit rotation is the
/// explicit migration escape (see `resume_tolerates_unsigned_only_under_migration_escape`).
#[test]
fn resume_fails_closed_on_foreign_key_id() {
    let cp = signed_checkpoint(&SEED_A, 1, &head_hex(), 9);
    // Current deployment uses key id 2 + a different seed — cannot verify.
    assert_eq!(
        check_head_on_resume(Some(&cp), Some(&SEED_B), 2, TENANT, REGION, false),
        HeadResumeCheck::FailClosed
    );
}

/// A signed head with NO seed configured to verify it → fail-CLOSED (cannot
/// prove integrity of a head that claims to be signed).
#[test]
fn resume_fails_closed_when_signed_but_no_seed() {
    let cp = signed_checkpoint(&SEED_A, KID, &head_hex(), 9);
    assert_eq!(
        check_head_on_resume(Some(&cp), None, KID, TENANT, REGION, false),
        HeadResumeCheck::FailClosed
    );
}

/// No signing regime (no seed) + a never-signed head → Proceed (dev/CI).
#[test]
fn resume_proceeds_unsigned_head_no_seed() {
    let cp = HeadCheckpoint {
        head: chain_hash_from_hex(&head_hex()).unwrap(),
        head_hex: head_hex(),
        next_sequence: 9,
        head_signature: None,
        signing_key_id: None,
        epoch_id: None,
        head_message_version: None,
        epoch_ledger_sequence: None,
        epoch_ledger_hash: None,
        head_witness_sequence: None,
        head_witness_hash: None,
    };
    assert_eq!(
        check_head_on_resume(Some(&cp), None, KID, TENANT, REGION, false),
        HeadResumeCheck::Proceed
    );
}

/// No checkpoint (genesis) → proceed.
#[test]
fn resume_proceeds_with_no_checkpoint() {
    assert_eq!(
        check_head_on_resume(None, Some(&SEED_A), KID, TENANT, REGION, false),
        HeadResumeCheck::Proceed
    );
}

/// `region_for_key` is irrelevant to the keypair: a macro region with no enum
/// (e.g. `apac`) still signs + verifies (the region is bound in the tuple,
/// not the key).
#[test]
fn macro_region_still_signs_and_verifies() {
    let sig = sign_head(&SEED_A, KID, TENANT, "apac", &head_hex(), 5).unwrap();
    assert!(verify_head(
        &SEED_A,
        KID,
        TENANT,
        "apac",
        &head_hex(),
        5,
        &sig
    ));
    // But it is region-bound: a different region must fail.
    assert!(!verify_head(
        &SEED_A,
        KID,
        TENANT,
        "afr",
        &head_hex(),
        5,
        &sig
    ));
}

// ---- B-038: partition lease + seal-loop fence ----

/// A batch of `n` deterministic sealed rows to drive the fenced loop.
fn sealed_rows(n: usize) -> Vec<SealedRow> {
    let payloads: Vec<Value> = (0..n).map(|i| json!({ "i": i })).collect();
    let (sealed, _, _) = seal_rows(ChainHash::genesis(), 0, &rows(&payloads)).unwrap();
    sealed
}

/// The fence decision is pure: it trips ONLY when the lease is enabled AND the
/// clock has reached the lease expiry. When the lease is OFF it is never `true`
/// regardless of the clock, so the seal loop always runs to completion.
#[test]
fn should_fence_only_when_enabled_and_expired() {
    // Lease OFF ⇒ never fence, even far past "expiry".
    assert!(!should_fence(1_000, 500, false));
    assert!(!should_fence(i64::MAX, 0, false));
    // Lease ON, before expiry ⇒ keep sealing.
    assert!(!should_fence(499, 500, true));
    // Lease ON, at/after expiry ⇒ fence (>= boundary is inclusive).
    assert!(should_fence(500, 500, true));
    assert!(should_fence(501, 500, true));
}

/// THE FENCE TEST (reaches the residual fork window). Drive the REAL seal loop
/// with a clock that crosses `my_lease_expires_ms` partway through and assert:
/// only the pre-expiry PREFIX was written, and the outcome is `Fenced(prefix)`
/// — so the caller does NOT advance the head. This is the path the prod probe
/// structurally cannot reach.
#[tokio::test]
async fn fence_stops_seal_loop_at_expiry_writing_only_the_prefix() {
    let sealed = sealed_rows(5);
    let expires = 1_000;
    // Clock: rows 0,1,2 see now < expiry (900); row 3 sees now == expiry
    // (1000) → fence BEFORE writing row 3. `should_fence` reads the clock once
    // per row, before each write, so 4 readings are consumed (rows 0..=3).
    let ticks = [900, 900, 900, 1_000, 1_000];
    let mut tick = ticks.into_iter();
    let clock = move || tick.next().unwrap_or(i64::MAX);

    let mut written: Vec<u64> = Vec::new();
    let outcome = run_fenced_seal_loop(&sealed, true, expires, clock, |row| {
        written.push(row.sequence_number);
        async { Ok(()) }
    })
    .await
    .unwrap();

    // Fenced after the 3-row prefix (sequences 0,1,2); rows 3,4 were NOT written.
    assert_eq!(outcome, FencedSeal::Fenced(3));
    assert_eq!(written, vec![0, 1, 2]);
}

/// With the lease OFF the fence is inert: the loop seals every row and reports
/// `Complete`, even when the clock is past a (would-be) expiry — proving the
/// flag-OFF default writes the whole batch exactly as before B-038.
#[tokio::test]
async fn lease_disabled_seals_whole_batch_completely() {
    let sealed = sealed_rows(4);
    let mut written: Vec<u64> = Vec::new();
    let outcome = run_fenced_seal_loop(
        &sealed,
        false,       // lease OFF
        0,           // expiry already in the past — must be ignored
        || i64::MAX, // clock way past expiry
        |row| {
            written.push(row.sequence_number);
            async { Ok(()) }
        },
    )
    .await
    .unwrap();
    assert_eq!(outcome, FencedSeal::Complete(4));
    assert_eq!(written, vec![0, 1, 2, 3]);
}

/// Lease ON but the clock never reaches expiry ⇒ the whole batch is sealed and
/// the head may advance (Complete). The fence only bites when the lease truly
/// expires mid-seal.
#[tokio::test]
async fn lease_enabled_but_not_expired_completes() {
    let sealed = sealed_rows(3);
    let mut written = 0u64;
    let outcome = run_fenced_seal_loop(
        &sealed,
        true,
        i64::MAX,
        || 0,
        |_row| {
            written += 1;
            async { Ok(()) }
        },
    )
    .await
    .unwrap();
    assert_eq!(outcome, FencedSeal::Complete(3));
    assert_eq!(written, 3);
}

/// A write error propagates (the loop is not swallowed by the fence).
#[tokio::test]
async fn seal_loop_propagates_write_error() {
    let sealed = sealed_rows(2);
    let err = run_fenced_seal_loop(
        &sealed,
        true,
        i64::MAX,
        || 0,
        |_row| async { Err("boom".to_string()) },
    )
    .await;
    assert_eq!(err, Err("boom".to_string()));
}

/// Sweep plumbing: a `Fenced(n)` outcome is accounted like a truncated batch —
/// its `n` rows count into `rows_sealed` and it forces `incomplete = true`,
/// while a `Leased` outcome only increments `partitions_leased`. This mirrors
/// the exact arithmetic the `handle_drain` match arms perform (kept in lock-step
/// with them) without needing a live D1.
#[test]
fn sweep_accounts_fenced_and_leased_outcomes() {
    // Replicate the match-arm accounting over a synthetic outcome stream.
    let outcomes = [
        PartitionOutcome::Sealed(4),
        PartitionOutcome::Leased,
        PartitionOutcome::Fenced(3),
        PartitionOutcome::Leased,
        PartitionOutcome::Drift,
        PartitionOutcome::Empty,
    ];
    let mut rows_sealed: u64 = 0;
    let mut partitions_drained: u64 = 0;
    let mut partitions_leased: u64 = 0;
    let mut partitions_drifted: u64 = 0;
    let mut incomplete = false;
    for o in outcomes {
        match o {
            PartitionOutcome::Sealed(n) => {
                partitions_drained += 1;
                rows_sealed += n;
            }
            PartitionOutcome::Fenced(n) => {
                rows_sealed += n;
                incomplete = true;
            }
            PartitionOutcome::Leased => partitions_leased += 1,
            PartitionOutcome::Drift => partitions_drifted += 1,
            PartitionOutcome::Empty => {}
        }
    }
    assert_eq!(rows_sealed, 7); // 4 sealed + 3 fenced prefix
    assert_eq!(partitions_drained, 1);
    assert_eq!(partitions_leased, 2);
    assert_eq!(partitions_drifted, 1);
    assert!(incomplete, "a fenced partition must force a re-drain");
}

/// Two lease holders minted for the same partition are DISTINCT, so a stealer's
/// holder-scoped release can never delete the other holder's lease.
#[test]
fn lease_holders_are_unique_per_invocation() {
    let a = new_lease_holder();
    let b = new_lease_holder();
    assert_ne!(a, b);
    assert_eq!(a.len(), 36, "uuid v4 hyphenated form is 36 chars");
}

/// The fence boundary equals the lease's written `expires_ms` (acquired_ms +
/// TTL), so a holder stops writing exactly when its lease becomes stealable —
/// never after. (The acquire SQL writes the same `acquired_ms + TTL`.)
#[test]
fn fence_expiry_matches_lease_ttl() {
    let acquired_ms = 1_700_000_000_000;
    let my_lease_expires_ms = acquired_ms + AUDIT_DRAIN_LEASE_TTL_MS;
    // At exactly the written expiry the holder fences (does not write past it).
    assert!(should_fence(my_lease_expires_ms, my_lease_expires_ms, true));
    // One ms before, it keeps going.
    assert!(!should_fence(
        my_lease_expires_ms - 1,
        my_lease_expires_ms,
        true
    ));
}

// ---- WP-L MED-21: `ok` is `partitions_failed == 0` ------------------
//
// The pre-fix shape returned `ok: true` regardless of how many
// partitions actually failed — masking a chronically-stuck chain
// under a green-looking response. The fix inverts that.

#[allow(dead_code)]
const B126_M2_TEST_1_1_REANCHOR: () = ();
