---
id: "ADR-0023"
type: "adr"
doc_status: "FROZEN"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-29"
updated: "2026-04-29"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "side-channel", "timing", "404-missreason-parity", "constant-time", "tower-middleware", "mann-whitney", "s02"]
---

# ADR-0023 — Constant-Time 404 MissReason Parity Defense via Tower Timing-Padding Middleware

## Status

FROZEN (S-02 WI-S02-004 ratificada em Lote 10.2bis cycle 7+ SEAL ship gate; aligned com ADR-0028 uniform 404 freeze).

## Context

S-02 read path (`ContentAddressableStorage::Read`, `GetBlob`, `FindMissingBlobs`) tem três classes de **404 MissReason** (per ADR-0028 + WI-S02-005 §6.1):

1. **NotFound**: digest never existed in tenant scope (KV negative cache hit OR D1 row absent fast path).
2. **CrossTenantMasked**: digest exists em outro tenant; AuthZ check `SELECT 1 ... WHERE tenant_id=? AND digest=?` returns 0; D1 found + AuthZ reject + audit emit (slower compute path).
3. **Tombstoned**: digest had existed in tenant but was soft-deleted (S-06 GC); D1 row found com `deleted_at IS NOT NULL` (intermediate compute path).

Per ADR-0028, todos os três variants retornam HTTP 404 uniform com same body — atacante NÃO distingue via response. **Mas underlying compute paths differ → timing leak permite enumeration:**

- Atacante autentica em Tenant B com PAT válido.
- Probe candidate digests (e.g., guessed Docker image hashes; public model checkpoint digests).
- Mede latency distribution per probe.
- Cluster fast (NotFound; KV hit) vs medium (Tombstoned; D1 row + soft-delete check) vs slow (CrossTenantMasked; D1 row + AuthZ reject + audit emit).
- Statistical analysis revela qual MissReason aplica → enumera blobs cross-tenant OR identifies soft-deleted blobs (which discloses prior existence).

**Threat model**: THR-I-002 (side-channel timing exposing existência de blob — `security_model.md §5.4`). Without mitigation: INV-TENANT-ISOLATION violated indirectly (existence oracle); INV-CAS-INTEGRITY trustworthiness undermined.

## Decision

**Tower middleware `TimingPaddingLayer` em `crates/corelink-worker/src/middleware/timing_padding.rs` que:**

1. **Pads apenas 404 responses** (uniform per ADR-0028; all MissReason variants):
   - Target latency: 200ms p99 (configurable via DO config-singleton S-13 forward).
   - Jitter: ±10% (deterministic RNG seeded per `request_id`; não broken via correlation).
   - Implementation: `tokio::time::sleep_until(start + computed_target)` (Tokio scheduler timer; **não CPU spin** — zero CF Worker CPU cost added).

2. **Não pads outros status codes**:
   - **200 OK**: atacante já tem o body; padding sem security benefit.
   - **403 PERMISSION_DENIED** (PAT scope failures, S-03): legitimate caller without scope é not enumeration vector — they know what they don't have access to; padding sem benefit.
   - **413, 429, 500, 503**: sem enumeration semantic.

3. **Statistical proof gate** (CI; documented em WI-S02-004 §10.4.1 + spec_contract §7.10.s02.4):
   - **3-arm methodology**: 10k samples per arm × 3 arms (NotFound × CrossTenantMasked × Tombstoned).
   - **Within-trial Šidák correction**: pairwise Mann-Whitney U test (3 pairs); per-pair effective α = 0.0170 → combined trial α ≤ 0.05.
   - **Across-trial Šidák replication** (CI flake mitigation): 3 independent trials × 3 pairs = 9 tests total; **ALL 9 tests must p > 0.05** (full conjunction acceptance); per-test Šidák α' = 1 − (1−0.05)^(1/9) ≈ 0.0057 controls combined familywise α at 0.05 target. Earlier '0.000125' claim was math error; cycle 13 SEAL corrected.
   - **Power analysis a priori**: 1−β ≥ 0.80 com effect d = 0.2 (custom Rust Cohen's d implementation OR external G*Power tool — `statrs` crate **não expõe** statistical_power).
   - **Mann-Whitney U via `statrs::stats_tests::mann_whitney_u`** (per [statrs docs](https://docs.rs/statrs/latest/statrs/stats_tests/index.html)).
   - **Bootstrap 95% CI sobre |Δmedian|**: target ≤ 1ms cada par (CI cruzando 0 mandatory).

4. **Operational métrica + alert** (single-source canonical; aligned cycle 5 SEAL):
   - Métrica: `corelink.cas.side_channel.timing_diff_ms` (gauge; 5min sliding window, p99 of max pairwise |Δmedian| across 3 arms).
   - Alert: SEV-2 se sustained > 5ms 5min canonical.

## Consequences

### Positive

- INV-CAS-SIDE-CHANNEL-INDISTINGUISHABLE (HIGH em invariant_registry §3.12) enforced canonically via 3-arm parity + statistical gate.
- CTRL-ISO-004 implemented com evidence-grade methodology (não cargo-cult `p > 0.05` sozinho).
- Atacante enumeration vector eliminado; cross-tenant existence oracle fechado.

### Negative

- **Cliente perceived latency tax**: ~150ms avg padding aplicado a ~10% das requests (404 MissReason variants × non-fast-path). Customer impact analysis em WI-S02-004 §22: aggregate ~25min/dia cumulative cliente wait time across all S-02 traffic; per-cliente imperceptible (200ms p99 within SLO-LAT-CAS-GET budget).
- **CF Worker CPU cost**: $0 (sleep timer não CPU spin; padding compute ~µs).
- **Statistical gate complexity**: requires Statistician methodology specialization (folded into Architect role per framework §33.5.4.3 + ADR-0034).

### Trade-offs rejected

- **Constant-time D1 query** (database-level timing): D1 SQLite engine tem inherent timing diffs (B-tree depth, cache vs disk); fundamentally not implementable; mitigated apenas at middleware layer.
- **Adaptive padding** (dynamic target based on load): ML-based tuning overkill at GA; static target suffices.
- **Pad all responses including 200 OK**: latency tax sem security benefit (atacante já tem o body).

## Alternatives considered

1. **No mitigation**: violates CTRL-ISO-004 + INV-CAS-SIDE-CHANNEL-INDISTINGUISHABLE; rejected.
2. **Fixed padding (no jitter)**: detectable via correlation analysis; rejected.
3. **Random jitter without seeded RNG**: broken via correlation analysis attacker-side; rejected.
4. **Spinlock padding**: CPU waste; rejected (use `tokio::time::sleep_until`).
5. **2-arm methodology (404 vs 403)**: was original spec but ADR-0028 unified status code → 3-arm 404 MissReason parity is correct model.

## Compliance

- **THR-I-002** (security_model.md §5.4) mitigated.
- **CTRL-ISO-004** (security_model.md §6.3) implemented.
- **INV-CAS-SIDE-CHANNEL-INDISTINGUISHABLE** (invariant_registry.md §3.12) enforced.
- **NIST SP 800-90B Annex C** referenced para `p > 0.05` industry-standard "indistinguishable" baseline.
- **OWASP Side-Channel Testing** referenced.
- **CVE-2018-0114 mitigation guides** referenced.

## Implementation evidence

- `crates/corelink-worker/src/middleware/timing_padding.rs` (Tower middleware).
- `tests/timing_indistinguishability.rs` (adversarial test 3-arm × 10k each).
- `benches/side_channel.rs` (criterion benchmark |Δmedian| ≤ 1ms).
- `docs/internal/side-channel-defense.md` (theory + implementation + operational).
- WI-S02-004 §6.1 (in-scope spec).

## Migration path

- **GA**: static `target_p99_ms = 200` (default); `jitter_pct = 10` (±10%).
- **Post-GA S-13 (config DO)**: tunable per-region via DO config-singleton; admin API gated.
- **Future S-15+ adaptive**: ML-based dynamic tuning if needed (não baseline; deferred).

---

**Fim ADR-0023.**
