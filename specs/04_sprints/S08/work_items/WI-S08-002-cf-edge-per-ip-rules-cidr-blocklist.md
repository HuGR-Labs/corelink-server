---
id: "WI-S08-002"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-005", "FF-HR-002"]
parent: "S-08"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "RESILIENCE-PATTERNS"
  - "SECURITY-MODEL"
  - "INVARIANT-REGISTRY"
  - "FAILURE-MODES"
  - "OBSERVABILITY-MODEL"
tags: ["wi", "s08", "rate-limit", "edge", "cf-ruleset", "per-ip", "cidr-blocklist", "ddos", "high-risk"]
---

# WI-S08-002 — CF Edge Per-IP Rate Rules + CIDR Blocklist (`infra/cloudflare/rulesets/`; CF Ruleset Engine pre-Worker enforcement; per-IP token bucket camada 2 of 4 do bulkhead multi-camada PAT-RATE-LIMIT-001; CIDR blocklist sourced de D1 `ip_blocklist` table; admin push via S-13 webhook → Terraform `cloudflare_ruleset` apply OR CF API runtime mutate; pre-auth defense-in-depth ANTES do Worker invocation; FM-250 DDoS volumetric mitigation; NAT-aware (multi-user atrás de IP corporate/mobile carrier não bloqueado false-positive); IPv4 /32 + IPv6 /64 granularity canonical)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-08](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S08-002 |
| Título | CF Ruleset Engine per-IP rate rules + dinâmico CIDR blocklist; pre-Worker enforcement (saves CPU + DO state em adversarial IP floods); 2 thresholds: (a) **anonymous (no Authorization header)** 10 RPS per IP /32 (free-grade safety) ou 100 RPS per IPv6 /64; (b) **authenticated (any token presente)** 1000 RPS per IP /32 ou 10000 RPS per IPv6 /64 (NAT-aware allowance — corporate/mobile-carrier scenarios Lote 10.7bis P0-6 race-aware NAT consideration absorbed); CIDR blocklist em D1 `ip_blocklist(cidr_text, reason, added_at, expires_at, added_by_admin_id, source)`; admin endpoint S-13 push para CF Ruleset via Terraform OR runtime CF API; sustained 5min × > 10000 RPS × 100% 4xx from single IP/ASN trigger automated suggest-block (admin review gated; humane response per sprint contract §7.10.s08.3 LGPD Art. 20 alignment); response 429 + `X-Rate-Limit-Type: per_ip` + `Retry-After: <seconds>`; INV-AVAIL-ISOLATION reinforcement via edge-layer drop (adversarial flood never reaches DO); zero cross-tenant linkability em IP layer (no tenant_id at edge; pre-auth) |
| Sprint | S-08 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (CTRL-RATE-001 security control camada 2; bypass at edge = INV-AVAIL-ISOLATION cross-tenant degradation pathway), FF-HR-002 (single adversarial IP saturates Worker CPU budget afetando todos tenants) |

## 1. Intent

CF edge per-IP rate limit é o **outermost bulkhead layer** do CoreLink — drops adversarial IP floods ANTES do Worker invocation, preservando CPU budget + DO state pro tráfego legítimo. Sem isso, a per-tenant DO RateLimiter (WI-S08-001) recebe overload a montante, causando DO cold-start storm + thundering herd at boundary. CF Ruleset Engine é declarative IaC; CIDR blocklist sincronizado de D1 via S-13 admin push.

```yaml
# File: infra/cloudflare/rulesets/per_ip_rate_limit.tf (Terraform managed)
# Lote 10.8bis P1-5 (R5): provider v4+ canonical schema — `action_parameters` block
# wrapping `ratelimit{}` (NOT legacy v3 top-level `ratelimit{}` block).

terraform {
  required_providers {
    cloudflare = {
      source  = "cloudflare/cloudflare"
      version = "~> 4.0"  # provider v4+ canonical (Lote 10.8bis P1-5)
    }
  }
}

resource "cloudflare_ruleset" "corelink_edge_rate_limit" {
  zone_id     = var.cloudflare_zone_id
  name        = "corelink-edge-per-ip-rate-limit"
  description = "Edge per-IP token bucket; pre-Worker enforcement (FF-HR-005)"
  kind        = "zone"
  phase       = "http_ratelimit"

  # Camada 2a: anonymous (no Authorization header) — provider v4 syntax
  rules {
    action      = "block"
    description = "Anonymous IP rate limit"
    expression  = "(http.request.uri.path matches \"^/v1/\" and not http.request.headers[\"authorization\"][0] matches \".*\")"
    action_parameters {
      ratelimit {
        characteristics       = ["ip.src"]
        period                = 60
        requests_per_period   = 600    # 10 RPS sustained over 60s window
        mitigation_timeout    = 60
        counting_expression   = ""
      }
    }
  }

  # Camada 2b: authenticated (NAT-aware allowance)
  rules {
    action      = "block"
    description = "Authenticated IP rate limit (NAT-aware)"
    expression  = "(http.request.uri.path matches \"^/v1/\" and http.request.headers[\"authorization\"][0] matches \".*\")"
    action_parameters {
      ratelimit {
        characteristics       = ["ip.src"]
        period                = 60
        requests_per_period   = 60000   # 1000 RPS sustained over 60s window
        mitigation_timeout    = 60
      }
    }
  }

  # Camada 2c: CIDR blocklist (sourced de D1 admin push)
  rules {
    action      = "block"
    description = "Static CIDR blocklist (sustained-abuse + manual admin)"
    expression  = "(ip.src in $corelink_ip_blocklist)"
    # $corelink_ip_blocklist é managed CF List; populated por S-13 admin push
  }
}

resource "cloudflare_list" "corelink_ip_blocklist" {
  account_id  = var.cloudflare_account_id
  name        = "corelink_ip_blocklist"
  description = "Per-IP/CIDR blocklist sourced de D1 ip_blocklist; updated via S-13 admin webhook"
  kind        = "ip"
  # items populated dinamicamente via cloudflare_list_item resources OR runtime CF API
}
```

```rust
// File: crates/corelink-edge-blocklist/src/lib.rs
// (admin push integration; S-13 dependency)

#![forbid(unsafe_code)]

#[async_trait]
pub trait EdgeBlocklist: Send + Sync {
    /// Adicionar CIDR ao blocklist (D1 + CF List sync); admin-gated (S-13).
    /// Audit fail-closed: se audit emit fails, abort transaction.
    async fn add_block(
        &self,
        admin_ctx: &AdminCtx,
        entry: BlocklistEntry,
    ) -> Result<(), EdgeBlocklistError>;

    /// Remover CIDR (admin appeal flow OR expired); D1 + CF List sync.
    async fn remove_block(
        &self,
        admin_ctx: &AdminCtx,
        cidr: Cidr,
    ) -> Result<(), EdgeBlocklistError>;

    /// Sustained-abuse detection: emit suggest_block event (NOT auto-apply; humane response).
    /// Source: edge logs aggregated 5min via CF Logpush → R2 → S-09 analytics.
    async fn suggest_block(
        &self,
        observation: SustainedAbuseObservation,
    ) -> Result<SuggestionId, EdgeBlocklistError>;

    /// Reconcile D1 ↔ CF List drift (nightly + on-demand).
    async fn reconcile(&self) -> Result<ReconcileReport, EdgeBlocklistError>;
}

pub struct BlocklistEntry {
    pub cidr: Cidr,                     // canonical text "192.0.2.0/24" ou "2001:db8::/32"
    pub reason: BlockReason,            // Sustained4xx | DDoSPattern | ManualAdmin | Compliance
    pub expires_at_ms: Option<i64>,     // None = permanent (admin manual)
    pub added_by_admin_id: AdminId,
    pub source: BlockSource,            // Manual | AutomatedSuggestionApproved
    pub appeal_url: String,             // sprint contract §7.10.s08.3 humane: customer can appeal
}

#[derive(thiserror::Error, Debug)]
pub enum EdgeBlocklistError {
    #[error("CIDR parse error: {0}")]
    CidrParseError(String),

    #[error("CF API error: {0}")]
    CfApiError(String),

    #[error("D1 backend error: {0}")]
    D1BackendError(String),

    #[error("audit emit failed (fail-closed; transaction aborted): {0}")]
    AuditEmitFailed(String),

    #[error("admin authorization failed: {0}")]
    AdminAuthFailed(String),

    #[error("CIDR overlap detected with existing entry: {existing}")]
    CidrOverlap { existing: String },

    #[error("automated suggestion not auto-applied; admin review required (LGPD Art. 20 / sprint contract §7.10.s08.3)")]
    HumaneReviewRequired,
}
```

**Cripto-driven invariants enforced**:

1. **INV-AVAIL-ISOLATION** (HIGH; registry §3.8) reinforcement at edge layer: adversarial single-IP flood drops at CF edge BEFORE per-tenant DO. Cross-tenant collateral impossible by edge enforcement (per-IP characteristic = `ip.src`, NOT `tenant_id`).

2. **CTRL-RATE-001 camada 2** (security_model.md §300; sprint contract §4 CAP-RATE-002): per-IP edge layer; 4-camadas defense-in-depth (camada 1 per-tenant DO em WI-S08-001; camada 2 edge per-IP this WI; camada 3 per-PAT em WI-S08-003 quota-middleware; camada 4 global circuit em WI-S08-005 RFC 9331 wrapper).

3. **NAT-awareness** (Lote 10.7bis P0-6 race-aware lesson generalizado): authenticated requests (Authorization header presente) granted 100× higher per-IP allowance (1000 RPS vs 10 RPS); reflects realidade de corporate proxies + mobile carrier NAT (CGNAT) where 1k+ distinct users compartilham single egress IP. False-positive cost: customer NAT scenarios bloqueados → enterprise customer loss (sprint contract §15 R-S08-006). Mitigation: warn-mode 15min antes do hard-block on first-offense.

4. **Humane automated response** (sprint contract §7.10.s08.3 LGPD Art. 20 + GDPR Art. 22 alignment): edge automated detection NEVER auto-applies hard-block. Sustained-abuse pattern (5min × > 10000 RPS × 100% 4xx from single IP/ASN) emits `corelink.edge.suggest_block` event → SEV-2 admin notification → human review ≤ 24h → admin manual approval → CF List add. Customer-self-service appeal endpoint via S-13 (`POST /v1/admin/blocklist_appeal`).

5. **CIDR canonical format**:
   - IPv4: `/32` (single IP) ou broader `/24`, `/16`, `/8` (admin manual; CIDR overlap rejected).
   - IPv6: `/128` (single IP) ou broader `/64` (canonical privacy boundary RFC 4291 §2.5.4); `/64` é minimum granularity para IPv6 prefix-based abuse (avoiding /128-by-/128 cardinality explosion em CF List).

6. **CF List size limit**: CF List supports 10000 items per list (CF docs); CoreLink starts com single list `corelink_ip_blocklist`; alarm SEV-3 se > 8000 items (proactive shard); shard plan deferred to S-14.

7. **D1 ↔ CF List consistency**:
   - Source-of-truth: D1 `ip_blocklist` table (durable, audited, queryable).
   - Replica: CF List (enforcement only; eventual consistency ≤ 5min).
   - Reconcile job: nightly + on-demand (admin trigger via S-13); detects drift; emits SEV-2 alert se drift > 10 entries.
   - Audit fail-closed (Lote 10.6bis pattern absorbed): se `corelink.edge.blocklist_add` event emit fails, transaction aborts (D1 row not inserted; CF List not pushed); no half-state.

8. **No tenant linkability at edge** (LINDDUN privacy): edge layer pre-auth; tenant_id NOT computed at edge layer; per-IP enforcement uses only `ip.src` (Cloudflare characteristic). Audit log redacts last octet of IPv4 (`192.0.2.X`) por privacy_model.md PII redaction policy; full IP retained em D1 for 30d operational then redacted.

## 2. Narrative (HIGH_RISK ≥ 300 palavras + race-correctness justification)

CF edge per-IP rate limit é a **outermost defense-in-depth layer** do bulkhead multi-camada (PAT-RATE-LIMIT-001). Sem essa camada, adversarial IP floods chegariam ao Worker (CPU budget + cold-start cost) e em seguida ao per-tenant DO (state contention + thundering herd). Edge drop é **CPU-free**: CF Ruleset Engine evaluates `ratelimit{}` em hardware-acelerado layer ANTES do Worker invocation; cost per dropped request é ~10× cheaper que Worker CPU.

**Why CF Ruleset Engine (vs Worker-level rate limit)**: (1) Worker invocation cost: each Worker invocation = ~$0.0000005 + CPU time; at 10k QPS adversarial flood = $5/hour wasted CPU se enforced em Worker. CF edge enforcement: 0 Worker cost. (2) Worker concurrency limit: CF account-level concurrency cap; flood at edge would saturate concurrency, dropping legitimate requests. (3) DDoS pre-mitigation: CF DDoS-managed integrates com Ruleset; layered defense.

**Why 2-tier threshold (anonymous vs authenticated)** (Lote 10.7bis P0-6 NAT-aware lesson absorbed): anonymous traffic = 10 RPS per IP (free-grade safety; pre-auth abuse mitigation; comparable a GitHub REST API 60/hour unauthenticated por sprint contract §16); authenticated traffic = 1000 RPS per IP (NAT allowance; corporate proxies + mobile CGNAT routinely share single egress IP across 1k+ users — false-positive blocking would lose enterprise customers; sprint contract §15 R-S08-006). Threshold sourced de Stripe API benchmark (25 RPS livemode = ~10× safety multiplier above) + AWS API Gateway throttling default (10k RPS × NAT-divisor).

**Why IPv6 /64 granularity** (RFC 4291 §2.5.4 canonical): IPv6 hosts assignment é typically /64 prefix per network; treating /128 as enforcement granularity explodes CF List cardinality (one entry per `2^64` host theoretically). /64 boundary é pragmatic + RFC-aligned; /128 reserved for surgical manual admin (compliance / law-enforcement requests).

**Why CIDR blocklist em D1 (vs CF List as source-of-truth)**: D1 é queryable (audit log integration; admin dashboards; appeal workflow); CF List é enforcement replica (~5min eventual consistency). Reconcile job nightly + on-demand catches drift. Pattern aligns com S-07 WI-S07-002 D1-as-source-of-truth + per-region cache.

**Why humane automated response** (sprint contract §7.10.s08.3): LGPD Art. 20 (revisão de decisões automatizadas) + GDPR Art. 22 prohibit fully-automated significant decisions affecting individuals without human review. Edge IP block é significant (user cannot access service); thus automated detection → SEV-2 admin review (≤ 24h) → manual approval → block. Self-service appeal via S-13.

**Adversarial scenarios**:
- **IP spoofing** (HTTPS origin): TLS handshake authenticates source IP at edge; spoofing infeasible em IPv4 + IPv6 com BCP 38 enforcement upstream. Threat model: not load-bearing.
- **Distributed botnet** (1000 IPs × 10 RPS each = 10k RPS aggregate): per-IP threshold doesn't trigger; mitigation falls to camada 1 (per-tenant DO RateLimiter WI-S08-001) + camada 4 (global circuit breaker WI-S08-005). Layered defense.
- **NAT-legitimate scenario** (corporate proxy 500 distinct authenticated users behind 1 IP, 2 RPS each = 1000 RPS): authenticated threshold 1000 RPS → on boundary; warn-mode kicks in primeiro; admin reviews; if legitimate, adds CIDR exception list (parallel `corelink_ip_allowlist` future S-14).
- **Sustained-abuse single IP** (10000 RPS × 5min × 100% 4xx): suggest_block event emit → SEV-2 admin notification → manual approval → CF List add.
- **D1 ↔ CF List drift** (CF API outage 30min): reconcile job catches drift on recovery; no enforcement gap (CF retains last-known list state); audit emits SEV-2.
- **Reconcile job race with admin add** (admin adds entry mid-reconcile): D1 `ip_blocklist` é authoritative; admin INSERT acquires row-level lock; reconcile job uses snapshot read; eventual consistency ≤ next reconcile cycle.

**Risk justification HIGH_RISK**:
- **FF-HR-005**: CTRL-RATE-001 camada 2; bypass at edge = AVAIL-ISOLATION cross-tenant degradation pathway (adversarial flood saturates Worker CPU shared cross-tenant).
- **FF-HR-002**: single adversarial IP can saturate Worker CPU + DO cold-start budget impacting todos tenants.
- 11 sign-offs (HIGH_RISK 10–12; this WI Crypto SME advisory N/A — pure network-layer enforcement; substituted por NetSec advisor) + chaos suite 11 + property test 100k race iterations on D1↔CF reconcile.

## 3. Customer Impact & Journey

**Persona 1 — Customer (NAT corporate proxy)**: 500 authenticated dev workstations behind single corporate egress IP at 2 RPS each (1000 RPS aggregate); on threshold; **warn-mode** activates (header `X-Rate-Limit-Warning: per_ip_approaching` em response); admin notified; corporate customer requests CIDR allowlist via S-13 ticket; admin adds; problem resolved.

**Persona 2 — Anonymous abuse (probe)**: anonymous IP probing endpoint discovery at 50 RPS; threshold 10 RPS exceeded; CF edge drops 80% requests; 429 + `X-Rate-Limit-Type: per_ip` + `Retry-After: 1`; abuse_score (WI-S08-004) NOT incremented (pre-auth; no tenant context); just edge drop. Cost-free defense.

**Persona 3 — Authenticated single user (legitimate)**: developer running parallel `bazel build` at 200 RPS sustained on single laptop; under 1000 RPS authenticated threshold; never hit edge limit; per-tenant DO RateLimiter (WI-S08-001) handles within-plan logic.

**Persona 4 — DDoS volumetric attack** (FM-250): 100k RPS distributed across 10000 IPs (10 RPS each); per-IP threshold doesn't trigger (10 RPS = on boundary anonymous); mitigation falls to CF DDoS-managed (account-level) + camada 4 global circuit breaker (WI-S08-005). Edge per-IP é defense against single-source flood, not distributed.

**Persona 5 — DevOps reviewing**: DASH-RATE (WI-S08-006) shows edge per-IP block_total + suggest_block_pending queue; admin reviews suggestions ≤ 24h; approves → CF List add → audit log; rejects → ignore (no false-positive blocked).

**SLA addendum**:
- Edge enforcement latency: ≤ 1ms p99 (CF Ruleset hardware-accelerated; sub-Worker).
- CIDR blocklist propagation: ≤ 5min D1 → CF List eventual consistency.
- Reconcile drift detection: nightly + on-demand; alert if drift > 10 entries.
- INV-AVAIL-ISOLATION reinforcement: 0 cross-tenant collateral em adversarial single-IP scenarios (chaos test 30d).
- False-positive rate target: ≤ 0.1% authenticated requests blocked falsely (NAT scenarios — 14.s08.002.4).

## 4. Capability Mapping

- **CAP-RATE-002** (Per-IP edge rate limit) — IMPLEMENTA primary.
- Trace: `security_model.md CTRL-RATE-001` (security control camada 2) + `resilience_patterns.md PAT-RATE-LIMIT-001` (multi-camada bulkhead) + `failure_modes.md FM-250 (DDoS volumetric)` + `failure_modes.md FM-201 (Config change rate-limit drop; Lote 10.8bis P1-3 — FM-251 canonical é "Credential stuffing / brute force")` + sprint contract §7.10.s08.3 (humane automated response LGPD).

## 5. Tipo

CF Ruleset Engine IaC + D1 backing table + admin sync via S-13 + reconcile job; HIGH_RISK; FF-HR-005 + FF-HR-002.

## 6. Escopo

### 6.1 In-scope

1. **`infra/cloudflare/rulesets/per_ip_rate_limit.tf`** — Terraform managed CF Ruleset:
   - Rule 1 (anonymous): 10 RPS per `ip.src` /32 IPv4 ou /64 IPv6 over 60s window; action=block (429).
   - Rule 2 (authenticated): 1000 RPS per `ip.src` /32 IPv4 ou /64 IPv6 over 60s window; action=block (429).
   - Rule 3 (CIDR blocklist): CF List `corelink_ip_blocklist`; action=block (no rate; static blocklist).
   - Mitigation_timeout 60s (block window after threshold breach).
   - Phase: `http_ratelimit` (pre-Worker; CF docs canonical).

2. **`crates/corelink-edge-blocklist/` module** — D1 ↔ CF List sync:
   - Trait `EdgeBlocklist` (add/remove/suggest/reconcile).
   - DO `EdgeBlocklistSync` per-region; alarm 5min reconcile cycle.
   - DO alarm re-arm AT START (Lote 10.4bis lesson absorbed).

3. **D1 migrations** (NEW tables; CHECK constraints inline per Lote 10.5bis lesson):
   ```sql
   CREATE TABLE ip_blocklist (
       cidr_text TEXT PRIMARY KEY,                  -- canonical "192.0.2.0/24" or "2001:db8::/32"
       cidr_family INTEGER NOT NULL,                -- 4 or 6
       reason TEXT NOT NULL,                        -- Sustained4xx | DDoSPattern | ManualAdmin | Compliance
       added_at INTEGER NOT NULL,                   -- unix ms; canonical no _ms suffix per Lote 10.7bis P0-3
       expires_at INTEGER,                          -- NULL = permanent; canonical no _ms suffix
       added_by_admin_id TEXT NOT NULL,
       source TEXT NOT NULL,                        -- Manual | AutomatedSuggestionApproved
       appeal_url TEXT NOT NULL,                    -- sprint contract §7.10.s08.3 humane
       cf_list_synced_at INTEGER,                   -- watermark; NULL until pushed to CF
       deleted_at INTEGER,                          -- soft-delete; canonical no _ms suffix
       CHECK (cidr_family IN (4, 6)),
       CHECK (reason IN ('Sustained4xx', 'DDoSPattern', 'ManualAdmin', 'Compliance')),
       CHECK (source IN ('Manual', 'AutomatedSuggestionApproved')),
       CHECK (expires_at IS NULL OR expires_at > added_at)
   );

   CREATE INDEX idx_ip_blocklist_active ON ip_blocklist(expires_at, deleted_at)
       WHERE deleted_at IS NULL;
   CREATE INDEX idx_ip_blocklist_cf_sync ON ip_blocklist(cf_list_synced_at)
       WHERE deleted_at IS NULL;

   CREATE TABLE ip_blocklist_suggestions (
       suggestion_id TEXT PRIMARY KEY,              -- ULID
       cidr_text TEXT NOT NULL,
       cidr_family INTEGER NOT NULL,
       observation_window_start INTEGER NOT NULL,
       observation_window_end INTEGER NOT NULL,
       observed_rps INTEGER NOT NULL,
       observed_4xx_ratio REAL NOT NULL,            -- 0.0 to 1.0
       suggested_at INTEGER NOT NULL,
       reviewed_at INTEGER,
       reviewer_admin_id TEXT,
       decision TEXT,                               -- approved | rejected | expired_unreviewed
       CHECK (cidr_family IN (4, 6)),
       CHECK (decision IS NULL OR decision IN ('approved', 'rejected', 'expired_unreviewed')),
       CHECK (observed_4xx_ratio >= 0.0 AND observed_4xx_ratio <= 1.0),
       CHECK (observation_window_end > observation_window_start)
   );

   CREATE INDEX idx_suggestions_pending ON ip_blocklist_suggestions(suggested_at, reviewed_at)
       WHERE reviewed_at IS NULL;
   ```
   D1 batch ≤ 250 rows constraint per Lote 10.5bis (reconcile inserts batched).

4. **Admin endpoints** (S-13 dependency; staging stub OK):
   - `POST /v1/admin/blocklist` — add CIDR (admin authenticated; audit fail-closed).
   - `DELETE /v1/admin/blocklist/{cidr}` — remove CIDR (admin appeal flow OR expired).
   - `GET /v1/admin/blocklist` — list active entries (paginated; D1 batch ≤ 250).
   - `GET /v1/admin/blocklist/suggestions` — pending automated suggestions for review.
   - `POST /v1/admin/blocklist/suggestions/{id}/approve` — approve suggestion → CIDR add.
   - `POST /v1/admin/blocklist/suggestions/{id}/reject` — reject (no block; audit log only).
   - `POST /v1/admin/blocklist_appeal` — customer-self-service appeal (sprint contract §7.10.s08.3 humane).

5. **Reconcile DO** (`EdgeBlocklistSync` per-region):
   ```rust
   // Pseudocode
   impl EdgeBlocklistSync {
       async fn alarm(&mut self) -> Result<(), Error> {
           // Re-arm AT START (Lote 10.4bis lesson)
           self.state.set_alarm(now() + Duration::from_secs(300)).await?;

           // Read D1 active entries (batch ≤ 250)
           let active = read_active_blocklist_d1().await?;

           // Read CF List current state
           let cf_list = cf_api_get_list().await?;

           // Diff
           let drift = diff(active, cf_list);

           if drift.added.len() > 0 || drift.removed.len() > 0 {
               // Apply diff to CF List (CF API runtime mutate; OR Terraform if IaC mode)
               cf_api_apply_diff(drift).await?;

               // Update cf_list_synced_at watermark in D1 (batch ≤ 250)
               update_synced_at_d1(drift.added.iter().map(|e| e.cidr_text)).await?;

               // Audit emit fail-closed
               audit_emit("corelink.edge.blocklist_reconciled", drift_summary)?;
           }

           if drift.unsynced.len() > 10 {
               // SEV-2 alert: significant drift
               emit_metric("corelink.edge.blocklist_drift_total", drift.unsynced.len() as f64);
           }

           Ok(())
       }
   }
   ```

6. **Sustained-abuse detection job** (S-09 dependency; CF Logpush → R2 → analytics):
   - Aggregate edge logs 5min windows by `ip.src`.
   - Trigger suggest_block emit if: `rps > 10000 AND ratio_4xx == 1.0 AND duration ≥ 5min`.
   - Insert row em `ip_blocklist_suggestions` (D1).
   - Emit `corelink.edge.suggest_block` event → SEV-2 admin notification (PagerDuty integration via S-09).
   - **NEVER auto-applies** (sprint contract §7.10.s08.3 humane response LGPD Art. 20 / GDPR Art. 22).

7. **CIDR canonical normalization**:
   - Parse via `ipnet` crate (`ipnet::IpNet::from_str`); accept IPv4 + IPv6.
   - Reject overlap detection: novo `192.0.2.0/24` vs existing `192.0.2.0/16` → `EdgeBlocklistError::CidrOverlap` (admin must remove broader OR refuse).
   - IPv6 normalized to /64 minimum (broader allowed; /128 only for compliance manual).

8. **Audit fail-closed pattern** (Lote 10.6bis absorbed):
   - All blocklist mutations emit audit events (`corelink.edge.blocklist_{add, remove, suggest, reconciled}`); audit fail aborts transaction (D1 row rolled back; CF List not pushed).
   - Audit immutability inherited via S-04 `corelink.audit_log` append-only.

9. **CF Workers Rust runtime APIs** (Lote 10.7bis R5 P0-3 lesson absorbed): use `worker::send_future()` para reconcile background; NEVER `tokio::spawn`; NEVER `wasm_bindgen_futures::spawn_local`. `async_lock::Semaphore` para reconcile concurrency control if needed (Lote 10.3-tris lesson).

10. **Métricas** (Prometheus convention):
    - `corelink.edge.rate_limit_block_total{tier=anonymous|authenticated, ip_family=4|6}` (counter; CF Logpush → analytics aggregation).
    - `corelink.edge.cidr_blocklist_size{family=4|6}` (gauge; sampled de D1).
    - `corelink.edge.cidr_blocklist_drift_total` (gauge; reconcile output; **alert SEV-2 if > 10**).
    - `corelink.edge.suggest_block_pending{age_bucket=<1h|1-24h|>24h}` (gauge; **alert SEV-3 if any > 24h** — humane SLA breach).
    - `corelink.edge.reconcile_duration_ms` (histogram; SLO ≤ 30s p99).
    - `corelink.edge.cf_api_error_total{operation=add|remove|list|reconcile}` (counter; **alert SEV-2 if > 10/5min**).
    - `corelink.edge.false_positive_appeal_rate` (gauge; appeals approved / blocks total; **alert SEV-3 if > 1%** — calibration signal).

11. **Property tests** (10k iter PR; **100k nightly per HIGH_RISK SOTA bar** — Lote 10.7bis P1-3 lesson absorbed):
    - `prop_cidr_normalize_idempotent`: 10k random CIDRs; assert parse → format → parse roundtrip.
    - `prop_cidr_overlap_detection`: 1k random CIDR pairs; assert overlap algorithm symmetric + transitive.
    - `prop_d1_cf_reconcile_convergence`: random sequences of D1 inserts + CF List drifts; assert reconcile converges to D1 state em ≤ 2 cycles.
    - `prop_audit_fail_closed`: inject audit emit fail; assert D1 row not inserted + CF List not pushed.
    - `prop_suggest_block_no_auto_apply`: 1k synthetic abuse patterns; assert 0 auto-applied (humane response invariant).
    - `prop_concurrent_admin_add_race`: 100 concurrent admin add of same CIDR; assert exactly 1 succeeds (D1 PRIMARY KEY); 99 receive `CidrOverlap`.

12. **Chaos suite** (HIGH_RISK ≥ 10; this WI = 11):
    - 1. **Single-IP flood anonymous** (10000 RPS sustained 1h from 1 IP) → CF edge blocks at 10 RPS threshold; Worker invocation count near-zero; CPU budget unaffected.
    - 2. **Single-IP flood authenticated** (5000 RPS sustained 1h from 1 IP, valid token) → CF edge blocks at 1000 RPS threshold; per-tenant DO RateLimiter (WI-S08-001) backstop catches remainder.
    - 3. **Distributed botnet** (10000 IPs × 10 RPS each = 100k RPS) → per-IP threshold not triggered; falls through to camada 1 + 4; verify defense-in-depth.
    - 4. **NAT corporate** (1 IP, 500 distinct authenticated users, 2 RPS each = 1000 RPS) → on threshold; warn-mode header emitted; admin notified; manual allowlist resolves.
    - 5. **CF API outage 30min** → reconcile fails; D1 unchanged; CF retains last state; no enforcement gap; SEV-2 alert; recovery: reconcile catches drift.
    - 6. **D1 outage 30min** → admin add temporarily fails; CF List unchanged; recovery: pending operations resume.
    - 7. **CIDR overlap admin** (admin adds /24 over existing /16) → rejected with CidrOverlap error; admin must remove broader first.
    - 8. **Suggest_block race admin add** (admin manually adds CIDR mid-suggestion) → suggestion rendered moot; suggestion auto-cancels with `decision='superseded_by_manual'`.
    - 9. **CF List size approaches 8000** → SEV-3 alert; shard plan triggered (deferred S-14; alert is informational).
    - 10. **Reconcile concurrent with admin mutation** → row-level lock D1; reconcile uses snapshot; admin INSERT serialized; no lost updates.
    - 11. **Audit emit fail mid-add** → D1 row rolled back; CF List not pushed; admin sees error response; idempotent retry succeeds when audit recovers.

### 6.2 Out-of-scope (deferred)

- **CIDR allowlist** (NAT customer exception list; deferred S-14 enterprise tier).
- **CF List sharding** (> 10000 entries; deferred S-14).
- **ML-based abuse detection** (anti-scope sprint contract §10).
- **Per-ASN aggregation** (per-IP only at edge; ASN-level deferred S-14).
- **Geo-IP rules** (compliance / sanctions blocking; deferred S-14 per ADR future).
- **IPv6 /128 surgical block** (deferred; admin manual via DB direct access for compliance request rare cases).
- **CF Turnstile / managed challenge integration** (CAPTCHA before block; deferred S-14 customer experience polish).

## 7. Anti-Scope

- ❌ Auto-apply suggested blocks sem human review (LGPD Art. 20 / GDPR Art. 22 violation; sprint contract §7.10.s08.3 humane response).
- ❌ Worker-level per-IP rate limit (cost overhead; CF edge é canonical).
- ❌ Hard-coded thresholds (use canonical 10/1000 RPS; admin override via S-13 future).
- ❌ Plaintext IP em audit logs (privacy_model.md PII; redact last octet IPv4 / last 64 bits IPv6 after 30d).
- ❌ Skip reconcile drift alert (silent CF List drift would be invisible enforcement gap).
- ❌ `tokio::spawn` em CF Workers Rust (Lote 10.7bis R5 P0-3 lesson).
- ❌ Skip alarm re-arm AT START (Lote 10.4bis lesson).
- ❌ CIDR overlap silent merge (admin intent ambiguous; reject explicit).

## 8. Acceptance Criteria (Gherkin) — 10 scenarios

```gherkin
Feature: CF Edge Per-IP Rate Rules + CIDR Blocklist

  Scenario: Anonymous IP rate limit triggers at 10 RPS
    Given anonymous request (no Authorization header) from IP 192.0.2.1
    Given previous 60s window saw 600 requests (10 RPS sustained)
    When 601st request arrives
    Then CF Ruleset Engine drops at edge (action=block)
    Then response 429 + X-Rate-Limit-Type: per_ip + Retry-After: 60
    Then Worker NOT invoked (CPU budget preserved)
    Then audit emit corelink.edge.rate_limit_block_total{tier=anonymous, ip_family=4}

  Scenario: Authenticated IP NAT-allowance threshold 1000 RPS
    Given authenticated request (valid token) from IP 203.0.113.5 (corporate NAT)
    Given previous 60s window saw 60000 requests from this IP (1000 RPS sustained)
    When 60001st request arrives
    Then CF Ruleset Engine drops at edge
    Then response 429 + X-Rate-Limit-Type: per_ip + Retry-After: 60
    Then per-tenant DO RateLimiter (WI-S08-001) NOT invoked for dropped requests

  Scenario: NAT corporate within authenticated threshold
    Given 500 authenticated dev workstations behind 1 corporate IP at 2 RPS each (1000 RPS total)
    When traffic sustained 1h
    Then on threshold; some warn-mode headers emitted (warning, not blocking)
    Then admin notification SEV-3 (NAT detection signal)
    Then customer requests CIDR allowlist via S-13 ticket
    Then admin allowlist add resolves (no false-positive blocking sustained)

  Scenario: CIDR blocklist add via admin endpoint
    Given admin AdminCtx with valid auth from S-13
    Given CIDR 192.0.2.0/24 NOT in existing blocklist
    When POST /v1/admin/blocklist {cidr: "192.0.2.0/24", reason: "ManualAdmin"}
    Then D1 ip_blocklist row inserted (added_at = now)
    Then CF List `corelink_ip_blocklist` push triggered (eventual ≤ 5min)
    Then audit emit corelink.edge.blocklist_add succeeds (fail-closed)
    Then response 201 Created

  Scenario: CIDR blocklist overlap rejected
    Given existing entry 192.0.2.0/16 in ip_blocklist
    When admin POST /v1/admin/blocklist {cidr: "192.0.2.0/24"}
    Then EdgeBlocklistError::CidrOverlap returned (existing: 192.0.2.0/16)
    Then D1 ip_blocklist row NOT inserted
    Then CF List unchanged
    Then admin must remove /16 first OR refuse

  Scenario: Sustained-abuse suggest_block (humane LGPD)
    Given IP 198.51.100.42 emits 15000 RPS sustained 5min × 100% 4xx
    When CF Logpush → R2 → S-09 analytics aggregates
    Then ip_blocklist_suggestions row inserted (suggested_at = now; decision=NULL)
    Then SEV-2 PagerDuty notification to admin
    Then NEVER auto-applies (sprint contract §7.10.s08.3 humane response)
    Then admin must POST /v1/admin/blocklist/suggestions/{id}/approve to apply

  Scenario: Suggested block reviewed within 24h SLA
    Given pending suggestion 25h old; never reviewed
    Then alert SEV-3 (humane SLA breach)
    Then suggestion expires with decision='expired_unreviewed' nightly cleanup

  Scenario: Reconcile detects drift D1 ↔ CF List
    Given D1 has 100 active entries; CF List has 95 (5 entries failed previous push)
    When reconcile DO alarm fires (5min cycle)
    Then drift detected (5 unsynced entries)
    Then CF API apply_diff pushes 5 entries
    Then cf_list_synced_at watermark updated em D1 batch ≤ 250
    Then drift_total metric = 0 after reconcile
    Then if drift > 10 entries unsynced, SEV-2 alert emitted

  Scenario: Audit fail-closed on add
    Given admin POST /v1/admin/blocklist {cidr: "192.0.2.0/24"}
    Given D1 INSERT succeeds; CF List push succeeds
    Given audit_outbox INSERT fails (D1 throttle)
    When transaction commits
    Then transaction ABORTED (rollback): D1 row deleted; CF List entry removed
    Then admin receives EdgeBlocklistError::AuditEmitFailed
    Then idempotent retry succeeds when audit recovers

  Scenario: Customer self-service appeal
    Given customer's CIDR 192.0.2.0/24 was blocked (Sustained4xx auto-suggest approved)
    When customer POST /v1/admin/blocklist_appeal {cidr, explanation, contact_email}
    Then appeal logged em D1 (audit trail; LGPD compliance)
    Then admin notification SEV-2 (review ≤ 24h)
    Then if approved: CIDR removed from blocklist + appeal_url marker; customer notified
    Then if rejected: appeal closed; customer notified with reason
```

## 9. Design Decisions

- 9.1: CF Ruleset Engine (NOT Worker-level) — pre-Worker enforcement; CPU budget preserved.
- 9.2: 2-tier threshold (anonymous 10 RPS / authenticated 1000 RPS) — NAT-aware (Lote 10.7bis P0-6 lesson generalizado).
- 9.3: D1 source-of-truth + CF List replica (eventual consistency ≤ 5min via reconcile).
- 9.4: IPv4 /32 + IPv6 /64 minimum granularity (RFC 4291 canonical).
- 9.5: Humane automated response (sprint contract §7.10.s08.3 LGPD Art. 20 / GDPR Art. 22).
- 9.6: CIDR overlap rejected (NOT silent merge; admin intent ambiguous).
- 9.7: CF List single-list MVP; shard deferred S-14 (CF 10000 items limit; alert at 8000).
- 9.8: Sustained-abuse detection via CF Logpush → R2 → S-09 analytics (NOT real-time DO; cost-prohibitive at edge volume).
- 9.9: Reconcile DO alarm 5min; re-arm AT START (Lote 10.4bis).
- 9.10: Audit fail-closed pattern (Lote 10.6bis absorbed).
- 9.11: Worker rust API canonical: `worker::send_future()` (Lote 10.7bis R5 P0-3).
- 9.12: D1 batch ≤ 250 rows (Lote 10.5bis canonical).
- 9.13: NEW ADR not required (extends CTRL-RATE-001 + ADR-0020 boundary already canonical).
- 9.14: NetSec advisor substitutes Crypto SME for sign-off (no key material; pure network-layer enforcement).

## 10. Completeness Criteria SOTA

- [ ] **10.s08.002.1** Terraform `cloudflare_ruleset` + `cloudflare_list` apply green em staging zone.
- [ ] **10.s08.002.2** Module compila + integration tests green.
- [ ] **10.s08.002.3** All 10 Gherkin scenarios green.
- [ ] **10.s08.002.4** Property tests 6 × 10k green; **100k nightly sustained 7d** (HIGH_RISK SOTA bar; Lote 10.7bis P1-3).
- [ ] **10.s08.002.5** Chaos suite 11 scenarios green.
- [ ] **10.s08.002.6** **INV-AVAIL-ISOLATION 30d sustained chaos zero violations** at edge layer (sprint contract §6 DoD).
- [ ] **10.s08.002.7** Edge enforcement latency ≤ 1ms p99 (CF Ruleset hardware).
- [ ] **10.s08.002.8** D1 ↔ CF List eventual consistency ≤ 5min sustained.
- [ ] **10.s08.002.9** Métricas (7) emitted; reconcile drift > 10 alerts SEV-2.
- [ ] **10.s08.002.10** Cargo-audit + cargo-deny + clippy clean.
- [ ] **10.s08.002.11** **D1 `ip_blocklist` migration** (NEW table; CHECK constraint inline per Lote 10.5bis).
- [ ] **10.s08.002.12** **D1 `ip_blocklist_suggestions` migration** (NEW table; humane review queue).
- [ ] **10.s08.002.13** Humane SLA: suggested blocks reviewed ≤ 24h (alert if > 24h pending).
- [ ] **10.s08.002.14** False-positive rate ≤ 0.1% authenticated requests blocked falsely (NAT scenarios).
- [ ] **10.s08.002.15** Customer self-service appeal endpoint operational + LGPD audit trail.

## 11. DoD

- [ ] Module compila + tests green; all Gherkin/property/chaos green; 11 sign-offs (HIGH_RISK lower bound; NetSec substitutes Crypto SME).

## 12. Invariants Validated

- **INV-AVAIL-ISOLATION** (HIGH; registry §3.8): edge layer reinforcement; per-IP enforcement = no cross-tenant collateral em adversarial single-IP scenarios.
- **INV-TENANT-ISOLATION** (CRITICAL): edge layer pre-auth; no tenant linkability at edge (LINDDUN privacy).
- **INV-AUDIT-APPEND-ONLY** (CRITICAL, TLA+): inherited via `corelink.audit_log` append-only — all blocklist mutations audited.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| CF Ruleset IaC | `infra/cloudflare/rulesets/per_ip_rate_limit.tf` | Terraform |
| CF List IaC | `infra/cloudflare/rulesets/ip_blocklist.tf` | Terraform |
| EdgeBlocklist module | `crates/corelink-edge-blocklist/` | Rust |
| Reconcile DO impl | `crates/corelink-edge-blocklist/src/reconcile_do.rs` | Rust |
| Admin endpoints (S-13 stub) | `crates/corelink-worker/src/admin/blocklist.rs` | Rust |
| Sustained-abuse detection | `crates/corelink-edge-blocklist/src/sustained_abuse.rs` | Rust |
| D1 migrations | `migrations/00X_ip_blocklist.sql`, `migrations/00X_ip_blocklist_suggestions.sql` | SQL |
| Property tests | `crates/corelink-edge-blocklist/tests/prop_blocklist.rs` | Rust |
| Chaos suite | `tests/chaos_edge_blocklist.rs` | Rust |
| Wrangler DO binding | `wrangler.toml` (additions; `EdgeBlocklistSync` per-region) | TOML |

## 14. Quality Standards SOTA

- 14.s08.002.1: Zero unsafe; zero unwrap em production paths.
- 14.s08.002.2: rustdoc 100% public API.
- 14.s08.002.3: Test coverage ≥ 90%.
- 14.s08.002.4: False-positive rate ≤ 0.1% (NAT-aware calibration).
- 14.s08.002.5: SAST clean.
- 14.s08.002.6: Métricas (7 §6.1.10).
- 14.s08.002.7: Edge enforcement latency ≤ 1ms p99 (CF Ruleset hardware-accelerated).
- 14.s08.002.8: Cost regression gate per-block ≤ $0.0000001 (CF Ruleset Engine cost).
- 14.s08.002.9: TenantCtx N/A at edge (pre-auth); admin endpoints use AdminCtx (S-13); CF Workers Rust API worker::send_future (Lote 10.7bis R5 P0-3); D1 batch ≤ 250 (Lote 10.5bis); CHECK inline (Lote 10.5bis).
- 14.s08.002.10: 100k nightly property test (HIGH_RISK SOTA bar; Lote 10.7bis P1-3).
- 14.s08.002.11: Humane response: 0 auto-applied blocks; ≤ 24h human review SLA.
- 14.s08.002.12: Privacy: IPv4 last octet redacted after 30d; IPv6 last 64 bits redacted after 30d.

## 15. Chaos Experiments (11)

§6.1.12 enumerated.

## 16. PRR

HIGH_RISK 11 sign-offs PRR (lower bound HIGH_RISK 10–12; NetSec substitutes Crypto SME at this WI; sprint contract §6 DoD enforces).

## 17. Sub-tasks

| ID | Sub-task | h |
|---|---|---|
| ST-001 | Terraform CF Ruleset + CF List IaC | 1.5 |
| ST-002 | EdgeBlocklist trait + admin endpoints stubs | 2 |
| ST-003 | D1 migrations (ip_blocklist + ip_blocklist_suggestions) | 1 |
| ST-004 | Reconcile DO + alarm | 2 |
| ST-005 | Sustained-abuse detection (CF Logpush → R2 → S-09) | 2 |
| ST-006 | CIDR normalize + overlap detection (ipnet) | 1 |
| ST-007 | Métricas (7) emit | 0.5 |
| ST-008 | Property tests (6 × 10k; 100k nightly) | 2 |
| ST-009 | Chaos suite (11) | 2 |
| ST-010 | NetSec advisor review (edge enforcement + humane response) | 1 |

**Total**: ~15h. **PERT** O=12h M=14h P=20h: **~15h** (sprint contract estimate 12h; ligeira upper-revision por scope humane appeal LGPD).

## 18. Dependencies

- Hard: S-03 SEALED (AdminCtx middleware); ADR-0020 FROZEN (quota boundary; Lote 10.7bis Phase 3 fix).
- Hard: WI-S08-001 (per-tenant DO RateLimiter; backstop layer when edge per-IP doesn't trigger).
- Soft: S-09 (PagerDuty integration; CF Logpush → R2 analytics); S-13 (admin plane endpoints; staging stub OK); WI-S08-005 (RFC 9331 headers wrap); WI-S08-006 (DASH-RATE consumes metrics).

## 19. Effort PERT: ~15h. ## 20. Time-boxing: 20h hard limit.

## 21. Observability

7 metrics §6.1.10. Trace span `edge_blocklist.{add, remove, suggest, reconcile, appeal}`. CF Logpush → R2 → S-09 for edge enforcement events.

## 22. Cost Analysis

- CF Ruleset Engine: included em CF Workers Unbound plan (no per-rule cost); CF List items free up to 10000 (account-level limit).
- Per-block cost: ~$0.0000001 (CF edge enforcement; far cheaper que Worker invocation $0.0000005).
- Reconcile cost: 1 DO instance per region × 5min alarm × ~10 D1 reads + ~1 CF API call = ~$0.0001/region/month — trivial.
- TCO 12m: 5 regions × $0.0001 × 12 = $0.006/yr — negligible.
- D1 storage: ip_blocklist ~ 1000 entries × 200 bytes = 200 KB — trivial.
- **Cost saved by edge enforcement**: 10k QPS adversarial flood × 24h × $0.0000005 (Worker cost) = $432/day if NOT edge-blocked; this WI saves potentially $$$ in adversarial scenarios.

## 23. API Contract

- Public: `EdgeBlocklist` trait + `BlocklistEntry`, `EdgeBlocklistError` types; `#[non_exhaustive]`.
- HTTP edge: 429 + `Retry-After` + `X-Rate-Limit-Type: per_ip` (CF Ruleset response template).
- HTTP admin: `/v1/admin/blocklist` REST CRUD + suggestions review + appeal; AdminCtx required (S-13).

## 24. Post-mortem Hooks

- INV-AVAIL-ISOLATION violation detected (cross_tenant_violation > 0 attributable to edge gap) → CRITICAL post-mortem.
- Reconcile drift > 10 entries sustained → SEV-2 5-Why.
- Suggested block pending > 24h → SEV-3; humane SLA breach 5-Why (process gap).
- False-positive rate > 0.1% authenticated → SEV-2 calibration review.
- CF API outage > 30min → SEV-2 (no enforcement gap, but reconcile lag).
- Customer self-service appeal approved (false-positive confirmed) → no-blame post-mortem; calibration signal.

## 25. Rollback / Recovery

- Rollback: `terraform destroy -target=cloudflare_ruleset.corelink_edge_rate_limit`; CF Ruleset removed; per-IP enforcement disabled; bulkhead camada 2 lost (cost overhead but no correctness break — camada 1 + 4 backstop).
- Recovery: D1 `ip_blocklist` durable; CF List re-pushed via reconcile on next cycle.
- RTO ≤ 5min; RPO ≤ 0min (D1 durable; CF List eventual ≤ 5min).

## 26. Security & Privacy

**STRIDE**:
- S(poofing): TLS handshake authenticates source IP; HTTPS-only enforced (CF edge); spoofing infeasible.
- T(ampering): D1 ip_blocklist append-only via audit log; reconcile detects drift.
- R(epudiation): audit fail-closed; all admin actions traceable to AdminId.
- I(nformation disclosure): IP redacted after 30d (privacy_model.md PII).
- D(enial of Service): edge layer drops adversarial floods pre-Worker (this WI mitigates DoS at outermost layer).
- E(scalation of Privilege): admin endpoints AdminCtx-gated (S-13).

**LINDDUN**:
- L(inkability): no tenant_id at edge; per-IP only; no cross-tenant linkability.
- I(dentifiability): IP retained 30d operational; redacted thereafter.
- N(on-repudiation): audit log immutable.
- D(etectability): edge logs CF Logpush → R2 (operational visibility).
- D(isclosure): IP redacted in logs after 30d.
- U(nawareness): customer self-service via `/v1/admin/blocklist_appeal` + audit access.
- N(on-compliance): LGPD Art. 20 / GDPR Art. 22 humane automated response (sprint contract §7.10.s08.3).

## 27. Knowledge Transfer

Tech talk (1.5h): "S-08 Edge Per-IP Rate Limit: CF Ruleset + IaC + D1 Sync + Humane LGPD Response"; doc `docs/dev/edge-rate-limit-architecture.md`; onboarding test 6 questions: CF Ruleset Engine vs Worker-level rationale, NAT-aware threshold (Lote 10.7bis P0-6 lesson generalized), IPv6 /64 canonical (RFC 4291), humane response LGPD Art. 20, audit fail-closed pattern, reconcile convergence.

## 28. Risk Register (12-row HIGH_RISK)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | INV-AVAIL-ISOLATION violation via edge gap | L | M | CRITICAL | L | LOW | Defense-in-depth camada 1 + 4; chaos test 30d; SEV-1 alert |
| R-002 | NAT corporate false-positive blocking | M | M | MEDIUM | M | LOW | 2-tier threshold + warn-mode + customer self-service appeal LGPD |
| R-003 | CF List 10000 limit reached | L | L | MEDIUM | L | LOW | Alert SEV-3 at 8000; shard plan deferred S-14 |
| R-004 | CF API outage breaks reconcile | M | L | LOW | L | LOW | Last-known state retained; SEV-2 alert; recovery on resume |
| R-005 | D1 ↔ CF List drift undetected | L | M | MEDIUM | L | LOW | Reconcile nightly + alert > 10 drift |
| R-006 | Sustained-abuse false-positive auto-block | L | L | CRITICAL | L | LOW | Humane response: NEVER auto-apply (LGPD); admin review ≤ 24h |
| R-007 | Audit fail silently → unaudited block | L | M | MEDIUM | L | LOW | Fail-closed bias; reconcile catches; SEV-2 alert |
| R-008 | CIDR overlap silent merge | L | L | MEDIUM | L | LOW | Reject explicit; admin must remove broader first |
| R-009 | IP redaction policy gap (PII retention) | L | L | LOW | L | LOW | privacy_model.md compliance; redact after 30d |
| R-010 | tokio::spawn em CF Workers (compile fail) | L | L | LOW | L | LOW | Lote 10.7bis R5 P0-3 lesson; worker::send_future |
| R-011 | Cost regression CF Ruleset evaluation | L | L | LOW | L | LOW | §14.s08.002.8 gate; CF Ruleset hardware-accelerated |
| R-012 | Admin endpoint S-13 lag (pre-staging dependency) | M | L | MEDIUM | L | LOW | Staging stub OK; full S-13 sealing post-S-13 |

## 29. Review Checkpoints

D+0 design (Architect; edge layer + IaC); D+2 NetSec (CF Ruleset + threat model); D+3 AppSec (audit fail-closed + admin AdminCtx); D+4 code review; D+6 chaos validation; D+7 PRR HIGH_RISK 11 sign-offs.

## 30. Sign-off (HIGH_RISK 11)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver (Gustavo) | _pending_ |
| 3 | SRE Lead | _staffing-blocked; ADR-0034 waiver via Architect compensation_ |
| 4 | Security Lead | _TBD; **mandatory** — INV-AVAIL-ISOLATION + edge layer threat model_ |
| 5-6 | Engineer × 2 | _TBD; **mandatory**_ |
| 7 | QA | _TBD; **mandatory** — chaos + property test 100k_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD; **mandatory emphatic** — LGPD Art. 20 / GDPR Art. 22 humane automated response (sprint contract §7.10.s08.3)_ |
| 10 | Privacy | _TBD; **mandatory** — IP redaction policy + LINDDUN linkability_ |
| 11 | Architect | _TBD; **mandatory** — IaC + edge enforcement boundary_ |
| 12 | NetSec advisor | _**substitutes Crypto SME** — pure network-layer; CF Ruleset + threat model + DDoS pre-mitigation review_ |

(Crypto SME N/A — no key material; substituted by NetSec; HIGH_RISK 11 sign-offs satisfies 10–12 lower bound.)

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (Lote 10.8) | Criação WI-S08-002; HIGH_RISK; SOTA pós-Lote 10.7bis lessons absorbed: NAT-aware (P0-6 generalizado); CF Workers Rust API worker::send_future (R5 P0-3); 100k nightly property test (P1-3); audit fail-closed (Lote 10.6bis); alarm re-arm AT START (Lote 10.4bis); D1 batch ≤250 (Lote 10.5bis); CHECK inline (Lote 10.5bis); column drift no `_ms` suffix (Lote 10.7bis P0-3). Humane response LGPD Art. 20 / GDPR Art. 22 fully integrado (sprint contract §7.10.s08.3 alignment). NEW migrations ip_blocklist + ip_blocklist_suggestions. NetSec advisor substitutes Crypto SME (no cripto load-bearing). |
| 1.1.0 | 2026-04-25 | Gustavo (Lote 10.8bis) | R4+R5 review remediation: P1-3 FM-251 → FM-201 canonical (FM-251 = "Credential stuffing" não "rate FP"); R5 P1-5 CF Terraform schema atualizado para provider v4+ canonical (action_parameters wrapping ratelimit{}; legacy v3 top-level ratelimit{} block deprecated); provider version pin "~> 4.0" added. |
| 1.2.0 | 2026-04-25 | Gustavo (Lote 10.8-tris **SEALED**) | Sonnet R5 round-2 review tris-validation pass: 0 NEW findings em este WI (round-2 tris score 7.8/10 from 7.0 round-1; +0.8 delta). P2 carry-forward to pre-launch advisory: ratio_4xx >= 0.99 (não exact == 1.0); ipnet WASM compatibility gate. **WI sealed pre-implementation**. |

## 32. Anti-patterns evitados

- ❌ Auto-apply suggested blocks (LGPD violation); ❌ Worker-level per-IP (cost overhead); ❌ Hard-coded thresholds; ❌ Plaintext IP retention indefinite (PII); ❌ Skip reconcile drift alert; ❌ tokio::spawn em CF Workers; ❌ Skip alarm re-arm; ❌ CIDR overlap silent merge; ❌ TenantCtx assumption at edge (pre-auth; AdminCtx para admin endpoints).

---

**Fim WI-S08-002.** Próximo: WI-S08-003 (Quota checker middleware atomic D1 CAS — CAP-QUOTA-001 hard-block 100% + CAP-QUOTA-002 bandwidth monthly).
