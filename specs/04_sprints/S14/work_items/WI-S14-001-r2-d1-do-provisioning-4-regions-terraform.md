---
id: "WI-S14-001"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-28"
updated: "2026-04-28"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-002", "FF-HR-003"]
parent: "S-14"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "STORAGE-SEMANTICS-MATRIX"
  - "PRIVACY-MODEL"
  - "SECURITY-MODEL"
  - "RESILIENCE-PATTERNS"
  - "OBSERVABILITY-MODEL"
  - "INVARIANT-REGISTRY"
  - "FAILURE-MODES"
tags: ["wi", "s14", "region", "terraform", "provisioning", "wnam", "enam", "weur", "sam", "high-risk"]
---

# WI-S14-001 — R2 + D1 + DO Provisioning 4 Regions (WNAM/ENAM/WEUR/SAM) via Terraform Module `corelink-region` Reusable + Data Migration Script S-01..S-13 Single-Region → S-14 Multi-Region + RB-region Runbook + Chaos Test Region Outage Cada Região

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-14](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S14-001 |
| Título | Provisionamento de 4 regiões production-grade (WNAM us-west / ENAM us-east / WEUR eu-west / SAM sa-east) via Terraform module `corelink-region` reusable com per-region inputs (`region_name + r2_location_hint + d1_location + do_jurisdiction + cf_zone`); R2 buckets + D1 instances + DO storage provisionados; data migration script Rust S-01..S-13 single-region → S-14 multi-region com dry-run + rollback path; runbook RB-region committed + dry-run; chaos test region outage cada uma das 4 regiões verde |
| Sprint | S-14 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-002 (cross-region tenant isolation surface introduced), FF-HR-003 (residency PII regulatory absoluto Schrems II + LGPD Art. 33) |

## 1. Intent

Provisionar infraestrutura production-grade em 4 regiões com Terraform module reusable (per-region inputs) + migration path S-01..S-13 single-region → S-14 multi-region: (1) **R2 buckets** per-region com `locationHint` ∈ {`wnam`, `enam`, `weur`, `sam`} mapping para Cloudflare regions (us-west / us-east / eu-west / sa-east); (2) **D1 instances** per-region (primary + read replicas opcional) com region affinity; (3) **Durable Objects storage** per-region jurisdiction (DO `jurisdictional_restriction` para EU = `eu`); (4) **Cloudflare zone** + custom domain routing per-region (`{region}.api.corelink.humangr.com` for explicit routing; falls back to `api.corelink.humangr.com` smart routing); (5) **Terraform module `corelink-region`** reusable com per-region inputs + outputs (R2 bucket name + D1 instance ID + DO namespace ID + zone ID); (6) **Data migration script** `scripts/migrate_single_to_multi_region.rs` Rust binary (single-region → multi-region tenant migration; dry-run report; rollback path via Terraform state revert + D1 backup restore); (7) **Runbook RB-region** (provisioning procedure + rollback + migration playbook); (8) **Chaos test region outage** cada uma das 4 regiões (simula CF region partial outage; verify failover routing engages + alerts fire). Foundation layer para WI-S14-002..009.

```hcl
# File: infra/terraform/modules/corelink-region/main.tf

variable "region_name" {
  type        = string
  description = "Region identifier (wnam/enam/weur/sam)"
  validation {
    condition     = contains(["wnam", "enam", "weur", "sam"], var.region_name)
    error_message = "region_name must be one of: wnam, enam, weur, sam"
  }
}

variable "r2_location_hint" {
  type        = string
  description = "R2 bucket location hint (wnam/enam/weur/sam mapped to CF storage regions)"
}

variable "d1_location" {
  type        = string
  description = "D1 instance primary location (wnam/enam/weur/sam)"
}

variable "do_jurisdiction" {
  type        = string
  description = "DO jurisdictional restriction (none/eu/us)"
  validation {
    condition     = contains(["none", "eu", "us"], var.do_jurisdiction)
    error_message = "do_jurisdiction must be one of: none, eu, us"
  }
}

variable "cf_zone_id" {
  type        = string
  description = "Cloudflare zone ID for {region}.api.corelink.humangr.com"
}

resource "cloudflare_r2_bucket" "corelink_cas" {
  account_id   = var.cf_account_id
  name         = "corelink-cas-${var.region_name}"
  location     = var.r2_location_hint
}

resource "cloudflare_d1_database" "corelink_meta" {
  account_id = var.cf_account_id
  name       = "corelink-meta-${var.region_name}"
  # location pinned via cloudflare-go provider extension
}

resource "cloudflare_workers_kv_namespace" "corelink_session" {
  account_id = var.cf_account_id
  title      = "corelink-session-${var.region_name}"
  # NOTE: KV is global; per-region namespace prefix prevents FM-054 cross-region leak
}

# DO with jurisdictional restriction
resource "cloudflare_workers_script" "corelink_do_region" {
  account_id  = var.cf_account_id
  name        = "corelink-do-${var.region_name}"
  module      = true
  content     = file("${path.module}/do_region.js")
  
  # Jurisdictional restriction set via Worker binding metadata
  # do_jurisdiction = var.do_jurisdiction (eu | us | none)
}

output "r2_bucket_name" {
  value = cloudflare_r2_bucket.corelink_cas.name
}

output "d1_instance_id" {
  value = cloudflare_d1_database.corelink_meta.id
}

output "do_namespace_id" {
  value = cloudflare_workers_script.corelink_do_region.id
}

output "zone_id" {
  value = var.cf_zone_id
}
```

```rust
// File: scripts/migrate_single_to_multi_region.rs

#![forbid(unsafe_code)]

use anyhow::Result;
use clap::Parser;
use serde::{Deserialize, Serialize};

#[derive(Parser, Debug)]
#[command(name = "migrate_single_to_multi_region")]
struct Args {
    #[arg(long, default_value = "true")]
    dry_run: bool,

    #[arg(long, value_enum)]
    target_region: Region,

    #[arg(long)]
    tenant_id_filter: Option<String>,

    #[arg(long, default_value = "false")]
    rollback: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, clap::ValueEnum)]
enum Region {
    Wnam,
    Enam,
    Weur,
    Sam,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    
    if args.rollback {
        rollback_migration(args).await?;
    } else if args.dry_run {
        dry_run_migration(args).await?;
    } else {
        execute_migration(args).await?;
    }
    Ok(())
}
```

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

Region provisioning é foundation layer cripto-load-bearing operacional crítico para S-14: cada decisão de Terraform / R2 location hint / D1 location / DO jurisdiction governs onde tenant data lives + onde failover replicas are stored + onde audit chain segments persist + onde BYOK envelope ciphertext rests. Bug em provisioning = INV-DATA-RESIDENCY violation (CRITICAL Schrems II + LGPD Art. 33) potencial; bug em migration = blob/AC/billing mis-routed ou mass tenant downtime.

**Bugs catastróficos possíveis** (todos endereçados):

1. **R2 location hint não respeitado pelo Cloudflare**: `locationHint` é hint não guarantee; CF may store em outra região por capacity reasons. Mitigação: WI-S14-002 insert checks validate stored location via R2 metadata API per-write; mismatch = audit emit + alert SEV-2; quarterly config audit.

2. **D1 location drift**: D1 instance criado em região X mas migrated por CF backend para região Y por capacity. Mitigação: D1 location verification via Workers binding metadata check daily; alert se drift detected; INV-DATA-RESIDENCY enforcement at app-level (insert checks WI-S14-002 valida).

3. **DO jurisdiction not set**: DO sem `jurisdictional_restriction = "eu"` para WEUR = potential EU data on US infrastructure. Mitigação: Terraform validation rule + post-deploy script `verify_do_jurisdiction.sh` runs against deployed DOs; PR fail se mismatch.

4. **KV global namespace cross-region leak (FM-054)**: KV is globally distributed; tenant cache miss em WEUR served by KV node em WNAM. Mitigação: per-region KV namespace prefix (`corelink-session-{region}`); KV reads scope to per-region namespace only; runbook RB-FM-054 dry-run em WI-S14-009.

5. **Migration single-region → multi-region partial failure**: half migration succeeds; tenant data split across regions. Mitigação: migration script transactional (D1 + R2 atomic per-tenant; failure rolls back); dry-run report identifies issues; rollback path via Terraform state revert + D1 backup restore (point-in-time recovery).

6. **Terraform state corruption**: state file lost ou corrompido = re-provisioning rebuild infrastructure (data loss potential). Mitigação: Terraform state em CF R2 backend com versioning + 90d retention; state lock via DynamoDB-equivalent (CF KV with consistent ops); backup state daily.

7. **Custom domain routing mismatch**: `{region}.api.corelink.humangr.com` certificate mismatch causing 525 errors. Mitigação: Terraform manages CF SSL certs + DNS records; integration test post-deploy validates custom domain routing per-region.

8. **Region outage propagates global** (FM-region-outage): WEUR outage causes WNAM tenants to see degraded service. Mitigação: chaos test region outage per-region; failover routing PAT-REGION-FAILOVER-001 (WI-S14-003) ensures isolation; per-region D1 + DO + R2 = no shared dependency.

**Atacante adversarial scenarios**:

- **Force tenant to wrong region via subdomain spoofing**: attacker tries `weur.api.corelink.humangr.com` for ENAM tenant. Mitigação: insert checks validate `tenant.primary_region == request.region` (WI-S14-002); mismatch = 403 + audit emit `corelink.region.cross_region_read_blocked`.

- **Inject migration script to cross-region**: attacker compromises CI; runs migration script with malicious target_region. Mitigação: migration script requires admin role + dual-approval (S-13) + WebAuthn UV=1 step-up; dry-run mandatory; rollback path tested.

- **Tamper with Terraform state**: attacker modifies state file to swap regions silently. Mitigação: state file em R2 versioning + integrity hash + Cosign-signed plan apply (S-12 inheritance).

**Risk justification HIGH_RISK**:

- **FF-HR-002**: cross-region tenant isolation surface is introduced; bug = cross-region leak Schrems II violation.
- **FF-HR-003**: residency PII regulatory absoluto; misrouting EU data to US = breach notification + €€€ fines.
- **Reversibility**: migration rollback ≤ 4h via Terraform state revert + D1 PITR; mas data already cross-routed = legal exposure permanent.

11 sign-offs canonical incl. Architect (multi-region architecture review + Terraform module + state management) + Security Lead (insert checks + IAM + state file integrity) + SRE Lead (region outage chaos test + RB-region) + Compliance Officer (Schrems II + LGPD Art. 33 attestation).

## 3. Customer Impact & Journey

**Persona 1 — Customer EU (financial services / healthcare)**:
- Signup com `primary_region: weur`; dado lands em WEUR R2 + WEUR D1 + WEUR DO; KV reads scoped per-region namespace.
- Evidence: insert check audit chain shows `tenant_id → primary_region: weur`; quarterly config audit verifies R2 bucket location + D1 location + DO jurisdiction set.

**Persona 2 — Auditor SOC 2 + Schrems II + LGPD**:
- CTRL-PRIV-031 attestation; Terraform plan + apply Cosign-signed; state file integrity verified; quarterly config audit report committed.
- Evidence pack: 4 regions provisioned via reusable module; per-region R2 + D1 + DO + KV namespace; chaos test region outage per-region green; RB-region dry-run.

**Persona 3 — Internal SRE on-call**:
- RB-region committed: provisioning procedure + rollback + migration playbook; chaos test region outage per-region green; alerts wire correctly per-region.

**SLA addendum**:
- Region provisioning ≤ 4h end-to-end (Terraform apply + DNS propagation + verification).
- Migration single-region → multi-region tenant ≤ 1h per tenant (D1 row migration + R2 copy + verify).
- Region outage chaos test verde per-region (4 regions × 1 outage scenario each).
- Quarterly config audit per-region (R2 + D1 + DO + KV namespace).
- Terraform state file backup daily 90d retention.

## 4. Capability Mapping

- **CAP-REGION-001** (4 regiões operantes) — IMPLEMENTA primary.
- Trace: `_spec_contract.md §4 + §5.1` + `privacy_model.md` (CTRL-PRIV-031 residency) + `storage_semantics_matrix.md` (multi-region storage model) + `resilience_patterns.md` (PAT-REGION-FAILOVER-001 foundation) + `failure_modes.md` (FM-054 KV global leak prevention via per-region namespace).

## 5. Tipo

Infrastructure-as-Code + migration script + runbook; HIGH_RISK; FF-HR-002 + FF-HR-003.

## 6. Escopo

### 6.1 In-scope

1. **Terraform module `infra/terraform/modules/corelink-region/`**:
   - `main.tf` — 4 resources (R2 bucket + D1 instance + DO Worker + KV namespace per-region) + custom domain DNS + SSL cert.
   - `variables.tf` — 5 inputs (region_name + r2_location_hint + d1_location + do_jurisdiction + cf_zone_id) + 1 implicit (cf_account_id).
   - `outputs.tf` — 4 outputs (R2 bucket name + D1 instance ID + DO namespace ID + zone ID).
   - Validation rules per-input (region_name ∈ enum; do_jurisdiction ∈ {none, eu, us}).

2. **Per-region invocations** (`infra/terraform/regions/`):
   - `wnam.tf`: `module "wnam" { source = "../modules/corelink-region"; region_name = "wnam"; r2_location_hint = "wnam"; d1_location = "wnam"; do_jurisdiction = "us"; ... }`.
   - `enam.tf`: similar.
   - `weur.tf`: `do_jurisdiction = "eu"` (mandatory para EU customer).
   - `sam.tf`: similar.
   - All 4 invocations apply via `terraform apply` (dry-run via `terraform plan` mandatory pre-apply).

3. **Migration script `scripts/migrate_single_to_multi_region.rs`** (Rust binary):
   - Args: `--dry-run` (default true) + `--target-region` + `--tenant-id-filter` + `--rollback`.
   - Dry-run mode: report tenants to migrate (D1 row count + R2 blob count + estimated duration).
   - Execute mode: per-tenant transactional [D1 row migration + R2 copy + verify hash] + audit emit per-tenant.
   - Rollback mode: revert Terraform state + D1 PITR restore + R2 backup restore.
   - Idempotent (re-run safe).
   - Bounded duration (per-tenant ≤ 1h; full migration ≤ 8h).

4. **Runbook `specs/05_runbooks/RB-region.md`**:
   - Provisioning procedure (per-region Terraform apply + verification).
   - Rollback procedure (Terraform state revert + D1 PITR + R2 backup restore).
   - Migration playbook (single-region → multi-region; dry-run → execute → verify).
   - Region outage incident response.
   - Quarterly config audit checklist.

5. **Chaos test region outage cada região**:
   - Synthetic region partial outage (CF region degraded service simulation via traffic blackhole).
   - Verify: failover routing engages (WI-S14-003 PAT-REGION-FAILOVER-001); alerts fire SEV-2; on-call paged; runbook commands executable.
   - Per-region (4 scenarios): WNAM outage → WNAM tenants degraded; ENAM outage; WEUR outage; SAM outage.
   - Output: `specs/_audits/2026-XX-XX-region-outage-chaos-s14.md` com timeline + drift findings.

6. **Métricas underscored Prometheus** (per `observability_model.md §3.1`; label `plan` aplicável):
   - `corelink_region_provisioning_duration_seconds_bucket{region}` (histogram).
   - `corelink_region_health_status{region}` (gauge; 0=down/1=degraded/2=healthy).
   - `corelink_region_terraform_drift_findings_total{region, severity}` (counter).
   - `corelink_region_migration_progress_ratio{tenant_id_hash, source_region, target_region}` (gauge during migration; tenant_id hashed for cardinality budget).
   - **NÃO** `corelink_region_health{tenant_id}` — tenant_id label proibido per INV-OBS-CARDINALITY-BUDGET.

7. **Observability** — trace spans `region.{provision, migrate, verify, rollback, chaos_outage}` com attributes:
   - `region.name` (enum).
   - `region.r2_bucket_name`, `region.d1_instance_id`, `region.do_namespace_id`.
   - `region.do_jurisdiction` (enum).
   - `result` (enum).

8. **Audit emission** — CloudEvent per provisioning/migration step (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER):
   - `corelink.region.provisioned` (per-region Terraform apply success).
   - `corelink.region.migration.started` / `corelink.region.migration.tenant_completed` / `corelink.region.migration.completed`.
   - `corelink.region.outage.detected` / `corelink.region.outage.resolved`.

9. **Integration tests E2E**:
   - Terraform plan + apply 4 regions (staging account).
   - Verify R2 bucket location matches `r2_location_hint` per-region.
   - Verify D1 instance accessible per-region.
   - Verify DO `jurisdictional_restriction` set correctly (eu for WEUR).
   - Verify KV namespace per-region prefix prevents cross-region access.
   - Custom domain `{region}.api.corelink.humangr.com` routing test per-region.

10. **Adversarial tests**:
    - Subdomain spoofing: attacker requests `weur.api.corelink.humangr.com` with ENAM tenant_id → WI-S14-002 insert checks reject 403.
    - Migration script malicious target_region: requires admin role + dual-approval + WebAuthn UV=1 (S-13 herdada).
    - Terraform state file tampering: state in R2 versioning + integrity hash; tamper detected.

### 6.2 Out-of-scope (deferred)

- **Tenant region pinning enforcement** (insert checks + 30k property test): WI-S14-002.
- **Hot blob replica + read failover** (PAT-REGION-FAILOVER-001): WI-S14-003.
- **BYOK adapters** (AWS/GCP/Azure/Vault): WI-S14-004 + WI-S14-005.
- **CMK kill switch + erasure attestation**: WI-S14-006 + WI-S14-007.
- **DPA + Schrems II TIA**: WI-S14-008.
- **TLA+ region_residency + pentest + PRR**: WI-S14-009.
- **APAC/AFR regions**: pós-GA demand-driven.
- **Active-active multi-region writes**: Fase 2 (primary-only at GA).
- **Per-tenant region migration self-service**: manual ticket only at GA.

## 7. Anti-Scope

- Skip Terraform module reusability (per-region copy-paste = drift).
- Skip migration dry-run report (production migration without dry-run = catastrophic).
- Skip rollback path (one-way migration = unrecoverable).
- Skip chaos test region outage per-region (operational unreadiness).
- Skip per-region KV namespace prefix (FM-054 cross-region leak).
- Skip DO `jurisdictional_restriction` for WEUR (Schrems II violation).
- Skip Terraform state versioning + integrity (state corruption = re-provisioning).
- Skip custom domain SSL cert per-region (525 errors).
- Auto-apply Terraform sem PR review + Cosign signature (S-12 herdada).
- Direct R2/D1/DO modification bypass Terraform (drift).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: WI-S14-001 — 4 regions provisioning + Terraform module + migration + chaos test

  Background:
    Given Cloudflare account configured
    Given Terraform module corelink-region v1.0 ready
    Given staging environment isolated

  Scenario: 4 regions Terraform apply
    When `terraform apply -var-file=regions/wnam.tfvars + enam.tfvars + weur.tfvars + sam.tfvars` runs
    Then 4 R2 buckets provisioned (corelink-cas-{region})
    And 4 D1 instances provisioned (corelink-meta-{region})
    And 4 DO Workers provisioned with do_jurisdiction set
    And 4 KV namespaces with per-region prefix
    And 4 custom domain DNS records ({region}.api.corelink.humangr.com)
    And SSL certs valid per-region

  Scenario: WEUR DO jurisdictional restriction enforced
    Given Terraform apply WEUR with do_jurisdiction = "eu"
    When `cloudflare api get DO worker metadata` queries
    Then jurisdictional_restriction = "eu" returned
    And no US data path possible
    And SOC 2 + GDPR Art. 46 compliance attested

  Scenario: KV namespace per-region prevents FM-054 cross-region leak
    Given KV namespace `corelink-session-wnam` and `corelink-session-weur`
    When tenant in WEUR writes to KV
    Then write goes to corelink-session-weur namespace only
    And WNAM Workers cannot read from corelink-session-weur
    And per-region scoping enforced em Worker binding

  Scenario: R2 location hint verification
    Given R2 bucket `corelink-cas-weur` with location_hint = "weur"
    When `wrangler r2 bucket info corelink-cas-weur` queries
    Then location reports as "weur"
    And actual storage region matches hint
    And quarterly config audit verifies match

  Scenario: Migration single-region → multi-region dry-run
    Given migration script with --dry-run --target-region weur
    When script runs against staging tenant set
    Then report lists tenants to migrate (D1 row count + R2 blob count)
    And estimated duration reported
    And no actual migration occurs
    And dry-run report committed em audit folder

  Scenario: Migration execute mode transactional
    Given dry-run report approved
    When script runs --execute --target-region weur --tenant-id-filter "eu_*"
    Then per-tenant atomic [D1 row migration + R2 copy + verify hash]
    And audit emit per-tenant migration completed
    And idempotent re-run safe (no duplicate migration)

  Scenario: Migration rollback path
    Given migration completed with errors em 5% tenants
    When script runs --rollback
    Then Terraform state reverted
    And D1 PITR restore previous snapshot
    And R2 backup restore previous bucket state
    And RTO ≤ 4h achieved

  Scenario: Chaos test region outage WNAM
    Given staging 4 regions live + tenants distributed
    When chaos test simulates WNAM region partial outage
    Then WNAM tenants see degraded service
    And ENAM/WEUR/SAM unaffected (region isolation)
    And alerts fire SEV-2
    And on-call paged via PagerDuty test channel
    And runbook RB-region commands executable
    And failover routing engages per WI-S14-003

  Scenario: Chaos test region outage cada região verde
    Given 4 chaos test scenarios (WNAM/ENAM/WEUR/SAM each)
    When all 4 scenarios run sequentially
    Then 4/4 scenarios green
    And per-region isolation validated
    And report committed em audit folder

  Scenario: Custom domain {region}.api.corelink.humangr.com routing
    Given DNS records + SSL certs provisioned per-region
    When client requests https://weur.api.corelink.humangr.com/v1/cas/get
    Then request routed to WEUR Worker
    And SSL cert valid (no 525 error)
    And Worker binding scopes to WEUR R2 + D1 + DO

  Scenario: Terraform state versioning + integrity
    Given state file em R2 backend with versioning
    When state file modified out-of-band
    Then integrity hash mismatch detected
    And `terraform plan` fails until reconciled
    And alert SEV-2 fires
```

## 9. Design Decisions

### 9.1 Why Terraform module reusable (NÃO per-region copy-paste)

- Drift over time inevitable em copy-paste; module ensures consistency.
- Per-region inputs encapsulate variation; outputs typed.
- Validation rules em variables prevent invalid combinations.
- Future regions (APAC/AFR) trivially added via new invocation.

### 9.2 Why per-region KV namespace prefix (NÃO single global namespace)

- KV is globally distributed by CF design; FM-054 cross-region leak prevention requires explicit scoping.
- Per-region namespace `corelink-session-{region}` enforced em Worker binding.
- INV-REGION-NO-CROSS-LEAK requires this baseline.

### 9.3 Why DO `jurisdictional_restriction` for WEUR (NÃO global routing)

- Schrems II + GDPR Art. 46: EU data must not transit non-EU infrastructure.
- DO without restriction may route to US edge per CF capacity.
- `jurisdictional_restriction = "eu"` enforces EU-only routing for WEUR DOs.

### 9.4 Why migration script Rust (NÃO bash)

- Type safety + structured error handling + bounded memory + audit emission.
- Pattern reused from S-12 (rb_fm_156_dry_run.rs).
- Idempotent re-run safe via D1 transaction + R2 versioning.

### 9.5 Why dry-run mandatory pre-execute

- Migration is destructive (D1 row migration + R2 copy + delete source).
- Dry-run report identifies tenants + estimated duration + edge cases.
- Approval gate before execute prevents catastrophic migration.

### 9.6 Why custom domain `{region}.api.corelink.humangr.com` (NÃO smart routing only)

- Smart routing implicit (CF chooses region); customer cannot verify routing.
- Explicit `{region}.api.corelink.humangr.com` allows customer-side verification.
- Falls back to `api.corelink.humangr.com` smart routing for non-region-aware clients.

### 9.7 Why chaos test region outage cada região (NÃO single representative)

- Per-region failure modes vary (latency profile, neighbor regions, capacity).
- 4 chaos tests = full coverage; representative test = blind spots.
- Cost: ~2h staging time per test = 8h total; acceptable for HIGH_RISK lane.

### 9.8 Why ADR potencial?

- Sim — **ADR-XXXX**: "Multi-region Terraform module + migration script + per-region KV namespace prefix S-14". Decisão arquitetural foundational; reuse pattern em APAC/AFR forward.
- Whitelist em `validate_references.py` até materializar.

## 10. Completeness Criteria SOTA

- [ ] **10.s14.001.1** Terraform module `corelink-region` reusable + 4 regions invocations + apply success em staging (EVT-013).
- [ ] **10.s14.001.2** R2 location hint verification per-region (4 regions); audit committed (EVT-002).
- [ ] **10.s14.001.3** D1 location verification per-region; daily drift check + alert (EVT-013).
- [ ] **10.s14.001.4** DO jurisdictional_restriction = "eu" for WEUR validated; SOC 2 + GDPR attestation (EVT-044).
- [ ] **10.s14.001.5** KV namespace per-region prefix enforced em Worker binding; FM-054 cross-region leak prevented (EVT-002).
- [ ] **10.s14.001.6** Migration script Rust binary + dry-run + execute + rollback paths green em staging (EVT-024).
- [ ] **10.s14.001.7** Runbook RB-region committed + dry-run executed (EVT-017).
- [ ] **10.s14.001.8** Chaos test region outage cada região (4 scenarios) verde; reports committed (EVT-023).
- [ ] **10.s14.001.9** Custom domain `{region}.api.corelink.humangr.com` routing test verde; SSL certs valid per-region (EVT-013).
- [ ] **10.s14.001.10** Terraform state em R2 versioning + integrity hash + 90d retention (EVT-002).
- [ ] **10.s14.001.11** Cost regression gate: 4 regions infra ≤ $800/mês (4× R2 + 4× D1 + 4× DO).
- [ ] **10.s14.001.12** SOC 2 CC6.1 + Schrems II + LGPD Art. 33 attestation em PRR doc (EVT-044).

## 11. DoD

- [ ] Terraform module + 4 region invocations applied em staging.
- [ ] R2 + D1 + DO + KV namespace per-region verified.
- [ ] DO jurisdictional_restriction set correctly per-region.
- [ ] Custom domain routing per-region operational.
- [ ] Migration script dry-run + execute + rollback paths green.
- [ ] RB-region committed + dry-run.
- [ ] Chaos test region outage 4/4 scenarios green.
- [ ] Métricas + trace spans + audit emission operational.
- [ ] ADR-XXXX (multi-region module) escrito + ratificado.
- [ ] Cost regression gate green.
- [ ] Code review (Architect + SRE Lead + Security Lead).
- [ ] PRR Architect + Compliance mini-sign-off (ship gate é WI-S14-009).

## 12. Invariants Validated

### Mantidas

- **INV-DATA-RESIDENCY** (CRITICAL — registry §3.11 herdada): Terraform module enforces R2 location_hint + D1 location + DO jurisdiction; quarterly config audit + WI-S14-002 insert checks runtime enforcement.
- **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER** (CRITICAL — registry §3.14 herdada S-03): provisioning audit emit em D1 atomic batch.

### Novas

Nenhuma neste WI (foundation; INVs novas em WI-S14-002/006/007).

TLA+ alignment: registry §4.2 indica `region_residency.tla` PLANNED S-14 WI-S14-009; este WI provê foundation infra.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Terraform module corelink-region | `infra/terraform/modules/corelink-region/main.tf + variables.tf + outputs.tf` | HCL |
| Per-region invocations | `infra/terraform/regions/{wnam,enam,weur,sam}.tf + .tfvars` | HCL |
| Migration script | `scripts/migrate_single_to_multi_region.rs` | Rust binary |
| Runbook RB-region | `specs/05_runbooks/RB-region.md` | Markdown |
| Chaos test region outage | `tests/chaos_region_outage.rs` (4 scenarios) | Rust |
| Chaos test reports | `specs/_audits/2026-XX-XX-region-outage-chaos-s14.md` | Markdown |
| ADR-XXXX (multi-region module) | `specs/03_architecture/adrs/ADR-XXXX-multi-region-terraform-module.md` | Markdown |
| Integration tests E2E | `tests/e2e_4_regions_provisioning.rs` | Rust |
| Verify scripts | `scripts/verify_r2_location.sh + verify_d1_location.sh + verify_do_jurisdiction.sh` | Bash |

## 14. Quality Standards SOTA

- **14.s14.001.1** Zero `unsafe`; zero `unwrap` em src/.
- **14.s14.001.2** Terraform plan + apply Cosign-signed (S-12 herdada).
- **14.s14.001.3** Test coverage ≥ 90% migration script + verify scripts.
- **14.s14.001.4** Latência: Terraform apply ≤ 4h end-to-end; migration per-tenant ≤ 1h.
- **14.s14.001.5** SAST: cargo-audit + cargo-deny + clippy `-D warnings` clean.
- **14.s14.001.6** Métricas region health + provisioning duration.
- **14.s14.001.7** Runbook RB-region committed + dry-run.
- **14.s14.001.8** Breaking changes em Terraform module = bump major + ADR + migration plan.
- **14.s14.001.9** Memory bounded em migration script (streaming R2 copy).
- **14.s14.001.10** Cost regression gate em CI.
- **14.s14.001.11** SOC 2 CC6.1 + Schrems II + LGPD Art. 33 attestation.
- **14.s14.001.12** Quarterly config audit cadence documented.

## 15. Chaos Experiments

1. **Region outage WNAM**: simulate CF region partial outage; verify ENAM/WEUR/SAM unaffected; failover engages.

2. **Region outage ENAM**: same pattern.

3. **Region outage WEUR**: same pattern; verify EU data path failover respects jurisdiction.

4. **Region outage SAM**: same pattern.

5. **Terraform state corruption**: tamper state file; verify integrity hash detects + plan fails + alert.

6. **DO jurisdiction drift**: simulate DO migrated to non-EU edge; verify daily check detects + alert.

7. **D1 location drift**: simulate D1 instance migrated to non-target region; verify daily check detects + alert.

8. **Migration partial failure**: inject 5% tenant migration failures; verify rollback path; idempotent re-run.

9. **KV cross-region leak attempt**: WNAM Worker tries read corelink-session-weur; verify access denied via per-region scoping.

10. **Custom domain SSL drift**: simulate cert expiry; verify alert + auto-renewal.

## 16. PRR (Production Readiness Review)

PRR HIGH_RISK 11 sign-offs canonical (S-14 ship gate é WI-S14-009; este WI passa por mini-PRR Architect + Security Lead + SRE Lead + Compliance Officer review):

- [ ] All 10 Gherkin scenarios green.
- [ ] Chaos test 4/4 region outage scenarios green.
- [ ] Migration script dry-run + execute + rollback paths green.
- [ ] RB-region dry-run executed.
- [ ] Cost regression gate green.
- [ ] Métricas + dashboards configurados em DASH-REGION.
- [ ] ADR-XXXX (multi-region module) published.
- [ ] SOC 2 CC6.1 + Schrems II + LGPD Art. 33 attestation.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | Terraform module corelink-region scaffold + variables + outputs | 3h |
| ST-002 | Per-region invocations 4 regions + .tfvars | 2h |
| ST-003 | DO jurisdictional_restriction wiring + verify script | 2h |
| ST-004 | KV namespace per-region prefix + Worker binding scope | 2h |
| ST-005 | Custom domain DNS + SSL cert per-region | 2h |
| ST-006 | Migration script Rust scaffold + dry-run mode | 3h |
| ST-007 | Migration script execute mode + transactional | 3h |
| ST-008 | Migration script rollback mode + Terraform revert + D1 PITR | 2.5h |
| ST-009 | Runbook RB-region escrita + dry-run | 2h |
| ST-010 | Chaos test region outage 4 scenarios | 4h |
| ST-011 | Métricas emit + trace spans + audit emission | 2h |
| ST-012 | Verify scripts (R2 location + D1 location + DO jurisdiction) | 1.5h |
| ST-013 | Integration tests E2E 4 regions | 3h |
| ST-014 | ADR-XXXX redação | 2h |
| ST-015 | Code review (Architect + Security Lead + SRE Lead + Compliance) iteration | 3h |

**Total Optimistic**: ~37h. **PERT** (O=18h, M=28h, P=44h, per spec contract §12): **28.7h**. Sub-tasks soma é detail-grain; PERT spec contract é consolidated.

## 18. Dependencies

### Hard blockers

- **S-01..S-13 SEALED** (foundation product working; admin plane ready).
- Cloudflare account with R2 + D1 + DO + Workers + Custom Domains enabled.
- Terraform 1.6+ + cloudflare-go provider 4.x+.
- Staging environment isolated.

### Soft blockers

- DNS records propagation (~15-30 min).
- SSL cert issuance (~5-10 min).

### Outbound

- **WI-S14-002** (tenant region pinning consume primary_region D1 column).
- **WI-S14-003** (hot blob replica + failover routing consume per-region R2 + D1).
- **WI-S14-004..007** (BYOK + erasure attestation consume per-region infra).
- **WI-S14-009** (TLA+ + pentest + PRR consume foundation).

## 19. Effort PERT

O: 18h, M: 28h, P: 44h → PERT **28.7h** (per spec contract §12).

## 20. Time-boxing

**32h hard limit owner**. If exceeded → escalation: split em sub-WI (Terraform module vs migration script vs chaos test).

## 21. Observability

5 métricas listadas §6.1.6. Trace spans em §6.1.7. Logs structured JSON; nivel INFO em ok, WARN em drift detected, ERROR em rollback.

Dashboard widget DASH-REGION:
- Region health per-region (4-panel WNAM/ENAM/WEUR/SAM availability).
- Region provisioning duration (gauge).
- Region migration progress ratio (gauge during active migration).
- Terraform drift findings per region (alert > 0).
- Region outage events count.

## 22. Cost Analysis

- 4× R2 buckets: ~$5/mês each = $20/mês baseline storage.
- 4× D1 instances: ~$10/mês each = $40/mês.
- 4× DO Workers: ~$15/mês each = $60/mês.
- 4× KV namespaces: ~$5/mês each = $20/mês.
- 4× Custom domains: included em CF Pro.
- 4× SSL certs: included em CF.
- Cross-region replication egress: ~$0.10/GB × top 1% = bounded.
- **Total custo direto WI-S14-001**: ~$140/mês baseline + workload-dependent ~$800/mês total infra. Bounded vs alternative (multi-cloud setup ~$5k+/mês).

## 23. API Contract

Não-aplicável (este WI é IaC + script; não introduz API surface). Dependent APIs em WI-S14-002 (tenant region pinning) + WI-S14-003 (failover routing).

## 24. Post-mortem Hooks

- Region outage > 30 min sustained → SEV-2 post-mortem.
- Terraform state corruption → SEV-2 + integrity hash review.
- Migration partial failure unrecoverable → CRITICAL post-mortem + Customer Trust review.
- DO jurisdiction drift detected production → CRITICAL + Schrems II review.
- D1 location drift > 5% tenants → SEV-2 + capacity planning review.

## 25. Rollback / Recovery

- Code rollback: revert PR + redeploy Worker.
- Infrastructure rollback: Terraform state revert + D1 PITR restore + R2 backup restore.
- Migration rollback: script `--rollback` mode.
- RTO: ≤ 4h (Terraform + D1 + R2 restore).
- RPO: 0 (D1 PITR + R2 versioning preserve all data).

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: Terraform IAM scoped via CF API token; subdomain spoofing defended by WI-S14-002 insert checks.
- **Tampering**: Terraform state em R2 versioning + integrity hash; migration script audit emit per-step.
- **Repudiation**: provisioning + migration events em audit chain (S-09 inheritance).
- **Information disclosure**: per-region KV namespace prefix prevents FM-054; DO jurisdiction enforces residency.
- **DoS**: chaos test validates region outage isolation; failover engages.
- **Elevation of privilege**: Terraform apply requires Cosign-signed plan (S-12 inheritance) + admin role + dual-approval (S-13 inheritance).

**LINDDUN delta**:
- **Linkability**: tenant_id em audit (compliance accountability).
- **Identifiability**: region em audit (intentional Schrems II evidence).
- **Non-repudiation**: provisioning + migration audit chain unbroken.
- **Detectability**: drift detection daily + alert.
- **Disclosure**: residency commitment per region documented em DPA (WI-S14-008).
- **Unawareness**: region selection at signup explicit.
- **Non-compliance**: SOC 2 CC6.1 + Schrems II + LGPD Art. 33 + GDPR Art. 46 satisfied.

## 27. Knowledge Transfer

- `infra/terraform/modules/corelink-region/README.md` — module overview + per-region inputs.
- ADR-XXXX — multi-region module ratification.
- Doc `docs/internal/multi-region-byok.md` (region section) — per-region architecture diagram.
- Workshop interno (1.5h) com Architect + SRE Lead + Compliance pós-merge.
- Onboarding test (5 questions): per-region KV namespace, DO jurisdiction, R2 location hint, migration dry-run, rollback procedure.

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | R2 location hint não respeitado | M | M | HIGH | M | LOW | Daily verify + insert check WI-S14-002 + audit alert |
| R-002 | D1 location drift | L | M | HIGH | L | LOW | Daily check + alert + insert check |
| R-003 | DO jurisdiction não set | L | H | CRITICAL | M | LOW | Terraform validation + post-deploy verify + CI gate |
| R-004 | KV global namespace cross-region leak (FM-054) | M | M | HIGH | M | LOW | Per-region namespace prefix + Worker binding scope + property test 30k WI-S14-002 |
| R-005 | Migration partial failure | M | M | HIGH | M | LOW | Transactional + dry-run + rollback path + idempotent |
| R-006 | Terraform state corruption | L | M | HIGH | L | LOW | R2 versioning + integrity hash + 90d retention + state lock |
| R-007 | Custom domain SSL drift | L | L | MEDIUM | L | LOW | Auto-renewal + alert SEV-3 |
| R-008 | Region outage propagates global | L | M | HIGH | L | LOW | Per-region isolation + chaos test verde + PAT-REGION-FAILOVER-001 WI-S14-003 |
| R-009 | Subdomain spoofing | L | L | MEDIUM | L | LOW | Insert checks WI-S14-002 + audit emit |
| R-010 | CF API rate limit em apply massivo | M | L | LOW | L | LOW | Throttle + retry + Terraform parallelism limit |
| R-011 | Chaos test impacta production | L | H | HIGH | L | LOW | Staging-only + test isolation + production-isolated infra |
| R-012 | Cost regression em multi-region | M | L | MEDIUM | L | LOW | Cost regression gate + benchmark + monthly budget review |

## 29. Review Checkpoints

1. **Design (D+0)**: Architect + Security Lead review module design + state management.
2. **Terraform draft (D+1)**: SRE Lead pair-program module + per-region invocations.
3. **Migration script (D+2)**: Engineer pair-program + Architect review transactional pattern.
4. **Code (D+3)**: peer review + adversarial test (subdomain spoofing).
5. **Security (D+3)**: Security Lead review IAM scoping + state file integrity.
6. **Compliance (D+4)**: Compliance Officer review SOC 2 + Schrems II + LGPD.
7. **Chaos test (D+4)**: SRE Lead + on-call review chaos scenarios + alerts.
8. **PRR mini (D+5)**: Architect + Security Lead + SRE Lead + Compliance Officer sign-off.

## 30. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Architect | _TBD; emphatic — multi-region architecture review + Terraform module + state management + per-region KV namespace_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD; emphatic — IAM scoping + state file integrity + subdomain spoofing defense_ | _pending_ | _pending_ |
| 5 | SRE Lead | _TBD; emphatic — region outage chaos test + RB-region + provisioning + migration_ | _pending_ | _pending_ |
| 6 | Engineer (S-14 lead) | _TBD_ | _pending_ | _pending_ |
| 7 | QA Lead | _TBD_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance Officer | _TBD; emphatic — SOC 2 CC6.1 + Schrems II + LGPD Art. 33 + GDPR Art. 46 attestation_ | _pending_ | _pending_ |
| 10 | Privacy Officer | _TBD; LGPD Art. 33 §1º + GDPR Art. 46 review_ | _pending_ | _pending_ |
| 11 | AppSec advisor | _TBD; emphatic — Terraform IAM + state file tampering scenarios_ | _pending_ | _pending_ |

> Crypto SME folds into Architect role specialization (S-14 BYOK em WI-S14-004+; este WI é infra-foundation; Crypto SME especializado contribui em WI-004/005/006/007). Peer reviewers contribuem em PR review sem sign-off canonical separado (folded into Engineer + Architect).

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-28 | Gustavo (via Claude Opus 4.7) | Criação WI-S14-001 (cycle 12.S14.0). |

## 32. Anti-patterns evitados

- Skip Terraform module reusability.
- Skip migration dry-run.
- Skip rollback path.
- Skip chaos test region outage cada região.
- Skip per-region KV namespace prefix.
- Skip DO jurisdictional_restriction WEUR.
- Auto-apply Terraform sem PR review + Cosign.
- Direct R2/D1/DO modification bypass Terraform.
- Single chaos test representative.
- Smart routing only sem custom domain explicit.
- Migration sem transactional atomic.
- State file sem versioning.

---

**Fim WI-S14-001.** Próximo: WI-S14-002 (tenant region pinning enforcement + 30k property test).
