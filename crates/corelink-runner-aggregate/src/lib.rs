//! `corelink-runner-aggregate` — the runner compute-overage aggregation core
//! for WI-S10-007 Phase B (shadow).
//!
//! Given the staged `runner_vcpu_seconds` usage events for one billing period
//! (drained from `usage_event_staging` by the credentialed cron workflow), the
//! immutable per-tenant period terms snapshots, and the per-region prior hash-chain heads, this
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

use std::collections::{BTreeMap, BTreeSet};

use corelink_billing_aggregator::{AggregatedCounter, ChainHash, HashChainBuilder};
use corelink_billing_emit::event::{IdemKey, UsageEventKind};
use corelink_runner_overage::{
    millicents_to_cents, overage_vcpu_hours_decimal, SECONDS_PER_VCPU_HOUR,
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

/// JSON artifact contract for `runner-aggregate-run`.
/// Version 3 requires immutable terms; v2 artifacts fail closed because their mutable inputs cannot safely price late usage.
pub const RUNNER_AGGREGATE_ARTIFACT_CONTRACT_VERSION: u32 = 3;

/// Product discriminator for the runner compute meter.
pub const RUNNER_VCPU_SECONDS_PRODUCT: &str = "runner_vcpu_seconds";

/// Aggregation error surface.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RunnerAggregateError {
    /// The input JSON uses a contract version this binary does not understand.
    #[error(
        "unsupported runner aggregate artifact contract version {found} (supported: {supported})"
    )]
    UnsupportedArtifactContractVersion {
        /// Contract version supplied by the caller.
        found: u32,
        /// Contract version supported by this binary.
        supported: u32,
    },
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
    /// Durable prior consumption did not belong to this runner product / period.
    #[error("invalid prior consumption for tenant {tenant_id}: {detail}")]
    InvalidPriorConsumption {
        /// Tenant whose durable state was invalid.
        tenant_id: Uuid,
        /// Validation failure detail.
        detail: String,
    },
    /// A tenant-period terms snapshot was absent, malformed, or did not bind
    /// to the tenant, billing period, or durable prior consumption.
    #[error("invalid tenant-period terms for tenant {tenant_id}: {detail}")]
    InvalidTenantPeriodTerms {
        /// Tenant whose immutable terms did not validate.
        tenant_id: Uuid,
        /// Validation failure detail.
        detail: String,
    },
    /// Integer-only usage or money arithmetic overflowed and was rejected.
    #[error("arithmetic overflow while {operation} for tenant {tenant_id}")]
    ArithmeticOverflow {
        /// Tenant whose calculation overflowed.
        tenant_id: Uuid,
        /// Checked operation that overflowed.
        operation: &'static str,
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

/// Durable cumulative runner usage for one `(tenant, product, billing_period)`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PriorTenantConsumption {
    /// Product whose consumption is represented; must be `runner_vcpu_seconds`.
    pub product: String,
    /// Billing period owning the durable consumption, `YYYY-MM`.
    pub billing_period: String,
    /// Consumption already durably recorded for this tenant/product/period.
    pub cumulative_vcpu_seconds: u128,
    /// Immutable terms snapshot reference used to price this durable total.
    pub terms_snapshot_ref: String,
    /// BLAKE3-256 digest of the immutable terms snapshot, lowercase 64-hex.
    pub terms_snapshot_digest_hex: String,
}

/// Immutable pricing terms for one `(tenant_id, billing_period)` snapshot.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TenantPeriodTerms {
    /// Tenant this snapshot belongs to; must equal its input map key.
    pub tenant_id: Uuid,
    /// Billing period this snapshot belongs to; must equal the input period.
    pub billing_period: String,
    /// Included vCPU-seconds for this tenant and period.
    pub allowance_vcpu_seconds: u128,
    /// Whole US cents per vCPU-hour for this tenant and period.
    pub rate_cents_per_vcpu_hour: u64,
    /// Immutable snapshot identifier in the upstream terms store.
    pub terms_snapshot_ref: String,
    /// BLAKE3-256 of the canonical fields in this record, lowercase 64-hex.
    pub terms_snapshot_digest_hex: String,
}

impl TenantPeriodTerms {
    /// Return the canonical BLAKE3-256 digest that this snapshot must carry.
    #[must_use]
    pub fn expected_snapshot_digest_hex(&self) -> String {
        let mut canonical =
            Vec::with_capacity(64 + self.billing_period.len() + self.terms_snapshot_ref.len());
        canonical.extend_from_slice(b"corelink.runner.tenant-period-terms.v1\\0");
        canonical.extend_from_slice(self.tenant_id.as_bytes());
        canonical.extend_from_slice(&(self.billing_period.len() as u64).to_be_bytes());
        canonical.extend_from_slice(self.billing_period.as_bytes());
        canonical.extend_from_slice(&self.allowance_vcpu_seconds.to_be_bytes());
        canonical.extend_from_slice(&self.rate_cents_per_vcpu_hour.to_be_bytes());
        canonical.extend_from_slice(&(self.terms_snapshot_ref.len() as u64).to_be_bytes());
        canonical.extend_from_slice(self.terms_snapshot_ref.as_bytes());
        blake3::hash(&canonical).to_hex().to_string()
    }
}

/// The pure aggregation input (one billing period).
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerAggregateInput {
    /// Required JSON artifact contract version.
    pub artifact_contract_version: u32,
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
    /// Durable consumption before this deduplicated batch, keyed by tenant.
    pub prior_consumption: BTreeMap<Uuid, PriorTenantConsumption>,
    /// The staged runner events to aggregate (the workflow feeds only
    /// not-yet-aggregated rows — the watermark).
    pub staged: Vec<StagedRunnerEvent>,
    /// Required immutable terms, keyed by tenant. Every staged tenant must
    /// have exactly one snapshot matching this input's billing period.
    pub tenant_period_terms: BTreeMap<Uuid, TenantPeriodTerms>,
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
    /// Legacy alias for [`Self::batch_vcpu_seconds`].
    pub total_vcpu_seconds: u128,
    /// Durable vCPU-seconds before this new batch (`C0`).
    pub prior_vcpu_seconds: u128,
    /// New deduplicated vCPU-seconds in this aggregate call (`Δ`).
    pub batch_vcpu_seconds: u128,
    /// Durable prior plus this new batch (`C1 = C0 + Δ`).
    pub cumulative_vcpu_seconds: u128,
    /// The tier's included allowance, in vCPU-seconds.
    pub allowance_vcpu_seconds: u128,
    /// Overage above the allowance, in vCPU-seconds.
    pub overage_vcpu_seconds: u128,
    /// Overage as the Stripe meter quantity, vCPU-hours (exact decimal string).
    pub overage_vcpu_hours: String,
    /// The rate applied, whole cents per vCPU-hour.
    pub rate_cents_per_vcpu_hour: u64,
    /// Immutable terms snapshot reference used for this line.
    pub terms_snapshot_ref: String,
    /// Immutable terms snapshot digest used for this line.
    pub terms_snapshot_digest_hex: String,
    /// Legacy alias for [`Self::charge_delta_millicents`].
    pub shadow_charge_millicents: u128,
    /// Display-only whole cents for [`Self::charge_delta_millicents`].
    pub shadow_charge_cents: u128,
    /// `F(C1) - F(C0)`, in canonical millicents. Never derived from display cents.
    pub charge_delta_millicents: u128,
    /// `F(C1)`, in canonical millicents, for shadow reconciliation only.
    pub cumulative_shadow_charge_millicents: u128,
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
    /// JSON artifact contract version emitted by this binary.
    pub artifact_contract_version: u32,
    /// Counter rows to UPSERT into `runner_usage_counter`.
    pub counters: Vec<CounterRow>,
    /// Chain-head updates to write into `runner_hash_chain_head`.
    pub chain_head_updates: Vec<ChainHeadUpdate>,
    /// Per-tenant shadow deltas for this invocation (bills nothing).
    /// Each line also carries the cumulative-period snapshot separately.
    pub shadow_ledger: Vec<ShadowLine>,
    /// Reserved for explicit non-pricing notes. Missing or invalid terms fail
    /// the entire artifact and are never silently skipped.
    pub skipped: Vec<SkipNote>,
    /// Sum of invocation charge deltas, in millicents; never a monthly snapshot.
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

fn is_canonical_hex32(s: &str) -> bool {
    parse_hex32(s).is_some_and(|digest| hex::encode(digest) == s)
}

fn validate_terms(
    terms: &TenantPeriodTerms,
    tenant_id: Uuid,
    billing_period: &str,
) -> Result<(), RunnerAggregateError> {
    if terms.tenant_id != tenant_id {
        return Err(RunnerAggregateError::InvalidTenantPeriodTerms {
            tenant_id,
            detail: format!("snapshot tenant {} does not match map key", terms.tenant_id),
        });
    }
    if terms.billing_period != billing_period {
        return Err(RunnerAggregateError::InvalidTenantPeriodTerms {
            tenant_id,
            detail: format!(
                "snapshot billing_period {:?} does not match input {:?}",
                terms.billing_period, billing_period
            ),
        });
    }
    if terms.terms_snapshot_ref.is_empty() {
        return Err(RunnerAggregateError::InvalidTenantPeriodTerms {
            tenant_id,
            detail: "snapshot reference is empty".to_string(),
        });
    }
    if !is_canonical_hex32(&terms.terms_snapshot_digest_hex)
        || terms.terms_snapshot_digest_hex != terms.expected_snapshot_digest_hex()
    {
        return Err(RunnerAggregateError::InvalidTenantPeriodTerms {
            tenant_id,
            detail: "snapshot digest does not bind the canonical terms".to_string(),
        });
    }
    Ok(())
}

fn cumulative_shadow_charge_millicents_checked(
    cumulative_vcpu_seconds: u128,
    allowance_vcpu_seconds: u128,
    rate_cents_per_vcpu_hour: u64,
    tenant_id: Uuid,
) -> Result<u128, RunnerAggregateError> {
    cumulative_vcpu_seconds
        .saturating_sub(allowance_vcpu_seconds)
        .checked_mul(u128::from(rate_cents_per_vcpu_hour))
        .and_then(|value| value.checked_mul(1000))
        .map(|numerator| numerator / u128::from(SECONDS_PER_VCPU_HOUR))
        .ok_or(RunnerAggregateError::ArithmeticOverflow {
            tenant_id,
            operation: "evaluating cumulative shadow charge",
        })
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
    if input.artifact_contract_version != RUNNER_AGGREGATE_ARTIFACT_CONTRACT_VERSION {
        return Err(RunnerAggregateError::UnsupportedArtifactContractVersion {
            found: input.artifact_contract_version,
            supported: RUNNER_AGGREGATE_ARTIFACT_CONTRACT_VERSION,
        });
    }

    for (tenant_id, terms) in &input.tenant_period_terms {
        validate_terms(terms, *tenant_id, &input.billing_period)?;
    }

    for (tenant_id, prior) in &input.prior_consumption {
        if prior.product != RUNNER_VCPU_SECONDS_PRODUCT {
            return Err(RunnerAggregateError::InvalidPriorConsumption {
                tenant_id: *tenant_id,
                detail: format!(
                    "product {:?} is not {RUNNER_VCPU_SECONDS_PRODUCT:?}",
                    prior.product
                ),
            });
        }
        if prior.billing_period != input.billing_period {
            return Err(RunnerAggregateError::InvalidPriorConsumption {
                tenant_id: *tenant_id,
                detail: format!(
                    "billing_period {:?} does not match input {:?}",
                    prior.billing_period, input.billing_period
                ),
            });
        }
        if prior.terms_snapshot_ref.is_empty()
            || !is_canonical_hex32(&prior.terms_snapshot_digest_hex)
        {
            return Err(RunnerAggregateError::InvalidTenantPeriodTerms {
                tenant_id: *tenant_id,
                detail: "prior consumption has an invalid terms snapshot binding".to_string(),
            });
        }
        let terms = input.tenant_period_terms.get(tenant_id).ok_or(
            RunnerAggregateError::InvalidTenantPeriodTerms {
                tenant_id: *tenant_id,
                detail: "missing terms snapshot for prior consumption".to_string(),
            },
        )?;
        if prior.terms_snapshot_ref != terms.terms_snapshot_ref
            || prior.terms_snapshot_digest_hex != terms.terms_snapshot_digest_hex
        {
            return Err(RunnerAggregateError::InvalidTenantPeriodTerms {
                tenant_id: *tenant_id,
                detail: "prior consumption terms snapshot does not match input".to_string(),
            });
        }
    }

    for event in &input.staged {
        if !input.tenant_period_terms.contains_key(&event.tenant_id) {
            return Err(RunnerAggregateError::InvalidTenantPeriodTerms {
                tenant_id: event.tenant_id,
                detail: "missing terms snapshot for staged usage".to_string(),
            });
        }
    }

    // 1. Group staged events by region → tenant (BTreeMap = deterministic walk).
    let mut by_region: BTreeMap<String, BTreeMap<Uuid, GroupAccum>> = BTreeMap::new();
    let mut deduped_idem_keys: BTreeSet<[u8; 32]> = BTreeSet::new();
    for ev in &input.staged {
        let key = parse_hex32(&ev.idem_key_hex).ok_or_else(|| {
            RunnerAggregateError::MalformedEvent(format!(
                "idem_key_hex not 64-hex for tenant {} region {}",
                ev.tenant_id, ev.region
            ))
        })?;
        // `Δ` is a deduplicated batch. An idempotency key identifies one usage
        // event globally, so a replay cannot change either the counter or charge.
        if !deduped_idem_keys.insert(key) {
            continue;
        }
        let acc = by_region
            .entry(ev.region.clone())
            .or_default()
            .entry(ev.tenant_id)
            .or_default();
        acc.vcpu_seconds = acc.vcpu_seconds.checked_add(ev.qty_vcpu_seconds).ok_or(
            RunnerAggregateError::ArithmeticOverflow {
                tenant_id: ev.tenant_id,
                operation: "summing deduplicated batch usage",
            },
        )?;
        acc.event_count =
            acc.event_count
                .checked_add(1)
                .ok_or(RunnerAggregateError::ArithmeticOverflow {
                    tenant_id: ev.tenant_id,
                    operation: "counting deduplicated batch events",
                })?;
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
            *t = t.checked_add(acc.vcpu_seconds).ok_or(
                RunnerAggregateError::ArithmeticOverflow {
                    tenant_id: *tenant_id,
                    operation: "summing a tenant batch across regions",
                },
            )?;
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
    let skipped: Vec<SkipNote> = Vec::new();
    let mut total_shadow_millicents: u128 = 0;

    for (tenant_id, total) in &tenant_totals {
        let terms = input.tenant_period_terms.get(tenant_id).ok_or(
            RunnerAggregateError::InvalidTenantPeriodTerms {
                tenant_id: *tenant_id,
                detail: "missing terms snapshot for aggregated usage".to_string(),
            },
        )?;

        let prior = input
            .prior_consumption
            .get(tenant_id)
            .map_or(0, |state| state.cumulative_vcpu_seconds);
        let cumulative =
            prior
                .checked_add(*total)
                .ok_or(RunnerAggregateError::ArithmeticOverflow {
                    tenant_id: *tenant_id,
                    operation: "adding durable prior consumption and batch usage",
                })?;
        let allowance = terms.allowance_vcpu_seconds;
        let prior_charge = cumulative_shadow_charge_millicents_checked(
            prior,
            allowance,
            terms.rate_cents_per_vcpu_hour,
            *tenant_id,
        )?;
        let cumulative_charge = cumulative_shadow_charge_millicents_checked(
            cumulative,
            allowance,
            terms.rate_cents_per_vcpu_hour,
            *tenant_id,
        )?;
        let charge_delta = cumulative_charge.checked_sub(prior_charge).ok_or(
            RunnerAggregateError::InvalidPriorConsumption {
                tenant_id: *tenant_id,
                detail: "cumulative shadow charge decreased".to_string(),
            },
        )?;
        total_shadow_millicents = total_shadow_millicents.checked_add(charge_delta).ok_or(
            RunnerAggregateError::ArithmeticOverflow {
                tenant_id: *tenant_id,
                operation: "summing shadow charge deltas",
            },
        )?;
        let over = cumulative.saturating_sub(allowance);

        shadow_ledger.push(ShadowLine {
            tenant_id: *tenant_id,
            billing_period: input.billing_period.clone(),
            total_vcpu_seconds: *total,
            prior_vcpu_seconds: prior,
            batch_vcpu_seconds: *total,
            cumulative_vcpu_seconds: cumulative,
            allowance_vcpu_seconds: allowance,
            overage_vcpu_seconds: over,
            overage_vcpu_hours: overage_vcpu_hours_decimal(over),
            rate_cents_per_vcpu_hour: terms.rate_cents_per_vcpu_hour,
            terms_snapshot_ref: terms.terms_snapshot_ref.clone(),
            terms_snapshot_digest_hex: terms.terms_snapshot_digest_hex.clone(),
            shadow_charge_millicents: charge_delta,
            shadow_charge_cents: millicents_to_cents(charge_delta),
            charge_delta_millicents: charge_delta,
            cumulative_shadow_charge_millicents: cumulative_charge,
        });
    }

    Ok(RunnerAggregateOutput {
        artifact_contract_version: RUNNER_AGGREGATE_ARTIFACT_CONTRACT_VERSION,
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

    fn terms_for(
        tenant_id: Uuid,
        allowance_vcpu_seconds: u128,
        rate_cents_per_vcpu_hour: u64,
    ) -> TenantPeriodTerms {
        let mut terms = TenantPeriodTerms {
            tenant_id,
            billing_period: "2026-08".to_string(),
            allowance_vcpu_seconds,
            rate_cents_per_vcpu_hour,
            terms_snapshot_ref: format!("terms://{tenant_id}/2026-08/v1"),
            terms_snapshot_digest_hex: String::new(),
        };
        terms.terms_snapshot_digest_hex = terms.expected_snapshot_digest_hex();
        terms
    }

    fn base_input(staged: Vec<StagedRunnerEvent>) -> RunnerAggregateInput {
        let tenant_period_terms = staged
            .iter()
            .map(|event| (event.tenant_id, terms_for(event.tenant_id, 100 * 3600, 20)))
            .collect();
        RunnerAggregateInput {
            artifact_contract_version: RUNNER_AGGREGATE_ARTIFACT_CONTRACT_VERSION,
            billing_period: "2026-08".to_string(),
            period_start_ms: 1_000,
            period_end_ms: 2_000,
            now_ms: 1_500,
            prior_consumption: BTreeMap::new(),
            staged,
            tenant_period_terms,
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
        let input = base_input(vec![
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
        assert_eq!(line.prior_vcpu_seconds, 0);
        assert_eq!(line.batch_vcpu_seconds, 300 * 3600);
        assert_eq!(line.cumulative_vcpu_seconds, 300 * 3600);
        assert_eq!(line.charge_delta_millicents, 4_000_000);
        assert_eq!(line.cumulative_shadow_charge_millicents, 4_000_000);
        assert_eq!(out.total_shadow_millicents, 4_000_000);
    }

    #[test]
    fn allowance_spans_regions_for_one_tenant() {
        let t = tenant(7);
        let input = base_input(vec![
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
        input
            .tenant_period_terms
            .insert(t, terms_for(t, 240 * 3600, 20));
        let out = aggregate_runner_usage(&input).unwrap();
        assert_eq!(out.shadow_ledger[0].overage_vcpu_seconds, 0);
        assert_eq!(out.shadow_ledger[0].shadow_charge_millicents, 0);
    }

    #[test]
    fn each_tenant_uses_its_own_allowance_and_rate() {
        let t1 = tenant(31);
        let t2 = tenant(32);
        let mut input = base_input(vec![
            StagedRunnerEvent {
                tenant_id: t1,
                region: "iad".to_string(),
                qty_vcpu_seconds: 200 * 3600,
                idem_key_hex: ik(0x31),
                time_ms: 1,
            },
            StagedRunnerEvent {
                tenant_id: t2,
                region: "fra".to_string(),
                qty_vcpu_seconds: 200 * 3600,
                idem_key_hex: ik(0x32),
                time_ms: 2,
            },
        ]);
        input
            .tenant_period_terms
            .insert(t1, terms_for(t1, 100 * 3600, 10));
        input
            .tenant_period_terms
            .insert(t2, terms_for(t2, 180 * 3600, 40));
        let out = aggregate_runner_usage(&input).unwrap();

        assert_eq!(out.shadow_ledger[0].charge_delta_millicents, 1_000_000);
        assert_eq!(out.shadow_ledger[1].charge_delta_millicents, 800_000);
    }

    #[test]
    fn missing_terms_fail_closed_before_aggregate_output() {
        let t = tenant(9);
        let mut input = base_input(vec![StagedRunnerEvent {
            tenant_id: t,
            region: "iad".to_string(),
            qty_vcpu_seconds: 500 * 3600,
            idem_key_hex: ik(0x09),
            time_ms: 1,
        }]);
        input.tenant_period_terms.clear();
        assert!(matches!(
            aggregate_runner_usage(&input),
            Err(RunnerAggregateError::InvalidTenantPeriodTerms { .. })
        ));
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

    fn prior_consumption(terms: &TenantPeriodTerms, vcpu_seconds: u128) -> PriorTenantConsumption {
        PriorTenantConsumption {
            product: RUNNER_VCPU_SECONDS_PRODUCT.to_string(),
            billing_period: "2026-08".to_string(),
            cumulative_vcpu_seconds: vcpu_seconds,
            terms_snapshot_ref: terms.terms_snapshot_ref.clone(),
            terms_snapshot_digest_hex: terms.terms_snapshot_digest_hex.clone(),
        }
    }

    fn starter_input_with_prior(
        prior_vcpu_seconds: u128,
        staged: Vec<StagedRunnerEvent>,
    ) -> RunnerAggregateInput {
        let t = tenant(42);
        let mut input = base_input(staged);
        let terms = terms_for(t, 100 * 3600, 20);
        input.tenant_period_terms.insert(t, terms.clone());
        input
            .prior_consumption
            .insert(t, prior_consumption(&terms, prior_vcpu_seconds));
        input
    }

    #[test]
    fn charge_delta_is_partition_invariant_across_the_allowance_boundary() {
        let t = tenant(42);
        let one = aggregate_runner_usage(&starter_input_with_prior(
            0,
            vec![StagedRunnerEvent {
                tenant_id: t,
                region: "iad".to_string(),
                qty_vcpu_seconds: 120 * 3600,
                idem_key_hex: ik(0x42),
                time_ms: 1,
            }],
        ))
        .unwrap();

        let first = aggregate_runner_usage(&starter_input_with_prior(
            0,
            vec![StagedRunnerEvent {
                tenant_id: t,
                region: "iad".to_string(),
                qty_vcpu_seconds: 60 * 3600,
                idem_key_hex: ik(0x43),
                time_ms: 1,
            }],
        ))
        .unwrap();
        let second = aggregate_runner_usage(&starter_input_with_prior(
            first.shadow_ledger[0].cumulative_vcpu_seconds,
            vec![StagedRunnerEvent {
                tenant_id: t,
                region: "iad".to_string(),
                qty_vcpu_seconds: 60 * 3600,
                idem_key_hex: ik(0x44),
                time_ms: 2,
            }],
        ))
        .unwrap();

        assert_eq!(one.shadow_ledger[0].charge_delta_millicents, 400_000);
        assert_eq!(
            one.shadow_ledger[0].charge_delta_millicents,
            first.shadow_ledger[0].charge_delta_millicents
                + second.shadow_ledger[0].charge_delta_millicents
        );
    }

    #[test]
    fn allowance_boundary_is_applied_once_to_cumulative_usage() {
        let t = tenant(42);
        let allowance = 100 * 3600;
        for (prior, batch, expected_delta) in
            [(allowance - 1, 1, 0), (allowance, 0, 0), (allowance, 1, 5)]
        {
            let out = aggregate_runner_usage(&starter_input_with_prior(
                prior,
                vec![StagedRunnerEvent {
                    tenant_id: t,
                    region: "iad".to_string(),
                    qty_vcpu_seconds: batch,
                    idem_key_hex: ik(batch as u8),
                    time_ms: 1,
                }],
            ))
            .unwrap();
            let line = &out.shadow_ledger[0];
            assert_eq!(line.cumulative_vcpu_seconds, prior + batch);
            assert_eq!(line.charge_delta_millicents, expected_delta);
        }
    }

    #[test]
    fn multi_region_batches_converge_with_one_region_batch() {
        let t = tenant(42);
        let one_region = aggregate_runner_usage(&starter_input_with_prior(
            0,
            vec![StagedRunnerEvent {
                tenant_id: t,
                region: "iad".to_string(),
                qty_vcpu_seconds: 120 * 3600,
                idem_key_hex: ik(0x51),
                time_ms: 1,
            }],
        ))
        .unwrap();
        let multi_region = aggregate_runner_usage(&starter_input_with_prior(
            0,
            vec![
                StagedRunnerEvent {
                    tenant_id: t,
                    region: "iad".to_string(),
                    qty_vcpu_seconds: 60 * 3600,
                    idem_key_hex: ik(0x52),
                    time_ms: 1,
                },
                StagedRunnerEvent {
                    tenant_id: t,
                    region: "fra".to_string(),
                    qty_vcpu_seconds: 60 * 3600,
                    idem_key_hex: ik(0x53),
                    time_ms: 2,
                },
            ],
        ))
        .unwrap();

        assert_eq!(multi_region.counters.len(), 2);
        assert_eq!(one_region.shadow_ledger, multi_region.shadow_ledger);
    }

    #[test]
    fn duplicate_idempotency_key_is_a_noop_for_usage_chain_and_shadow_charge() {
        let t = tenant(42);
        let event = StagedRunnerEvent {
            tenant_id: t,
            region: "iad".to_string(),
            qty_vcpu_seconds: 120 * 3600,
            idem_key_hex: ik(0x61),
            time_ms: 1,
        };
        let once =
            aggregate_runner_usage(&starter_input_with_prior(0, vec![event.clone()])).unwrap();
        let duplicate =
            aggregate_runner_usage(&starter_input_with_prior(0, vec![event.clone(), event]))
                .unwrap();
        assert_eq!(duplicate.counters, once.counters);
        assert_eq!(duplicate.chain_head_updates, once.chain_head_updates);
        assert_eq!(duplicate.counters[0].event_count, 1);
        assert_eq!(duplicate.shadow_ledger, once.shadow_ledger);
    }

    // This is intentionally an independent integer oracle. It does not call
    // the production money helper or cumulative charge function, so a shared
    // allowance/rounding mistake cannot make this property vacuously green.
    fn full_period_oracle(total: u128, allowance: u128, rate: u64) -> u128 {
        total.saturating_sub(allowance) * u128::from(rate) * 1000
            / u128::from(SECONDS_PER_VCPU_HOUR)
    }

    #[test]
    fn deterministic_random_partitions_match_the_full_period_oracle() {
        let tenant_id = tenant(42);
        let regions = ["iad", "fra", "nrt", "syd", "gru"];
        let mut state = 0x1630_cafe_u64;
        let mut next = || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            state
        };

        for case_id in 0..64_u8 {
            let rate = (next() % 97) + 1;
            let terms = terms_for(tenant_id, 100 * 3600, rate);
            let events: Vec<_> = (0..24_u8)
                .map(|event_id| StagedRunnerEvent {
                    tenant_id,
                    region: regions[(next() as usize) % regions.len()].to_string(),
                    qty_vcpu_seconds: u128::from(1_000 + next() % 30_000),
                    idem_key_hex: ik(case_id.wrapping_add(event_id)),
                    time_ms: next(),
                })
                .collect();
            let full_total: u128 = events.iter().map(|event| event.qty_vcpu_seconds).sum();
            let expected = full_period_oracle(full_total, terms.allowance_vcpu_seconds, rate);

            let mut one_input = base_input(events.clone());
            one_input
                .tenant_period_terms
                .insert(tenant_id, terms.clone());
            let one = aggregate_runner_usage(&one_input).unwrap();
            assert_eq!(
                one.shadow_ledger[0].cumulative_shadow_charge_millicents, expected,
                "case {case_id}: one batch must match the independent full-period oracle"
            );

            let mut offset = 0_usize;
            let mut consumed = 0_u128;
            let mut partition_charge = 0_u128;
            while offset < events.len() {
                let width = ((next() % 5) + 1) as usize;
                let end = (offset + width).min(events.len());
                let mut partition_input = base_input(events[offset..end].to_vec());
                partition_input
                    .tenant_period_terms
                    .insert(tenant_id, terms.clone());
                if consumed != 0 {
                    partition_input
                        .prior_consumption
                        .insert(tenant_id, prior_consumption(&terms, consumed));
                }
                let output = aggregate_runner_usage(&partition_input).unwrap();
                let line = &output.shadow_ledger[0];
                partition_charge += line.charge_delta_millicents;
                consumed = line.cumulative_vcpu_seconds;
                offset = end;
            }
            assert_eq!(consumed, full_total, "case {case_id}: all regions converge");
            assert_eq!(
                partition_charge, expected,
                "case {case_id}: arbitrary partitions must preserve the canonical shadow charge"
            );
        }
    }

    #[test]
    fn late_event_uses_the_fixed_period_snapshot_and_existing_consumption() {
        let tenant_id = tenant(42);
        let terms = terms_for(tenant_id, 100 * 3600, 20);
        let first = aggregate_runner_usage(&starter_input_with_prior(
            0,
            vec![StagedRunnerEvent {
                tenant_id,
                region: "iad".to_string(),
                qty_vcpu_seconds: 99 * 3600,
                idem_key_hex: ik(0xa1),
                time_ms: 20,
            }],
        ))
        .unwrap();
        assert_eq!(first.shadow_ledger[0].charge_delta_millicents, 0);

        let late = aggregate_runner_usage(&starter_input_with_prior(
            99 * 3600,
            vec![StagedRunnerEvent {
                tenant_id,
                region: "fra".to_string(),
                qty_vcpu_seconds: 2 * 3600,
                idem_key_hex: ik(0xa2),
                // It arrives after the first drain but was emitted earlier.
                time_ms: 10,
            }],
        ))
        .unwrap();
        let line = &late.shadow_ledger[0];
        assert_eq!(line.terms_snapshot_ref, terms.terms_snapshot_ref);
        assert_eq!(line.cumulative_vcpu_seconds, 101 * 3600);
        assert_eq!(line.charge_delta_millicents, 20_000);
        assert_eq!(line.cumulative_shadow_charge_millicents, 20_000);
    }

    #[test]
    fn rejects_wrong_contract_or_prior_scope_and_overflow() {
        let t = tenant(42);
        let mut unsupported = starter_input_with_prior(0, vec![]);
        unsupported.artifact_contract_version = 2;
        assert!(matches!(
            aggregate_runner_usage(&unsupported),
            Err(RunnerAggregateError::UnsupportedArtifactContractVersion { .. })
        ));

        let mut wrong_period = starter_input_with_prior(0, vec![]);
        wrong_period
            .prior_consumption
            .get_mut(&t)
            .unwrap()
            .billing_period = "2026-09".to_string();
        assert!(matches!(
            aggregate_runner_usage(&wrong_period),
            Err(RunnerAggregateError::InvalidPriorConsumption { .. })
        ));

        let mut missing_prior_terms = starter_input_with_prior(0, vec![]);
        missing_prior_terms.tenant_period_terms.clear();
        assert!(matches!(
            aggregate_runner_usage(&missing_prior_terms),
            Err(RunnerAggregateError::InvalidTenantPeriodTerms { .. })
        ));

        let mut wrong_terms_period = starter_input_with_prior(0, vec![]);
        wrong_terms_period
            .tenant_period_terms
            .get_mut(&t)
            .unwrap()
            .billing_period = "2026-09".to_string();
        assert!(matches!(
            aggregate_runner_usage(&wrong_terms_period),
            Err(RunnerAggregateError::InvalidTenantPeriodTerms { .. })
        ));

        let mut wrong_terms_tenant = starter_input_with_prior(0, vec![]);
        wrong_terms_tenant
            .tenant_period_terms
            .get_mut(&t)
            .unwrap()
            .tenant_id = tenant(43);
        assert!(matches!(
            aggregate_runner_usage(&wrong_terms_tenant),
            Err(RunnerAggregateError::InvalidTenantPeriodTerms { .. })
        ));

        let mut changed_terms = starter_input_with_prior(0, vec![]);
        let terms = changed_terms.tenant_period_terms.get_mut(&t).unwrap();
        terms.allowance_vcpu_seconds += 1;
        assert!(matches!(
            aggregate_runner_usage(&changed_terms),
            Err(RunnerAggregateError::InvalidTenantPeriodTerms { .. })
        ));

        let terms = changed_terms.tenant_period_terms.get_mut(&t).unwrap();
        terms.terms_snapshot_ref = "terms://current/2026-08/v2".to_string();
        terms.terms_snapshot_digest_hex = terms.expected_snapshot_digest_hex();
        assert!(matches!(
            aggregate_runner_usage(&changed_terms),
            Err(RunnerAggregateError::InvalidTenantPeriodTerms { .. })
        ));

        // A tier/rate change belongs to a later snapshot/period; it cannot
        // retroactively reprice consumption already bound to this snapshot.
        let mut changed_rate = starter_input_with_prior(0, vec![]);
        let terms = changed_rate.tenant_period_terms.get_mut(&t).unwrap();
        terms.rate_cents_per_vcpu_hour += 1;
        terms.terms_snapshot_digest_hex = terms.expected_snapshot_digest_hex();
        assert!(matches!(
            aggregate_runner_usage(&changed_rate),
            Err(RunnerAggregateError::InvalidTenantPeriodTerms { .. })
        ));

        let overflow = starter_input_with_prior(
            u128::MAX,
            vec![StagedRunnerEvent {
                tenant_id: t,
                region: "iad".to_string(),
                qty_vcpu_seconds: 1,
                idem_key_hex: ik(0x71),
                time_ms: 1,
            }],
        );
        assert!(matches!(
            aggregate_runner_usage(&overflow),
            Err(RunnerAggregateError::ArithmeticOverflow { .. })
        ));
    }
}
