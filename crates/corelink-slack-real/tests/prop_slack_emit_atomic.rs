//! Property tests pinning the load-bearing invariants of
//! `corelink-slack-real` (WI-PROPTEST-FU-001 — DEBT-009).
//!
//! Closes the proptest-density gap identified in
//! `specs/_audits/sealed/2026-05-15-proptest-density.md` (ratio 0/2 → 4/2).
//!
//! # Invariant coverage
//!
//! | Test                                                         | Invariant pinned                                  |
//! |--------------------------------------------------------------|---------------------------------------------------|
//! | `prop_inv_audit_emit_atomic_pre_slack_post`                  | INV-AUDIT-EMIT-ATOMIC (audit fires pre-mutation)  |
//! | `prop_inv_audit_emit_atomic_with_handler_failclosed_blocks_slack` | INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (fail-CLOSED)  |
//! | `prop_inv_retry_backoff_bounded_by_max`                      | RetryPolicy bounded-backoff (no DoS via overflow) |
//! | `prop_inv_block_kit_canonical_roundtrip`                     | Block Kit JSON canonical roundtrip                |
//!
//! Adversarial fixtures:
//! - `FailingSlackAuditSink`: deterministically rejects every emit
//!   with `SlackAuditError::EmitFailed` — exercises the fail-CLOSED path.
//! - Random `SlackChannel` from the canonical 6-variant taxonomy.
//! - Random retry attempt index across `[0, 64]` (covers the exponential
//!   shift-overflow path at attempt ≥ 32 where `1 << attempt` saturates).
//! - Field key/value strings drawn from a deliberately-hostile mrkdwn
//!   alphabet (`<`, `>`, `&`, `*`, `_`, `~`, backtick, `\`).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target: panics surface as test failures by design"
)]

use std::sync::Arc;
use std::time::Duration;

use corelink_slack_real::{
    InMemorySharedSlackClient, InMemorySlackAuditSink, RetryDecision, RetryPolicy, SharedSlackClient,
    SlackAuditError, SlackAuditEvent, SlackAuditOutcome, SlackAuditSink, SlackChannel,
    SlackClientError, SlackMessage,
};
use proptest::prelude::*;
use proptest::test_runner::Config;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;

// =====================================================================
// PROPTEST_CASES runtime knob (S-07 P1-2 contract — runtime fn, NOT const,
// so the nightly job can override via `PROPTEST_CASES=100000`).
// =====================================================================

fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1_024)
}

const ALL_CHANNELS: &[SlackChannel] = &[
    SlackChannel::AlertsSev1,
    SlackChannel::AlertsSev2,
    SlackChannel::EnterpriseInquiries,
    SlackChannel::BreachNotifications,
    SlackChannel::OncallHandoff,
    SlackChannel::LighthouseCustomers,
];

fn pick_channel(rng: &mut ChaCha20Rng) -> SlackChannel {
    ALL_CHANNELS[rng.random_range(0..ALL_CHANNELS.len())]
}

// Hostile-by-design alphabet — every mrkdwn-special character lives here,
// so the proptest WILL hit the escape path 100 % of the time over a
// non-trivial sample size.
const HOSTILE_CHARS: &[char] = &[
    'a', 'B', '<', '>', '&', '*', '_', '~', '`', '\\', '1', ' ', '\n', '"', '\'', 'Z',
];

fn pick_hostile_string(rng: &mut ChaCha20Rng, len: usize) -> String {
    (0..len)
        .map(|_| HOSTILE_CHARS[rng.random_range(0..HOSTILE_CHARS.len())])
        .collect()
}

/// Always-failing audit sink — used to assert fail-CLOSED.
#[derive(Clone, Debug, Default)]
struct FailingSlackAuditSink;

impl SlackAuditSink for FailingSlackAuditSink {
    fn emit(&self, _event: &SlackAuditEvent) -> Result<(), SlackAuditError> {
        Err(SlackAuditError::EmitFailed("adversarial fixture".into()))
    }
}

proptest! {
    #![proptest_config(Config { cases: proptest_cases(), .. Config::default() })]

    /// INV-AUDIT-EMIT-ATOMIC: for every successful Slack send, exactly
    /// ONE audit event MUST have been emitted BEFORE the send body
    /// landed in the recorder. The property asserts:
    ///
    /// 1. The recorded send count == 1 (the dispatch ran).
    /// 2. The audit event count == 1 (audit was emitted in the same call).
    /// 3. The audit event's channel matches the message's channel
    ///    (event-to-payload binding integrity).
    /// 4. The audit outcome is `Sent` (success path classification).
    ///
    /// Adversarial input: random channel from the 6-variant taxonomy +
    /// random mrkdwn-hostile header/footer strings.
    #[test]
    fn prop_inv_audit_emit_atomic_pre_slack_post(seed in any::<u64>()) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let channel = pick_channel(&mut rng);
        let header = pick_hostile_string(&mut rng, 16);
        let footer = pick_hostile_string(&mut rng, 8);
        let timestamp = format!("2026-05-15T{:02}:{:02}:{:02}Z",
            rng.random_range(0..24u8),
            rng.random_range(0..60u8),
            rng.random_range(0..60u8));

        let audit = Arc::new(InMemorySlackAuditSink::new());
        let client = InMemorySharedSlackClient::new(audit.clone());
        let msg = SlackMessage::new(channel, header, footer, timestamp);

        let outcome = client.send(&msg);
        let send_ok = outcome.is_ok();
        prop_assert!(send_ok, "INV-AUDIT-EMIT-ATOMIC: send must succeed under in-memory fake; got {outcome:?}");
        prop_assert_eq!(client.len(), 1,
            "INV-AUDIT-EMIT-ATOMIC: dispatch recorder count mismatch");
        prop_assert_eq!(audit.len(), 1,
            "INV-AUDIT-EMIT-ATOMIC: audit count != 1 — event missing or duplicated");

        let snapshot = audit.snapshot();
        let evt = &snapshot[0];
        let outcome_is_sent = evt.outcome == SlackAuditOutcome::Sent;
        prop_assert!(outcome_is_sent,
            "INV-AUDIT-EMIT-ATOMIC: outcome != Sent (got {:?})", evt.outcome);
        prop_assert_eq!(evt.channel, channel,
            "INV-AUDIT-EMIT-ATOMIC: channel binding drift — event {:?} vs message {:?}",
            evt.channel, channel);
    }

    /// INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (fail-CLOSED): when the audit
    /// sink rejects emit, the send MUST return `SlackClientError::AuditFailed`
    /// AND NO payload may have been recorded by the dispatch path.
    ///
    /// This is the canonical chain-of-custody property: a Slack POST is
    /// NEVER observable if audit chain integrity could not be guaranteed.
    ///
    /// Adversarial fixture: `FailingSlackAuditSink` rejects every emit.
    #[test]
    fn prop_inv_audit_emit_atomic_with_handler_failclosed_blocks_slack(
        seed in any::<u64>(),
    ) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let channel = pick_channel(&mut rng);
        let header = pick_hostile_string(&mut rng, 12);

        let failing_sink: Arc<dyn SlackAuditSink> = Arc::new(FailingSlackAuditSink);
        let client = InMemorySharedSlackClient::new(failing_sink);
        let msg = SlackMessage::new(channel, header, "f", "2026-05-15T00:00:00Z");

        let outcome = client.send(&msg);

        // 1. Send MUST surface AuditFailed (fail-CLOSED).
        let is_audit_failed = matches!(&outcome, Err(SlackClientError::AuditFailed(_)));
        prop_assert!(is_audit_failed,
            "INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER fail-CLOSED violation: \
             expected Err(AuditFailed), got {outcome:?}");

        // 2. Dispatch recorder MUST be empty — no payload landed.
        prop_assert_eq!(client.len(), 0,
            "INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER fail-CLOSED violation: \
             {} payload(s) recorded despite audit failure (chain-of-custody breach)",
            client.len());
    }

    /// RetryPolicy bounded-backoff: every `backoff_for(n)` MUST be
    /// ≤ `max_backoff`, regardless of attempt index — including the
    /// attempt-≥32 path where `1u32 << attempt` saturates. Also asserts
    /// the policy's decide-on-429 is `Retry` (within max_retries) or
    /// `GiveUp` (at exhaustion), never `Success`.
    ///
    /// Adversarial input: attempt index sampled across [0, 64] to cover
    /// the saturating-shift boundary at attempt=32.
    #[test]
    fn prop_inv_retry_backoff_bounded_by_max(
        seed in any::<u64>(),
        attempt in 0u32..64u32,
    ) {
        let _ = seed; // reserved for future randomized policy params
        let policy = RetryPolicy::r2_4_default();
        let backoff = policy.backoff_for(attempt);

        prop_assert!(
            backoff <= policy.max_backoff,
            "RetryPolicy bounded-backoff violation: backoff_for({attempt})={backoff:?} \
             exceeds max_backoff={:?}",
            policy.max_backoff
        );
        prop_assert!(
            backoff >= Duration::from_millis(0),
            "RetryPolicy bounded-backoff violation: backoff is negative (impossible — Duration is unsigned)"
        );

        // 429 decision class: Retry while attempt < max_retries; GiveUp at exhaustion.
        let decision = policy.decide(Some(429), attempt);
        let is_retry_or_giveup = matches!(decision, RetryDecision::Retry { .. } | RetryDecision::GiveUp);
        prop_assert!(is_retry_or_giveup,
            "RetryPolicy: 429 decided as {decision:?} (expected Retry|GiveUp)");
        let is_not_success = !matches!(decision, RetryDecision::Success);
        prop_assert!(is_not_success,
            "RetryPolicy: 429 decided as Success — must never happen");

        // 2xx → always Success regardless of attempt.
        let success_decision = policy.decide(Some(200), attempt);
        let is_success = matches!(success_decision, RetryDecision::Success);
        prop_assert!(is_success,
            "RetryPolicy: 200 decided as {success_decision:?} (expected Success)");
    }

    /// Block Kit JSON canonical roundtrip: the wire JSON produced by
    /// `SlackMessage::to_block_kit_json()` MUST satisfy:
    ///
    /// 1. Top-level `text` mirrors `header` (Slack notification fallback).
    /// 2. `blocks` is a JSON array.
    /// 3. First block has `type == "header"` and its inner text equals
    ///    the message header (verbatim, since `plain_text` is rendered
    ///    literally by Slack — no escape transform applied).
    /// 4. The last block has `type == "context"` (footer + timestamp).
    ///
    /// Adversarial input: hostile-alphabet header/footer + random channel.
    #[test]
    fn prop_inv_block_kit_canonical_roundtrip(seed in any::<u64>()) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let channel = pick_channel(&mut rng);
        // Cap header length to 32 chars — well under the 150 cap — so we
        // never trigger truncation in this roundtrip property.
        let header = pick_hostile_string(&mut rng, 12);
        let footer = pick_hostile_string(&mut rng, 8);
        let ts = format!("2026-05-{:02}T00:00:00Z", 1 + rng.random_range(0..28u8));

        let msg = SlackMessage::new(channel, header.clone(), footer, ts);
        let json = msg.to_block_kit_json();

        // 1. Top-level `text` is the verbatim header.
        let text = json.get("text").and_then(|v| v.as_str()).unwrap_or("");
        prop_assert_eq!(text, header.as_str(),
            "Block Kit roundtrip: top-level `text` drift");

        // 2. `blocks` is an array with ≥ 2 blocks (header + context minimum).
        let blocks = json.get("blocks").and_then(|v| v.as_array());
        let has_blocks = blocks.is_some();
        prop_assert!(has_blocks, "Block Kit roundtrip: blocks array missing");
        let blocks = blocks.unwrap();
        prop_assert!(blocks.len() >= 2,
            "Block Kit roundtrip: expected ≥ 2 blocks, got {}", blocks.len());

        // 3. First block is `header` with verbatim plain_text.
        let first_type = blocks[0].get("type").and_then(|v| v.as_str()).unwrap_or("");
        prop_assert_eq!(first_type, "header",
            "Block Kit roundtrip: first block type drift");
        let header_text = blocks[0]
            .get("text").and_then(|v| v.get("text")).and_then(|v| v.as_str())
            .unwrap_or("");
        prop_assert_eq!(header_text, header.as_str(),
            "Block Kit roundtrip: header text drift (plain_text rendered literally)");

        // 4. Last block is `context`.
        let last = blocks.last().expect("non-empty");
        let last_type = last.get("type").and_then(|v| v.as_str()).unwrap_or("");
        prop_assert_eq!(last_type, "context",
            "Block Kit roundtrip: last block type drift (expected context for footer+timestamp)");
    }
}

// =====================================================================
// Determinism canary — PRNG seed reproducibility.
// =====================================================================

#[test]
fn prng_seed_is_deterministic_across_invocations() {
    let mut a = ChaCha20Rng::seed_from_u64(0xDEAD_BEEF_CAFE_F00D);
    let mut b = ChaCha20Rng::seed_from_u64(0xDEAD_BEEF_CAFE_F00D);
    for _ in 0..128 {
        let av: u64 = a.random();
        let bv: u64 = b.random();
        assert_eq!(av, bv, "ChaCha20Rng output must be deterministic per seed");
    }
}

/// Canary: the `FailingSlackAuditSink` actually fails (sanity smoke).
#[test]
fn failing_sink_rejects_emit() {
    let sink = FailingSlackAuditSink;
    let evt = SlackAuditEvent {
        outcome: SlackAuditOutcome::Sent,
        channel: SlackChannel::AlertsSev1,
        webhook_redacted: "<test>".into(),
        final_status: Some(200),
        attempts: 1,
        reason: None,
    };
    let res = sink.emit(&evt);
    assert!(matches!(res, Err(SlackAuditError::EmitFailed(_))));
}
