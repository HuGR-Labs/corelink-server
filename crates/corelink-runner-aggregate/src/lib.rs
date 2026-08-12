//! `corelink-runner-aggregate` — the runner compute-overage aggregation core
//! for WI-S10-007 Phase B (shadow).
//!
//! Given the staged `runner_vcpu_seconds` usage events for one billing period
//! (drained from `usage_event_staging` by the credentialed cron workflow), the
//! per-tenant runner tier map, and the per-region prior hash-chain heads, this
//! crate:
//!
//!   1. aggregates the events per `(tenant_id, region, billing_period)` into a
//!      counter row (SUM of vCPU-seconds + event count),
//!   2. links each counter into the per-region BLAKE3 hash chain (reusing
//!      [`corelink_billing_aggregator::HashChainBuilder`]), producing the
//!      `prev_hash` / `own_digest` the `runner_usage_counter` row (migration
//!      0094) carries + the advanced `runner_hash_chain_head`, and
//!   3. computes the per-tenant **shadow charge** — what WOULD be billed —
//!      reusing [`corelink_runner_overage`], with NO Stripe contact.
//!
//! ## Purity / the credentialed I/O boundary
//!
//! [`aggregate_runner_usage`] is pure: it takes a [`RunnerAggregateInput`] and
//! returns a [`RunnerAggregateOutput`] with no I/O, no credentials, and no clock
//! read (the `now_ms` timestamp is passed in). The `runner-aggregate-run` binary
//! is the same JSON-in/JSON-out shell. The credentialed D1 drain (SELECT the
//! un-aggregated staged rows) and the atomic write (the `runner_usage_counter`
//! UPSERT together with the `runner_hash_chain_head` UPDATE, in one D1 batch)
//! are the cron workflow's job, exactly as `billing-reconcile-run` leaves the
//! credentialed extraction to its workflow. This keeps the money logic
//! unit-falsifiable and keeps secrets out of the Rust surface.
//!
//! ## Determinism + the watermark contract
//!
//! Regions and tenants are walked in `BTreeMap` order and each aggregate's
//! `idem_keys_seen` is sorted, so the same input yields byte-identical canonical
//! bytes and therefore an identical chain digest — the property the durable
//! store's replay-idempotency depends on. The chain advances once per
//! `(tenant, region)` aggregate per call, so the **workflow MUST feed only
//! not-yet-aggregated staged rows** (watermark) — re-feeding the same events
//! would double-advance the chain. Shadow-only here; the durable write phase
//! (WP-3c) binds the watermark + the `(tenant, region, billing_period)` UPSERT.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use corelink_billing_aggregator::{AggregatedCounter, ChainHash, HashChainBuilder};
use corelink_billing_emit::event::{IdemKey, UsageEventKind};
use corelink_runner_overage::{
    millicents_to_cents, overage_vcpu_hours_decimal, overage_vcpu_seconds,
    shadow_charge_millicents, RunnerTier, DEFAULT_OVERAGE_RATE_CENTS_PER_VCPU_HOUR,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Fixed UUIDv5 namespace for deriving each aggregate's deterministic `id` from
/// `(tenant_id, region, billing_period)`. A stable namespace keeps the CloudEvent
/// `id` — and therefore the chain's canonical bytes — reproducible across runs
/// (no clock / random in the id path).
const AGG_ID_NAMESPACE: Uuid = Uuid::from_u128(0x5751_0007_0094_0000_0000_0000_0000_0001);

/// CloudEvents `source` stamped on each runner aggregate.
const RUNNER_AGG_SOURCE: &str = "corelink/billing/runner-aggregate";

/// Aggregation error surface.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RunnerAggregateError {
    /// A staged event carried a malformed field (bad uuid, non-64-hex idem_key,
    /// bad billing period).
    #[error("malformed staged event: {0}")]
    MalformedEvent(String),
    /// A prior chain head hex was not 64 hex chars / not decodable.
    #[error("malformed prior chain head for region {region}: {detail}")]
    MalformedChainHead {
        /// The region whose prior head was malformed.
        region: String,
        /// What was wrong.
        detail: String,
    },
    /// The hash-chain builder rejected an append (sequence / prev_hash break).
    #[error("hash chain break in region {region}: {detail}")]
    ChainBreak {
        /// The region whose chain broke.
        region: String,
        /// The underlying chain-break detail.
        detail: String,
    },
}

/// One raw runner usage record drained from `usage_event_staging`
/// (`event_kind = "runner_vcpu_seconds"`). Field names mirror the staging /
/// emit vocabulary so the workflow can project D1 rows straight to this shape.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StagedRunnerEvent {
    /// Owning tenant UUID.
    pub tenant_id: Uuid,
    /// 3-char region code (iad/fra/nrt/syd/gru).
    pub region: String,
    /// Billable quantity in vCPU-seconds (`qty` of the `runner_vcpu_seconds`
    /// meter = wall-clock seconds × box vCPU count).
    pub qty_vcpu_seconds: u128,
    /// The event's BLAKE3 idempotency key (64 hex chars).
    pub idem_key_hex: String,
    /// Emit wall-clock (unix ms) — used only to order events deterministically
    /// within a group; never as the aggregation clock.
    pub time_ms: u64,
}

/// The per-region prior hash-chain head (from `runner_hash_chain_head`), so the
/// chain resumes rather than restarting each run.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PriorChainHead {
    /// Current head (64 hex chars).
    pub current_head_hex: String,
    /// Sequence number of the next aggregate to append.
    pub next_sequence: u64,
}

/// The pure aggregation input (one billing period).
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RunnerAggregateInput {
    /// The billing period, `YYYY-MM`.
    pub billing_period: String,
    /// Inclusive period-start wall-clock (unix ms) stamped on each aggregate.
    pub period_start_ms: u64,
    /// Exclusive period-end wall-clock (unix ms) stamped on each aggregate.
    pub period_end_ms: u64,
    /// The aggregation-run wall-clock (unix ms), stamped as each aggregate's
    /// `time_ms` + the counter row's `aggregated_at`. Passed in (never read from
    /// a clock) so the function stays pure + reproducible.
    pub now_ms: u64,
    /// Overage rate in whole US cents per vCPU-hour (shadow only). Defaults to
    /// [`DEFAULT_OVERAGE_RATE_CENTS_PER_VCPU_HOUR`] when absent.
    #[serde(default)]
    pub rate_cents_per_vcpu_hour: Option<u64>,
    /// The staged runner events to aggregate (the workflow feeds only
    /// not-yet-aggregated rows — the watermark).
    pub staged: Vec<StagedRunnerEvent>,
    /// Tenant UUID → runner tier SKU (`runner_starter`..`runner_max`), from
    /// `runners_entitlement`. A tenant absent here (or with an unknown SKU) is
    /// reported in `skipped` and gets no shadow line.
    #[serde(default)]
    pub tenant_tiers: BTreeMap<Uuid, String>,
    /// Region code → prior chain head. A region absent here starts at genesis.
    #[serde(default)]
    pub prior_chain_heads: BTreeMap<String, PriorChainHead>,
}

/// One aggregated counter row destined for `runner_usage_counter` (migration
/// 0094). The workflow writes these + the chain-head updates in one atomic D1
/// batch.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct CounterRow {
    /// Owning tenant.
    pub tenant_id: Uuid,
    /// Region code.
    pub region: String,
    /// Billing period `YYYY-MM`.
    pub billing_period: String,
    /// SUM of `runner_vcpu_seconds` in this `(tenant, region, period)`.
    pub vcpu_seconds: u128,
    /// Number of staged records folded in.
    pub event_count: u64,
    /// Aggregation wall-clock (unix ms) = `now_ms`.
    pub aggregated_at_ms: u64,
    /// The chain link slot (previous head), 64 hex.
    pub prev_hash_hex: String,
    /// This row's digest (= next `prev_hash`), 64 hex.
    pub own_digest_hex: String,
}

/// The advanced per-region chain head destined for `runner_hash_chain_head`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ChainHeadUpdate {
    /// Region code.
    pub region: String,
    /// New current head (64 hex).
    pub current_head_hex: String,
    /// Sequence number of the next aggregate to append after this run.
    pub next_sequence: u64,
    /// The last aggregated period `YYYY-MM`.
    pub last_aggregated_period: String,
}

/// One per-tenant shadow-charge line (what WOULD be billed this period).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ShadowLine {
    /// Owning tenant.
    pub tenant_id: Uuid,
    /// Billing period `YYYY-MM`.
    pub billing_period: String,
    /// The tenant's runner tier SKU.
    pub tier_sku: String,
    /// Total vCPU-seconds across ALL regions this period.
    pub total_vcpu_seconds: u128,
    /// The tier's included allowance, in vCPU-seconds.
    pub allowance_vcpu_seconds: u128,
    /// Overage above the allowance, in vCPU-seconds.
    pub overage_vcpu_seconds: u128,
    /// Overage as the Stripe meter quantity, vCPU-hours (exact decimal string).
    pub overage_vcpu_hours: String,
    /// The rate applied, whole cents per vCPU-hour.
    pub rate_cents_per_vcpu_hour: u64,
    /// The would-be charge, in millicents (canonical; avoids compounding round).
    pub shadow_charge_millicents: u128,
    /// The would-be charge, whole US cents (display).
    pub shadow_charge_cents: u128,
}

/// A record that was not aggregated / not charged, with the reason.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SkipNote {
    /// Why it was skipped.
    pub reason: String,
    /// The tenant involved, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<Uuid>,
}

/// The pure aggregation output.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct RunnerAggregateOutput {
    /// Counter rows to UPSERT into `runner_usage_counter`.
    pub counters: Vec<CounterRow>,
    /// Chain-head updates to write into `runner_hash_chain_head`.
    pub chain_head_updates: Vec<ChainHeadUpdate>,
    /// Per-tenant shadow charges (bills nothing).
    pub shadow_ledger: Vec<ShadowLine>,
    /// Records skipped (unknown tenant tier, etc.).
    pub skipped: Vec<SkipNote>,
    /// Total shadow charge across all tenants this period, in millicents.
    pub total_shadow_millicents: u128,
}

/// Per-`(tenant, region)` accumulator built while scanning the staged events.
#[derive(Default)]
struct GroupAccum {
    vcpu_seconds: u128,
    event_count: u64,
    idem_keys: Vec<IdemKey>,
}

fn parse_hex32(s: &str) -> Option<[u8; 32]> {
    let bytes = hex::decode(s).ok()?;
    <[u8; 32]>::try_from(bytes.as_slice()).ok()
}

/// Aggregate staged runner usage into per-`(tenant, region)` counters (chained
/// per region) + per-tenant shadow charges. Pure — no I/O.
///
/// # Errors
///
/// - [`RunnerAggregateError::MalformedEvent`] on a bad idem_key hex.
/// - [`RunnerAggregateError::MalformedChainHead`] on a bad prior head hex.
/// - [`RunnerAggregateError::ChainBreak`] if the chain builder rejects an append.
pub fn aggregate_runner_usage(
    input: &RunnerAggregateInput,
) -> Result<RunnerAggregateOutput, RunnerAggregateError> {
    let rate = input
        .rate_cents_per_vcpu_hour
        .unwrap_or(DEFAULT_OVERAGE_RATE_CENTS_PER_VCPU_HOUR);

    // 1. Group staged events by region → tenant (BTreeMap = deterministic walk).
    let mut by_region: BTreeMap<String, BTreeMap<Uuid, GroupAccum>> = BTreeMap::new();
    for ev in &input.staged {
        let key = parse_hex32(&ev.idem_key_hex).ok_or_else(|| {
            RunnerAggregateError::MalformedEvent(format!(
                "idem_key_hex not 64-hex for tenant {} region {}",
                ev.tenant_id, ev.region
            ))
        })?;
        let acc = by_region
            .entry(ev.region.clone())
            .or_default()
            .entry(ev.tenant_id)
            .or_default();
        acc.vcpu_seconds = acc.vcpu_seconds.saturating_add(ev.qty_vcpu_seconds);
        acc.event_count = acc.event_count.saturating_add(1);
        acc.idem_keys.push(IdemKey(key));
    }

    let mut counters: Vec<CounterRow> = Vec::new();
    let mut chain_head_updates: Vec<ChainHeadUpdate> = Vec::new();
    // Per-tenant total vCPU-seconds across regions, for the shadow charge.
    let mut tenant_totals: BTreeMap<Uuid, u128> = BTreeMap::new();

    // 2. Per region: resume the chain, append each tenant's aggregate.
    for (region, tenants) in &by_region {
        let mut builder = match input.prior_chain_heads.get(region) {
            Some(prior) => {
                let head = parse_hex32(&prior.current_head_hex).ok_or_else(|| {
                    RunnerAggregateError::MalformedChainHead {
                        region: region.clone(),
                        detail: "current_head_hex not 64-hex".to_string(),
                    }
                })?;
                HashChainBuilder::resume(ChainHash(head), prior.next_sequence)
            }
            None => HashChainBuilder::new(),
        };

        for (tenant_id, acc) in tenants {
            let mut idem_keys = acc.idem_keys.clone();
            idem_keys.sort_by(|a, b| a.0.cmp(&b.0));

            let prev_hash = *builder.head();
            let seq = builder.next_sequence();
            let agg_id = Uuid::new_v5(
                &AGG_ID_NAMESPACE,
                format!("{tenant_id}|{region}|{}", input.billing_period).as_bytes(),
            );
            let aggregate = AggregatedCounter::new(
                RUNNER_AGG_SOURCE,
                agg_id,
                input.now_ms,
                seq,
                prev_hash,
                *tenant_id,
                input.billing_period.clone(),
                UsageEventKind::RunnerVcpuSeconds,
                acc.vcpu_seconds,
                acc.event_count,
                input.period_start_ms,
                input.period_end_ms,
                idem_keys,
            );
            let own = builder
                .append(&aggregate)
                .map_err(|e| RunnerAggregateError::ChainBreak {
                    region: region.clone(),
                    detail: e.to_string(),
                })?;

            counters.push(CounterRow {
                tenant_id: *tenant_id,
                region: region.clone(),
                billing_period: input.billing_period.clone(),
                vcpu_seconds: acc.vcpu_seconds,
                event_count: acc.event_count,
                aggregated_at_ms: input.now_ms,
                prev_hash_hex: prev_hash.to_hex(),
                own_digest_hex: own.to_hex(),
            });

            let t = tenant_totals.entry(*tenant_id).or_default();
            *t = t.saturating_add(acc.vcpu_seconds);
        }

        chain_head_updates.push(ChainHeadUpdate {
            region: region.clone(),
            current_head_hex: builder.head().to_hex(),
            next_sequence: builder.next_sequence(),
            last_aggregated_period: input.billing_period.clone(),
        });
    }

    // 3. Per-tenant shadow charge (allowance is per-tenant across all regions).
    let mut shadow_ledger: Vec<ShadowLine> = Vec::new();
    let mut skipped: Vec<SkipNote> = Vec::new();
    let mut total_shadow_millicents: u128 = 0;

    for (tenant_id, total) in &tenant_totals {
        let Some(sku) = input.tenant_tiers.get(tenant_id) else {
            skipped.push(SkipNote {
                reason: "no runner tier for tenant (absent from tenant_tiers)".to_string(),
                tenant_id: Some(*tenant_id),
            });
            continue;
        };
        let Some(tier) = RunnerTier::from_sku(sku) else {
            skipped.push(SkipNote {
                reason: format!("unknown runner tier sku '{sku}'"),
                tenant_id: Some(*tenant_id),
            });
            continue;
        };

        let over = overage_vcpu_seconds(*total, tier);
        let millicents = shadow_charge_millicents(over, rate);
        total_shadow_millicents = total_shadow_millicents.saturating_add(millicents);

        shadow_ledger.push(ShadowLine {
            tenant_id: *tenant_id,
            billing_period: input.billing_period.clone(),
            tier_sku: sku.clone(),
            total_vcpu_seconds: *total,
            allowance_vcpu_seconds: u128::from(tier.allowance_vcpu_seconds()),
            overage_vcpu_seconds: over,
            overage_vcpu_hours: overage_vcpu_hours_decimal(over),
            rate_cents_per_vcpu_hour: rate,
            shadow_charge_millicents: millicents,
            shadow_charge_cents: millicents_to_cents(millicents),
        });
    }

    Ok(RunnerAggregateOutput {
        counters,
        chain_head_updates,
        shadow_ledger,
        skipped,
        total_shadow_millicents,
    })
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives; the indices are constructed deterministically from fixtures with known lengths."
)]
mod tests {
    use super::*;

    fn ik(byte: u8) -> String {
        hex::encode([byte; 32])
    }

    fn tenant(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn base_input(staged: Vec<StagedRunnerEvent>) -> RunnerAggregateInput {
        RunnerAggregateInput {
            billing_period: "2026-08".to_string(),
            period_start_ms: 1_000,
            period_end_ms: 2_000,
            now_ms: 1_500,
            rate_cents_per_vcpu_hour: None,
            staged,
            tenant_tiers: BTreeMap::new(),
            prior_chain_heads: BTreeMap::new(),
        }
    }

    #[test]
    fn empty_input_yields_empty_output() {
        let out = aggregate_runner_usage(&base_input(vec![])).unwrap();
        assert!(out.counters.is_empty());
        assert!(out.shadow_ledger.is_empty());
        assert_eq!(out.total_shadow_millicents, 0);
    }

    #[test]
    fn sums_qty_per_tenant_region_and_counts_events() {
        let t = tenant(1);
        let mut input = base_input(vec![
            StagedRunnerEvent {
                tenant_id: t,
                region: "iad".to_string(),
                qty_vcpu_seconds: 100 * 3600,
                idem_key_hex: ik(0x01),
                time_ms: 1,
            },
            StagedRunnerEvent {
                tenant_id: t,
                region: "iad".to_string(),
                qty_vcpu_seconds: 200 * 3600, // 300 vCPU-h total in iad
                idem_key_hex: ik(0x02),
                time_ms: 2,
            },
        ]);
        input.tenant_tiers.insert(t, "runner_starter".to_string()); // 100h allowance
        let out = aggregate_runner_usage(&input).unwrap();

        assert_eq!(out.counters.len(), 1);
        assert_eq!(out.counters[0].vcpu_seconds, 300 * 3600);
        assert_eq!(out.counters[0].event_count, 2);

        // 300h used, 100h allowance → 200h over × $0.20 = $40.00 = 4_000_000 mc.
        assert_eq!(out.shadow_ledger.len(), 1);
        let line = &out.shadow_ledger[0];
        assert_eq!(line.overage_vcpu_seconds, 200 * 3600);
        assert_eq!(line.shadow_charge_millicents, 4_000_000);
        assert_eq!(line.shadow_charge_cents, 4000);
        assert_eq!(out.total_shadow_millicents, 4_000_000);
    }

    #[test]
    fn allowance_spans_regions_for_one_tenant() {
        let t = tenant(7);
        let mut input = base_input(vec![
            StagedRunnerEvent {
                tenant_id: t,
                region: "iad".to_string(),
                qty_vcpu_seconds: 60 * 3600,
                idem_key_hex: ik(0x0a),
                time_ms: 1,
            },
            StagedRunnerEvent {
                tenant_id: t,
                region: "fra".to_string(),
                qty_vcpu_seconds: 60 * 3600, // 120h total across 2 regions
                idem_key_hex: ik(0x0b),
                time_ms: 2,
            },
        ]);
        input.tenant_tiers.insert(t, "runner_starter".to_string()); // 100h allowance
        let out = aggregate_runner_usage(&input).unwrap();

        // Two counter rows (one per region), but ONE shadow line summing both.
        assert_eq!(out.counters.len(), 2);
        assert_eq!(out.shadow_ledger.len(), 1);
        assert_eq!(out.shadow_ledger[0].total_vcpu_seconds, 120 * 3600);
        // 120h - 100h = 20h over × $0.20 = $4.00.
        assert_eq!(out.shadow_ledger[0].shadow_charge_cents, 400);
    }

    #[test]
    fn under_allowance_bills_zero() {
        let t = tenant(3);
        let mut input = base_input(vec![StagedRunnerEvent {
            tenant_id: t,
            region: "iad".to_string(),
            qty_vcpu_seconds: 50 * 3600,
            idem_key_hex: ik(0x03),
            time_ms: 1,
        }]);
        input.tenant_tiers.insert(t, "runner_pro".to_string()); // 240h allowance
        let out = aggregate_runner_usage(&input).unwrap();
        assert_eq!(out.shadow_ledger[0].overage_vcpu_seconds, 0);
        assert_eq!(out.shadow_ledger[0].shadow_charge_millicents, 0);
    }

    #[test]
    fn unknown_tier_is_skipped_not_charged() {
        let t = tenant(9);
        let input = base_input(vec![StagedRunnerEvent {
            tenant_id: t,
            region: "iad".to_string(),
            qty_vcpu_seconds: 500 * 3600,
            idem_key_hex: ik(0x09),
            time_ms: 1,
        }]);
        // no tenant_tiers entry
        let out = aggregate_runner_usage(&input).unwrap();
        assert!(out.shadow_ledger.is_empty());
        assert_eq!(out.skipped.len(), 1);
        assert_eq!(out.skipped[0].tenant_id, Some(t));
        // The counter row is still produced (usage is recorded even if unpriced).
        assert_eq!(out.counters.len(), 1);
    }

    #[test]
    fn chain_links_and_is_deterministic() {
        let t1 = tenant(1);
        let t2 = tenant(2);
        let input = base_input(vec![
            StagedRunnerEvent {
                tenant_id: t1,
                region: "iad".to_string(),
                qty_vcpu_seconds: 3600,
                idem_key_hex: ik(0x11),
                time_ms: 1,
            },
            StagedRunnerEvent {
                tenant_id: t2,
                region: "iad".to_string(),
                qty_vcpu_seconds: 7200,
                idem_key_hex: ik(0x22),
                time_ms: 2,
            },
        ]);
        let a = aggregate_runner_usage(&input).unwrap();
        let b = aggregate_runner_usage(&input).unwrap();
        // Deterministic: identical input → identical output (incl. chain digests).
        assert_eq!(a, b);
        // The second aggregate's prev_hash is the first's own_digest (linked).
        assert_eq!(a.counters.len(), 2);
        assert_eq!(a.counters[1].prev_hash_hex, a.counters[0].own_digest_hex);
        // Genesis prev_hash for the first append.
        assert_eq!(a.counters[0].prev_hash_hex, ChainHash::genesis().to_hex());
        // One chain-head update for the single region.
        assert_eq!(a.chain_head_updates.len(), 1);
        assert_eq!(
            a.chain_head_updates[0].current_head_hex,
            a.counters[1].own_digest_hex
        );
        assert_eq!(a.chain_head_updates[0].next_sequence, 2);
    }

    #[test]
    fn resume_continues_the_chain_from_prior_head() {
        let t1 = tenant(1);
        // First run at genesis.
        let first = aggregate_runner_usage(&base_input(vec![StagedRunnerEvent {
            tenant_id: t1,
            region: "iad".to_string(),
            qty_vcpu_seconds: 3600,
            idem_key_hex: ik(0x11),
            time_ms: 1,
        }]))
        .unwrap();
        let head = &first.chain_head_updates[0];

        // Second run resumes from the first's head with a new tenant.
        let mut input = base_input(vec![StagedRunnerEvent {
            tenant_id: tenant(2),
            region: "iad".to_string(),
            qty_vcpu_seconds: 3600,
            idem_key_hex: ik(0x22),
            time_ms: 3,
        }]);
        input.prior_chain_heads.insert(
            "iad".to_string(),
            PriorChainHead {
                current_head_hex: head.current_head_hex.clone(),
                next_sequence: head.next_sequence,
            },
        );
        let second = aggregate_runner_usage(&input).unwrap();
        // The resumed aggregate's prev_hash is the first run's head, seq continues.
        assert_eq!(second.counters[0].prev_hash_hex, head.current_head_hex);
        assert_eq!(second.chain_head_updates[0].next_sequence, 2);
    }

    #[test]
    fn malformed_idem_key_errors() {
        let input = base_input(vec![StagedRunnerEvent {
            tenant_id: tenant(1),
            region: "iad".to_string(),
            qty_vcpu_seconds: 3600,
            idem_key_hex: "not-hex".to_string(),
            time_ms: 1,
        }]);
        assert!(matches!(
            aggregate_runner_usage(&input),
            Err(RunnerAggregateError::MalformedEvent(_))
        ));
    }
}
