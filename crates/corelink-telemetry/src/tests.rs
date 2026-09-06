use super::*;
use proptest::prelude::*;

fn forge() -> LighthouseCustomer {
    LighthouseCustomer::recruit(CustomerId::new("LH-FORGE").expect("valid"), Tier::Team)
}

fn oss() -> LighthouseCustomer {
    LighthouseCustomer::recruit(CustomerId::new("LH-OSS-01").expect("valid"), Tier::Team)
}

fn ent() -> LighthouseCustomer {
    LighthouseCustomer::recruit(
        CustomerId::new("LH-ENT-BYOK-01").expect("valid"),
        Tier::EnterpriseByok,
    )
}

fn good_sample(cid: &CustomerId, ts: i64) -> SlaSample {
    SlaSample {
        sample_id: format!("s-{ts}"),
        customer_id: cid.clone(),
        sampled_at: ts,
        avail_cas_put_met: true,
        avail_cas_get_met: true,
        lat_cas_get_p99_met: true,
        fresh_billing_met: true,
        byok_key_health_ok: true,
    }
}

#[test]
fn customer_id_accepts_canonical_shape() {
    assert_eq!(
        CustomerId::new("LH-ENT-BYOK-01").unwrap().as_str(),
        "LH-ENT-BYOK-01"
    );
}

#[test]
fn customer_id_rejects_lowercase() {
    assert!(matches!(
        CustomerId::new("LH-forge"),
        Err(TrackerError::InvalidCustomerId(_))
    ));
}

#[test]
fn customer_id_rejects_missing_prefix() {
    assert!(matches!(
        CustomerId::new("FORGE-01"),
        Err(TrackerError::InvalidCustomerId(_))
    ));
}

#[test]
fn customer_id_rejects_empty_suffix() {
    assert!(matches!(
        CustomerId::new("LH-"),
        Err(TrackerError::InvalidCustomerId(_))
    ));
}

#[test]
fn state_machine_canonical_happy_path() {
    let mut c = forge();
    assert_eq!(c.state, LifecycleState::Recruiting);
    c.transition_to(LifecycleState::Engaged, 100, false)
        .unwrap();
    c.transition_to(LifecycleState::Migrating, 200, false)
        .unwrap();
    c.transition_to(LifecycleState::Observing, 300, false)
        .unwrap();
    // Need 30d before attestation.
    let after_window = 300 + OBSERVATION_WINDOW_SECS;
    c.transition_to(LifecycleState::Attested, after_window, false)
        .unwrap();
    c.transition_to(LifecycleState::CaseStudySigned, after_window + 1, false)
        .unwrap();
    assert!(c.state.is_terminal());
    assert!(c.attestation_signed_at.is_some());
    assert!(c.case_study_signed_at.is_some());
}

#[test]
fn state_machine_rejects_skip_transitions() {
    let mut c = forge();
    // Recruiting → Observing is illegal.
    assert!(matches!(
        c.transition_to(LifecycleState::Observing, 100, false),
        Err(TrackerError::IllegalTransition { .. })
    ));
    // State unchanged after illegal attempt.
    assert_eq!(c.state, LifecycleState::Recruiting);
}

#[test]
fn observation_window_must_elapse_before_attestation() {
    let mut c = forge();
    c.transition_to(LifecycleState::Engaged, 100, false)
        .unwrap();
    c.transition_to(LifecycleState::Migrating, 200, false)
        .unwrap();
    c.transition_to(LifecycleState::Observing, 300, false)
        .unwrap();
    // Only 29 days elapsed — must reject.
    let twenty_nine_days = 300 + (29 * 24 * 60 * 60);
    assert!(matches!(
        c.transition_to(LifecycleState::Attested, twenty_nine_days, false),
        Err(TrackerError::ObservationWindowNotElapsed { .. })
    ));
    assert_eq!(c.state, LifecycleState::Observing);
}

#[test]
fn sla_breach_blocks_attestation() {
    let mut c = forge();
    c.transition_to(LifecycleState::Engaged, 100, false)
        .unwrap();
    c.transition_to(LifecycleState::Migrating, 200, false)
        .unwrap();
    c.transition_to(LifecycleState::Observing, 300, false)
        .unwrap();
    let mut bad = good_sample(&c.customer_id, 400);
    bad.lat_cas_get_p99_met = false;
    c.record_sample(&bad);
    let after = 300 + OBSERVATION_WINDOW_SECS;
    assert!(matches!(
        c.transition_to(LifecycleState::Attested, after, false),
        Err(TrackerError::SlaBreachBlocksAttestation)
    ));
}

#[test]
fn enterprise_requires_byok_health_for_attestation() {
    let mut c = ent();
    c.transition_to(LifecycleState::Engaged, 100, false)
        .unwrap();
    c.transition_to(LifecycleState::Migrating, 200, false)
        .unwrap();
    c.transition_to(LifecycleState::Observing, 300, false)
        .unwrap();
    let after = 300 + OBSERVATION_WINDOW_SECS;
    // byok_health_ok=false must block.
    assert!(matches!(
        c.transition_to(LifecycleState::Attested, after, false),
        Err(TrackerError::ByokKeyHealthFailed)
    ));
    // byok_health_ok=true succeeds.
    c.transition_to(LifecycleState::Attested, after, true)
        .unwrap();
    assert_eq!(c.state, LifecycleState::Attested);
}

#[test]
fn team_tier_ignores_byok_health() {
    let mut c = forge();
    c.transition_to(LifecycleState::Engaged, 100, false)
        .unwrap();
    c.transition_to(LifecycleState::Migrating, 200, false)
        .unwrap();
    c.transition_to(LifecycleState::Observing, 300, false)
        .unwrap();
    let after = 300 + OBSERVATION_WINDOW_SECS;
    // Team tier — byok_health_ok=false MUST NOT block.
    c.transition_to(LifecycleState::Attested, after, false)
        .unwrap();
    assert_eq!(c.state, LifecycleState::Attested);
}

#[test]
fn withdrawn_reachable_pre_attestation() {
    for from in [
        LifecycleState::Recruiting,
        LifecycleState::Engaged,
        LifecycleState::Migrating,
    ] {
        let mut c = forge();
        // Force the customer into the source state via legal transitions.
        let mut ts = 100;
        if from != LifecycleState::Recruiting {
            c.transition_to(LifecycleState::Engaged, ts, false).unwrap();
            ts += 100;
        }
        if from == LifecycleState::Migrating {
            c.transition_to(LifecycleState::Migrating, ts, false)
                .unwrap();
            ts += 100;
        }
        c.transition_to(LifecycleState::Withdrawn, ts, false)
            .unwrap();
        assert_eq!(c.state, LifecycleState::Withdrawn);
    }
}

#[test]
fn withdrawn_not_reachable_post_attestation() {
    let mut c = forge();
    c.transition_to(LifecycleState::Engaged, 100, false)
        .unwrap();
    c.transition_to(LifecycleState::Migrating, 200, false)
        .unwrap();
    c.transition_to(LifecycleState::Observing, 300, false)
        .unwrap();
    let after = 300 + OBSERVATION_WINDOW_SECS;
    c.transition_to(LifecycleState::Attested, after, false)
        .unwrap();
    assert!(matches!(
        c.transition_to(LifecycleState::Withdrawn, after + 1, false),
        Err(TrackerError::IllegalTransition { .. })
    ));
}

#[test]
fn record_sample_flags_breach() {
    let mut c = forge();
    let good = good_sample(&c.customer_id, 100);
    c.record_sample(&good);
    assert!(!c.sla_breach_recorded);
    let mut bad = good_sample(&c.customer_id, 200);
    bad.avail_cas_put_met = false;
    c.record_sample(&bad);
    assert!(c.sla_breach_recorded);
}

#[test]
fn observation_outcome_pending_before_window() {
    let mut c = forge();
    c.transition_to(LifecycleState::Engaged, 100, false)
        .unwrap();
    c.transition_to(LifecycleState::Migrating, 200, false)
        .unwrap();
    c.transition_to(LifecycleState::Observing, 300, false)
        .unwrap();
    assert_eq!(c.observation_outcome(400), ObservationOutcome::Pending);
}

#[test]
fn observation_outcome_met_after_window_no_breach() {
    let mut c = forge();
    c.transition_to(LifecycleState::Engaged, 100, false)
        .unwrap();
    c.transition_to(LifecycleState::Migrating, 200, false)
        .unwrap();
    c.transition_to(LifecycleState::Observing, 300, false)
        .unwrap();
    let after = 300 + OBSERVATION_WINDOW_SECS;
    assert_eq!(c.observation_outcome(after), ObservationOutcome::Met);
}

#[test]
fn observation_outcome_miss_when_breach() {
    let mut c = forge();
    c.transition_to(LifecycleState::Engaged, 100, false)
        .unwrap();
    c.transition_to(LifecycleState::Migrating, 200, false)
        .unwrap();
    c.transition_to(LifecycleState::Observing, 300, false)
        .unwrap();
    let mut bad = good_sample(&c.customer_id, 400);
    bad.fresh_billing_met = false;
    c.record_sample(&bad);
    let after = 300 + OBSERVATION_WINDOW_SECS;
    assert_eq!(c.observation_outcome(after), ObservationOutcome::Miss);
}

#[test]
fn ga_gate_met_with_3_attested() {
    let mut f = forge();
    let mut o = oss();
    let mut e = ent();
    for c in [&mut f, &mut o, &mut e] {
        c.transition_to(LifecycleState::Engaged, 100, false)
            .unwrap();
        c.transition_to(LifecycleState::Migrating, 200, false)
            .unwrap();
        c.transition_to(LifecycleState::Observing, 300, false)
            .unwrap();
    }
    let after = 300 + OBSERVATION_WINDOW_SECS;
    f.transition_to(LifecycleState::Attested, after, false)
        .unwrap();
    o.transition_to(LifecycleState::Attested, after, false)
        .unwrap();
    e.transition_to(LifecycleState::Attested, after, true)
        .unwrap();

    let report = compute_ga_gate(&[f, o, e]).unwrap();
    assert_eq!(report.team_attested, 2);
    assert_eq!(report.enterprise_attested, 1);
    assert!(report.gate_met);
}

#[test]
fn ga_gate_not_met_with_2_team_only() {
    let mut f = forge();
    let mut o = oss();
    for c in [&mut f, &mut o] {
        c.transition_to(LifecycleState::Engaged, 100, false)
            .unwrap();
        c.transition_to(LifecycleState::Migrating, 200, false)
            .unwrap();
        c.transition_to(LifecycleState::Observing, 300, false)
            .unwrap();
    }
    let after = 300 + OBSERVATION_WINDOW_SECS;
    f.transition_to(LifecycleState::Attested, after, false)
        .unwrap();
    o.transition_to(LifecycleState::Attested, after, false)
        .unwrap();
    let report = compute_ga_gate(&[f, o]).unwrap();
    assert!(!report.gate_met);
    assert_eq!(report.team_attested, 2);
    assert_eq!(report.enterprise_attested, 0);
}

#[test]
fn ga_gate_rejects_overfilled_team_slots() {
    let t1 = LighthouseCustomer::recruit(CustomerId::new("LH-T1").unwrap(), Tier::Team);
    let t2 = LighthouseCustomer::recruit(CustomerId::new("LH-T2").unwrap(), Tier::Team);
    let t3 = LighthouseCustomer::recruit(CustomerId::new("LH-T3").unwrap(), Tier::Team);
    assert!(matches!(
        compute_ga_gate(&[t1, t2, t3]),
        Err(TrackerError::SlotAllocationViolation(_))
    ));
}

#[test]
fn ga_gate_rejects_oversized_roster() {
    let roster: Vec<LighthouseCustomer> = (0..4)
        .map(|i| {
            LighthouseCustomer::recruit(CustomerId::new(format!("LH-X{i}")).unwrap(), Tier::Team)
        })
        .collect();
    assert!(matches!(
        compute_ga_gate(&roster),
        Err(TrackerError::SlotAllocationViolation(_))
    ));
}

#[test]
fn ga_gate_ignores_withdrawn_slot() {
    let mut withdrawn =
        LighthouseCustomer::recruit(CustomerId::new("LH-WITHDRAWN").unwrap(), Tier::Team);
    withdrawn
        .transition_to(LifecycleState::Withdrawn, 100, false)
        .unwrap();

    let mut f = forge();
    let mut o = oss();
    let mut e = ent();
    for c in [&mut f, &mut o, &mut e] {
        c.transition_to(LifecycleState::Engaged, 100, false)
            .unwrap();
        c.transition_to(LifecycleState::Migrating, 200, false)
            .unwrap();
        c.transition_to(LifecycleState::Observing, 300, false)
            .unwrap();
    }
    let after = 300 + OBSERVATION_WINDOW_SECS;
    f.transition_to(LifecycleState::Attested, after, false)
        .unwrap();
    o.transition_to(LifecycleState::Attested, after, false)
        .unwrap();
    e.transition_to(LifecycleState::Attested, after, true)
        .unwrap();

    // 4 entries (3 active + 1 withdrawn) MUST still violate roster cap
    // (we count slots, not active slots, to keep the audit trail honest).
    assert!(matches!(
        compute_ga_gate(&[withdrawn.clone(), f.clone(), o.clone(), e.clone()]),
        Err(TrackerError::SlotAllocationViolation(_))
    ));

    // 3-slot roster with one withdrawn + 2 active team + 1 enterprise:
    // gate not met because team_attested = 2 only if backup picks up.
    // Here the withdrawn slot is a team slot so we only have 1 team
    // active.
    let report = compute_ga_gate(&[withdrawn, f, e]).unwrap();
    assert_eq!(report.team_attested, 1);
    assert_eq!(report.enterprise_attested, 1);
    assert!(!report.gate_met);
}

#[test]
fn negative_timestamp_rejected() {
    let mut c = forge();
    assert!(matches!(
        c.transition_to(LifecycleState::Engaged, -1, false),
        Err(TrackerError::NegativeTimestamp(_))
    ));
}

// ── Property tests ──────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(
        std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(256),
    ))]

    /// State machine never allows skip-transitions regardless of (from, to)
    /// pairs — fail-CLOSED invariant.
    #[test]
    fn prop_illegal_transitions_always_rejected(
        from_idx in 0usize..7,
        to_idx in 0usize..7,
        ts in 0i64..1_000_000_000,
    ) {
        let states = [
            LifecycleState::Recruiting,
            LifecycleState::Engaged,
            LifecycleState::Migrating,
            LifecycleState::Observing,
            LifecycleState::Attested,
            LifecycleState::CaseStudySigned,
            LifecycleState::Withdrawn,
        ];
        // proptest indices are bounded, so indexing is sound; we suppress
        // the lint locally because the alternative (.get().unwrap()) is
        // semantically identical and the lint is a workspace default
        // designed to catch unbounded indexing.
        #[allow(clippy::indexing_slicing)]
        let from = states[from_idx];
        #[allow(clippy::indexing_slicing)]
        let to = states[to_idx];

        let mut c = forge();
        c.state = from;
        // Make sure observation timestamp is set so the gate check on
        // Observing→Attested fires the right error path.
        c.observation_started_at = Some(0);

        if from.can_transition_to(to) {
            // Legal — may still fail on a precondition (window, breach,
            // byok), but never on IllegalTransition.
            let r = c.transition_to(to, ts, true);
            let is_illegal = matches!(r, Err(TrackerError::IllegalTransition { .. }));
            prop_assert!(!is_illegal, "legal transition reported as illegal");
        } else {
            // Illegal — MUST reject with IllegalTransition.
            let r = c.transition_to(to, ts, true);
            let is_illegal = matches!(r, Err(TrackerError::IllegalTransition { .. }));
            prop_assert!(is_illegal);
            prop_assert_eq!(c.state, from);
        }
    }

    /// SLA breach flag is monotonic — once set, it never resets through
    /// `record_sample` (only operator-driven remediation cycle can reset,
    /// which is outside library scope).
    #[test]
    fn prop_sla_breach_monotonic(
        samples in proptest::collection::vec(any::<bool>(), 1..50),
    ) {
        let mut c = forge();
        let mut breach_seen = false;
        for (i, &met) in samples.iter().enumerate() {
            let mut s = good_sample(&c.customer_id, i as i64);
            if !met {
                s.lat_cas_get_p99_met = false;
                breach_seen = true;
            }
            c.record_sample(&s);
            prop_assert_eq!(c.sla_breach_recorded, breach_seen);
        }
    }
}
