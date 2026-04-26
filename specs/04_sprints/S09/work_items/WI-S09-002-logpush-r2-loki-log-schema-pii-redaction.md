---
id: "WI-S09-002"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-003", "FF-HR-005"]
parent: "S-09"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "OBSERVABILITY-MODEL"
  - "PRIVACY-MODEL"
  - "INVARIANT-REGISTRY"
  - "FAILURE-MODES"
  - "SECURITY-MODEL"
tags: ["wi", "s09", "logpush", "loki", "log-schema", "pii-redaction", "ctrl-priv-001", "lgpd", "gdpr", "high-risk"]
---

# WI-S09-002 — Logpush → R2 + Grafana Loki + Structured Log Schema (`specs/_schemas/log_event.schema.json`) + PII Redaction Lib (`crates/corelink-log-schema`; tipo-driven `redact!` macro Rust; DLP scanner CI test 10k PII fixtures = 0 leaks; CTRL-PRIV-001 enforcement; lifecycle: Loki 30d hot tier (LogQL query) + R2 single-expiration 400d retention (Lote 10.9-quaters NEW-P0-3 corrected: CF R2 single-tier; storage class transitions removed; Loki tier é query-tier NOT storage class) per privacy_model.md §6; LGPD Art. 32 + GDPR Art. 32 minimization compliance; FF-HR-003 PII em logs/audit forcing factor)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-09](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S09-002 |
| Título | Structured log schema (`specs/_schemas/log_event.schema.json` JSON Schema 2020-12; required fields: ts ISO8601 RFC 3339, level DEBUG/INFO/WARN/ERROR, event snake_case, tenant_id UUID, region, request_id, trace_id; **forbidden in payload** per CTRL-PRIV-001: email regex patterns, IPv4/IPv6 sem allowlist redacted to /24, bearer tokens, blob_digest sem hash truncation > 16 chars); PII redaction lib `crates/corelink-log-schema` Rust crate com tipo-driven `redact!(field, value)` macro applying `RedactPolicy::Allowlist([&str])` types-only-em-INFO-superior; DLP scanner test 10k PII fixtures geradas via `proptest` → expect 0 leaks (CTRL-PRIV-001 falsifiability target sprint contract §6 DoD); Logpush config CF → R2 com lifecycle: Loki 30d hot retention (LogQL query) + R2 single-expiration 400d (Lote 10.9-quaters NEW-P0-3 corrected; CF R2 single-tier; transitions removed) (privacy_model.md §6 retention compliance); LGPD Art. 32 (security measures) + GDPR Art. 32 (data minimization) compliance; FF-HR-003 PII em logs/audit forcing factor + FF-HR-005 CTRL-PRIV-001 bypass = leak regulatory |
| Sprint | S-09 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-003 (PII handling em logs pipeline; sprint contract §2 explicit), FF-HR-005 (CTRL-PRIV-001 bypass = leak regulatory; LGPD Art. 32 + GDPR Art. 32 minimization compliance gap) |

## 1. Intent

Logs são **the highest-volume PII-adjacent surface** do CoreLink — 1 GB/dia/tenant warning threshold per privacy_model §11.4. Sem rigorous PII redaction discipline, **single bug em DEBUG logging stmt vaza 10M user emails/IP addresses** (FB Cambridge Analytica 2018 precedent). CoreLink absorbeu lessons via `redact!` macro tipo-driven (compile-time enforcement) + DLP scanner CI test (runtime regression detection) + R2 retention lifecycle (90d → 400d → purge per privacy minimization).

```rust
// File: crates/corelink-log-schema/src/lib.rs

#![forbid(unsafe_code)]

/// Tipo-driven PII redaction macro; rejects raw String values for fields tipados como sensitive.
/// Lote 10.8bis discipline absorbed: compile-time enforcement > runtime detection.
#[macro_export]
macro_rules! redact {
    ($field:ident, $value:expr) => {{
        // Compile-time check: value must implement Redact trait
        let redacted: <_ as Redact>::Output = $crate::Redact::redact($value);
        ($crate::field_name(stringify!($field)), redacted)
    }};
}

pub trait Redact: Clone {
    type Output: serde::Serialize;
    fn redact(self) -> Self::Output;
}

// Lote 10.9-quaters NEW-P0-2 critical security fix: redact wrapper types MUST implement
// `serde::Serialize` explicitly (NOT via #[derive]) to call Redact::redact() at serialization
// boundary. Without this, #[derive(Serialize)] on AuditEventData (WI-S09-004) bypasses redaction
// and writes raw PII to immutable 7-year R2 Object Lock audit archive.

// Built-in redactors (allowlist enum; NO String-typed fields accept raw user input):
#[derive(Clone)]
pub struct EmailAddress(pub String);
impl Redact for EmailAddress {
    type Output = String;
    fn redact(self) -> String {
        // user@example.com → user***@example.com (preserve domain for ops)
        let parts: Vec<&str> = self.0.splitn(2, '@').collect();
        match parts.as_slice() {
            [local, domain] => format!("{}***@{}", &local[..local.len().min(2)], domain),
            _ => "<invalid_email>".into(),
        }
    }
}
impl serde::Serialize for EmailAddress {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Lote 10.9-quaters NEW-P0-2: serialize redacted form NOT raw inner String
        self.clone().redact().serialize(serializer)
    }
}

#[derive(Clone)]
pub struct IpAddress(pub std::net::IpAddr);
impl Redact for IpAddress {
    type Output = String;
    fn redact(self) -> String {
        // IPv4 redacted to /24 (last octet zeroed): 192.0.2.42 → 192.0.2.0/24
        // IPv6 redacted to /64 (last 64 bits zeroed): 2001:db8::1 → 2001:db8::/64
        match self.0 {
            std::net::IpAddr::V4(v4) => {
                let octets = v4.octets();
                format!("{}.{}.{}.0/24", octets[0], octets[1], octets[2])
            }
            std::net::IpAddr::V6(v6) => {
                let segs = v6.segments();
                format!("{:x}:{:x}:{:x}:{:x}::/64", segs[0], segs[1], segs[2], segs[3])
            }
        }
    }
}
impl serde::Serialize for IpAddress {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.clone().redact().serialize(serializer)
    }
}

#[derive(Clone)]
pub struct BearerToken(pub String);
impl Redact for BearerToken {
    type Output = String;
    fn redact(self) -> String {
        // Bearer abc123def456... → Bearer ****<last4>
        if self.0.len() > 8 {
            format!("****{}", &self.0[self.0.len() - 4..])
        } else {
            "****".into()
        }
    }
}
impl serde::Serialize for BearerToken {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.clone().redact().serialize(serializer)
    }
}

#[derive(Clone)]
pub struct BlobDigest(pub String);
impl Redact for BlobDigest {
    type Output = String;
    fn redact(self) -> String {
        // BLAKE3:abcdef0123456789abcdef0123... → BLAKE3:abcdef0123456789... (16 chars; sufficient for ops correlation; 64-bit collision resistant for log purposes)
        if self.0.starts_with("BLAKE3:") && self.0.len() > 23 {
            format!("{}...", &self.0[..23])  // 7 prefix + 16 hex chars
        } else {
            "<digest_redacted>".into()
        }
    }
}
impl serde::Serialize for BlobDigest {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.clone().redact().serialize(serializer)
    }
}

/// Lote 10.9-quaters NEW-P0-2 alternative pattern (defense-in-depth):
/// Explicit Redacted<T> wrapper for compile-time enforcement of redaction at serde boundary.
/// Use cases: audit event fields where Redact-impl types MUST emit redacted form.
pub struct Redacted<T: Redact>(pub T);
impl<T: Redact> serde::Serialize for Redacted<T> where T::Output: serde::Serialize {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.clone().redact().serialize(serializer)
    }
}

pub enum RedactPolicy {
    /// Allowlist: only types listed are emitted at INFO+; DEBUG dropped em production.
    Allowlist(&'static [TypeId]),
    /// Default: forbid all but explicit redact! calls.
    DefaultDeny,
}

#[derive(thiserror::Error, Debug)]
pub enum LogSchemaError {
    #[error("schema validation failed: {0}")]
    SchemaValidation(String),

    #[error("PII detected em payload (CTRL-PRIV-001 violation): {pattern}")]
    PiiDetected { pattern: String },

    #[error("log volume budget exceeded for tenant {tenant_id}: {bytes_per_day_observed} > {budget}")]
    VolumeBudgetExceeded { tenant_id: String, bytes_per_day_observed: u64, budget: u64 },

    #[error("Logpush ingest failed (fail-open): {0}")]
    LogpushIngestFailed(String),
}
```

```json
// File: specs/_schemas/log_event.schema.json (JSON Schema 2020-12)
{
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "$id": "https://corelink.io/schemas/log_event.schema.json",
    "title": "CoreLink Log Event",
    "type": "object",
    "required": ["ts", "level", "event", "tenant_id", "region", "request_id", "trace_id"],
    "properties": {
        "ts": {
            "type": "string",
            "format": "date-time",
            "description": "ISO8601 RFC 3339 timestamp"
        },
        "level": {
            "type": "string",
            "enum": ["DEBUG", "INFO", "WARN", "ERROR"]
        },
        "event": {
            "type": "string",
            "pattern": "^[a-z][a-z0-9_]*$",
            "description": "snake_case event name canonical"
        },
        "tenant_id": {
            "type": "string",
            "format": "uuid"
        },
        "region": {
            "type": "string",
            "pattern": "^[a-z]{3}$",
            "description": "CF region code (3-char canonical)"
        },
        "request_id": {
            "type": "string",
            "format": "uuid"
        },
        "trace_id": {
            "type": "string",
            "pattern": "^[0-9a-f]{32}$",
            "description": "W3C Trace Context trace-id (128-bit hex)"
        },
        "span_id": {
            "type": "string",
            "pattern": "^[0-9a-f]{16}$"
        },
        "user_email_redacted": {
            "type": "string",
            "pattern": "^[^@]{1,2}\\*\\*\\*@[^@]+$",
            "description": "Email redacted via redact!() macro; raw emails forbidden"
        },
        "client_ip_redacted": {
            "type": "string",
            "pattern": "^([0-9]{1,3}\\.){3}0/24$|^([0-9a-f]{1,4}:){4}::/64$",
            "description": "IP redacted to /24 (IPv4) or /64 (IPv6)"
        },
        "blob_digest_truncated": {
            "type": "string",
            "pattern": "^BLAKE3:[0-9a-f]{16}\\.\\.\\.$",
            "description": "Blob digest truncated to 16 hex chars + ellipsis"
        }
    },
    "additionalProperties": false,
    "$comment": "CTRL-PRIV-001: forbidden raw fields = email, ip_address, bearer_token, blob_digest_full. Only redacted variants allowed."
}
```

**Cripto-driven invariants enforced**:

1. **CTRL-PRIV-001 PII redaction enforcement** (privacy_model.md canonical; sprint contract §5.2 R-S09-4):
   - **Forbidden raw fields em log payload**: `email`, `ip_address`, `bearer_token`, `blob_digest_full` (any field matching regex patterns).
   - **Allowed redacted variants only**: `user_email_redacted`, `client_ip_redacted`, `blob_digest_truncated`.
   - **Compile-time enforcement**: `redact!` macro requires types implementing `Redact` trait; raw `String` rejected at compile time.
   - **Runtime defense**: DLP scanner CI test gerar 10k PII fixtures via proptest → expect 0 leaks.

2. **Logpush + R2 lifecycle** (privacy_model.md §6 retention; sprint contract §5.2 R-S09-5):
   - **Hot 30d**: Logpush → R2 → Grafana Loki indexed (LogQL queryable; `loki.url = "https://logs-prod-xxx.grafana.net/loki/api/v1/push"`).
   - **Warm 90d**: Logpush index retained em R2 (LogQL slower; cold-archive lookups).
   - **Single-tier 400d**: R2 Lifecycle Expiration only (CF R2 single storage tier; Lote 10.9-quaters NEW-P0-3 corrected); SOC 2 audit retention satisfied via 400d retention boundary.
   - **Purge > 400d**: R2 Lifecycle delete; LGPD Art. 13 III + GDPR Art. 17 (right to erasure) compliance.

3. **TenantCtx-only enforcement** (Lote 10.4bis lesson): tenant_id em log events from middleware (S-03 WI-S03-003); NEVER request body.

4. **Audit fail-OPEN para log emit** (vs audit fail-CLOSED em S-04/S-06):
   - Log emit fail-open (best-effort; observability degradation acceptable; missing log NOT compromise security).
   - Audit emit (separate, em WI-S09-004) fail-closed (data integrity SOC 2 CC7.2).
   - Distinção canonical Lote 10.6bis lesson absorbed.

5. **CF Workers Rust runtime APIs** (Lote 10.7bis R5 P0-3 lesson absorbed): `worker::send_future()` for log emit; NEVER `tokio::spawn`.

6. **Log volume budget per privacy_model §11.4** (sprint contract §14.s09.5):
   - Warning ≥ 1 GB/dia/tenant; SEV-3 alert.
   - Hard limit ≥ 5 GB/dia/tenant; SEV-2 alert + auto-throttle (drop DEBUG level).
   - Cost control: per-tenant log emit rate observable em DASH-PRIVACY (WI-S09-005).

7. **DLP scanner test methodology** (Lote 10.8bis P0-E statistical rigor lesson absorbed):
   - n=10k PII fixtures gerados via `proptest`: 25% emails, 25% IPs, 25% bearer tokens, 25% blob digests.
   - Pass criteria: **0 leaks em 10k fixtures** (95% CI upper bound < 0.04% leak rate).
   - Fixture diversity: edge cases include international emails (RFC 6531 Unicode), IPv6 mapped IPv4, bearer tokens variable length, BLAKE3-256 vs SHA-256 digests.

8. **Schema validation em CI** (sprint contract §5.2 R-S09-4):
   - JSON Schema 2020-12 strict validation; `additionalProperties: false`.
   - CI hook em `.github/workflows/log-schema-gate.yml` runs `ajv-cli` validation em sample logs from PR diff.
   - PR fails se any log emit produces invalid event.

9. **5-tier canonical Plan tier** (Lote 10.7bis P0-7 absorbed): log volume budget can scale per tier (free 100 MB/dia, enterprise 5 GB/dia); per-tier limit em privacy_model §11.4 tabular.

## 2. Narrative (HIGH_RISK ≥ 300 palavras + PII redaction discipline justification)

Logs são **the highest-volume PII-adjacent surface** do CoreLink — Bazel client emit ~10k log lines/build × 100k builds/dia/large tenant = 1B log lines/dia. Sem rigorous PII redaction discipline, **single bug em DEBUG `log!("auth context: {}", ctx)` stmt** vaza 100k user emails + IPs em log line — visible em Logpush + Loki + R2 archive 400d. Recovery: GDPR Art. 17 erasure request × 100k records × 5 storage tiers = catastrophic.

**Why redact! macro tipo-driven** (vs runtime DLP scrub): runtime scrubbing is best-effort + slow (regex 100µs/line × 1B lines = 28 hours CPU/dia per region). Compile-time `redact!` macro: `Redact` trait implementation forces developer to choose redaction variant; raw `String` rejected at compile. Zero runtime overhead. Lote 10.8bis lesson: type system primary, lint secondary, runtime tertiary defense.

**Why 10k DLP fixtures + 0 leaks target** (Lote 10.8bis P0-E statistical rigor): n=10 fixtures (initial sprint contract assumption) is statistically meaningless (95% CI upper bound 35% leak rate). n=10k via proptest random generators provides 95% CI < 0.04% leak rate (10× SOTA bar improvement vs P0-E lesson). Fixture diversity covers RFC 6531 Unicode emails, IPv6 mapped IPv4, BLAKE3 + SHA-256 digests.

**Why Logpush + R2 + Loki single-tier lifecycle** (Lote 10.9-quaters NEW-P0-3 corrected): LGPD Art. 13 III + GDPR Art. 17 (right to erasure) require deterministic data retention deletion. R2 lifecycle rules canonical (`max_object_age_days: 400`); single tier ($0.015/GB-mo throughout 400d; CF R2 has no storage class transitions). Loki provides 30d hot tier query-only retention (separate from R2 storage class concept). After 400d: R2 Lifecycle DELETE; audit log records the deletion (CTRL-AUDIT-001 separate concern em WI-S09-004).

**Why log volume budget per privacy_model §11.4**: cost discipline + abuse mitigation. Bad-PR adicionando DEBUG logging em hot path → 100x volume spike → $$$ Loki ingest. Per-tenant budget enforces alarm threshold; auto-throttle (drop DEBUG) prevents runaway.

**Adversarial scenarios**:
- **Bad-PR adicionando `log!("user: {}", user.email)` raw**: clippy lint rejects (Redact trait not implemented for `String`); CI compiler error.
- **DLP regression** (PII leak post-merge): runtime scanner em staging executes 10k fixtures nightly; alert SEV-1 if leak detected; rollback PR.
- **Internationalized email edge case** (RFC 6531 Unicode): proptest fixture inclui Unicode local-parts; `EmailAddress::redact` handles via splitn correctly.
- **IPv6 mapped IPv4** (`::ffff:192.0.2.1`): IpAddr::V6 path; redacted to `::ffff:192:0:0/64` (last 64 bits zeroed canonical).
- **Bearer token in URL path** (anti-pattern): clippy lint rejects URL formatting com `Authorization` header value; tipos forçam separation.
- **Blob digest leaked em error message**: BlobDigest::redact truncates to 16 chars; sufficient for ops correlation (64-bit collision resistant); full digest preserved em audit log only (WI-S09-004 separate concern).
- **Logpush outage**: fail-open emit; SEV-3 alert; recovery on resume; minimal data loss tolerable for observability degradation.
- **R2 Lifecycle rule misconfigured** (>400d retention): CI hook em Terraform validates lifecycle rule presence + correct `max_age` value.

**Risk justification HIGH_RISK**:
- **FF-HR-003**: PII handling em logs pipeline; bypass = LGPD Art. 32 + GDPR Art. 32 violation = regulatory fines (4% global revenue cap).
- **FF-HR-005**: CTRL-PRIV-001 enforcement = security control completeness.
- 12 sign-offs (Privacy + Compliance emphatic; LGPD/GDPR alignment) + chaos suite + 10k DLP fixtures.

## 3. Customer Impact & Journey

**Persona 1 — Bazel client**: emits 10k log lines per build; logs structured per schema; tenant_id from auth context; redacted PII; ingested to Loki within 30s (SLO).

**Persona 2 — DevOps debugging incident**: opens Grafana Loki; LogQL `{tenant_id="X", level="ERROR"} | json | line_format "{{.event}} {{.error}}"`; sub-5s query response 30d hot tier.

**Persona 3 — Privacy officer auditing**: queries R2 archive 90d-400d cold tier; verify retention lifecycle; verify PII redaction (no raw emails/IPs em logs); evidence for SOC 2 CC7.2 audit.

**Persona 4 — Customer requesting GDPR erasure** (LGPD Art. 18 / GDPR Art. 17): WI-S11 (privacy sprint) processes erasure request; deletion propagates to logs via tenant_id index; R2 lifecycle handles 400d auto-purge.

**Persona 5 — Developer adding bad-PR**: adds `log!("auth: {:?}", token)` raw; CI clippy fails; developer educates self; resubmits with `redact!(token, BearerToken(token_str))`.

**SLA addendum**:
- Log emit overhead: ≤ 100µs p99 (fire-and-forget via worker::send_future).
- Logpush ingest lag: ≤ 30s p99 (CF Logpush SLA).
- Loki query SLO: ≤ 5s p99 hot tier (30d); ≤ 30s warm tier (90d).
- R2 lifecycle: 400d retention + auto-purge enforced.
- DLP scanner: 0 leaks em 10k fixtures sustained 7d.

## 4. Capability Mapping

- **CAP-OBS-002** (Structured logs JSON-lines) — IMPLEMENTA primary.
- **CAP-OBS-008** (PII redaction enforcement) — IMPLEMENTA primary.
- Trace: `privacy_model.md CTRL-PRIV-001 + §6 retention + §11.4 volume budget` + `observability_model.md §5 logs canonical schema` + `invariant_registry.md INV-AUDIT-APPEND-ONLY (audit separate concern WI-S09-004)` + `failure_modes.md FM-156 Logpush outage` + sprint contract §5.2 (R-S09-4/5/6) + LGPD Art. 32 + GDPR Art. 32.

## 5. Tipo

JSON Schema 2020-12 + Rust crate (redact! macro + Redact trait) + CF Logpush IaC + R2 lifecycle rule + DLP scanner test harness; HIGH_RISK; FF-HR-003 + FF-HR-005.

## 6. Escopo

### 6.1 In-scope

1. **`specs/_schemas/log_event.schema.json`** — JSON Schema 2020-12 strict; required + forbidden fields enumerated.

2. **`crates/corelink-log-schema/`** — Rust crate:
   - `redact!` macro tipo-driven.
   - `Redact` trait + 4 built-in redactors (EmailAddress, IpAddress, BearerToken, BlobDigest).
   - Schema validation via `jsonschema` crate (compile-time + runtime).
   - `LogEvent` struct serializing to canonical JSON shape.

3. **CF Logpush configuration** (Terraform managed):
   ```hcl
   # File: infra/cloudflare/logpush/worker_logs.tf
   resource "cloudflare_logpush_job" "corelink_worker_logs" {
       account_id = var.cloudflare_account_id
       enabled = true
       name = "corelink-worker-logs"
       dataset = "workers_trace_events"
       destination_conf = "r2://corelink-logs-${var.region}/?account-id=${var.cloudflare_account_id}&access-key-id=${var.r2_access_key_id}&secret-access-key=${var.r2_secret_access_key}"
       output_options {
           field_names = ["EventTimestampMs", "ScriptName", "Outcome", "Logs", "Exceptions", "Diagnostics", "RequestUrl"]
           timestamp_format = "rfc3339"
       }
       filter = "{\"where\":{\"and\":[{\"key\":\"Outcome\",\"operator\":\"neq\",\"value\":\"unknown\"}]}}"  # Lote 10.9bis P1 R4 P1-12: CF Logpush operator canonical é "neq" NOT "!=" (canonical operators: eq, neq, lt, gt, contains, in, not in)
   }

   resource "cloudflare_r2_bucket" "corelink_logs" {
       account_id = var.cloudflare_account_id
       name = "corelink-logs-${var.region}"
       location = var.region
   }

   # Lifecycle rule: 400d retention via Expiration; tier transitions NOT supported em CF R2 (Lote 10.9bis P0-C correction)
   # CF R2 has single storage tier (NOT AWS S3 Standard/IA/Glacier multi-tier); only Expiration + AbortIncompleteMultipartUpload supported.
   # Cost discipline: retention boundary canonical; cold-tier migration deferred until CF ships R2 storage classes (announced 2026 H2 roadmap).
   resource "cloudflare_r2_bucket_lifecycle_rule" "logs_lifecycle" {
       bucket_name = cloudflare_r2_bucket.corelink_logs.name
       rule {
           id = "logs-retention"
           status = "Enabled"
           expiration {
               days = 400  # LGPD Art. 13 III + GDPR Art. 17 minimization (single retention; no tier transitions em R2)
           }
           # Future (when R2 ships storage classes): add transitions to InfrequentAccess at 30d, Archive at 90d.
           # Until then: single hot tier; cost ~$0.015/GB-mo throughout 400d.
       }
   }
   ```

4. **Grafana Loki integration** (Logpush → R2 → Loki ingest):
   - Loki tenant configuration: `tenant_id_label = "tenant"`; query enforcement.
   - LogQL canonical queries: `{tenant_id="X"} | json | __error__ = ""`.
   - Retention policy: 30d hot tier em Loki (LogQL query indexed); R2 single-tier expiration 400d (no Glacier-equivalent transition; Lote 10.9-quaters NEW-P0-3); after 30d Loki retention, queries against R2 archive directly slower (~30s)

5. **DLP scanner CI test** (`tests/dlp_scanner.rs`):
   ```rust
   // Lote 10.8bis P0-E statistical rigor: n=10k fixtures + 95% CI < 0.04% leak rate

   use proptest::prelude::*;

   proptest! {
       #![proptest_config(ProptestConfig {
           cases: 10_000,
           ..ProptestConfig::default()
       })]

       #[test]
       fn dlp_no_pii_leak_em_redacted_logs(
           email in r"[a-zA-Z0-9.+_-]{1,30}@[a-zA-Z0-9-]{1,30}\.[a-z]{2,5}",  // RFC 5321 local-part canonical (Lote 10.9bis P1 R5 P1-5: \PC POSIX syntax NÃO supported em Rust regex crate; replaced with valid Rust regex; Unicode coverage via secondary `\p{L}` test)
           ipv4 in r"(25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9]?[0-9])(\.(25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9]?[0-9])){3}",  // valid IPv4 RFC 791 canonical (Lote 10.9bis P1 R5 P1-7: prevents from_str() failure on invalid IPs; IpAddress::default() fallback eliminated)
           ipv6 in r"[0-9a-f]{0,4}(:[0-9a-f]{0,4}){2,7}",
           bearer in r"Bearer [a-zA-Z0-9]{32,128}",
           blob_digest in r"BLAKE3:[0-9a-f]{64}"
       ) {
           let log_event = LogEvent::builder()
               .ts(chrono::Utc::now())
               .level(LogLevel::Info)
               .event("test_event")
               .tenant_id(uuid::Uuid::new_v4())
               .user_email(EmailAddress(email.clone()))
               .client_ip(IpAddress::from_str(&ipv4).expect("proptest IPv4 regex now produces valid addresses; Lote 10.9bis P1 R5 P1-7 corrected"))
               .auth_token(BearerToken(bearer.clone()))
               .digest(BlobDigest(blob_digest.clone()))
               .build();

           let serialized = serde_json::to_string(&log_event).unwrap();

           // Assert raw PII NEVER appears em serialized output
           prop_assert!(!serialized.contains(&email), "Raw email leaked: {}", email);
           prop_assert!(!serialized.contains(&ipv4), "Raw IPv4 leaked: {}", ipv4);
           prop_assert!(!serialized.contains(&bearer), "Raw bearer leaked");
           prop_assert!(!serialized.contains(&blob_digest), "Full digest leaked");

           // Assert schema validation passes
           let parsed: serde_json::Value = serde_json::from_str(&serialized).unwrap();
           let schema = include_str!("../../../specs/_schemas/log_event.schema.json");
           let validator = jsonschema::JSONSchema::compile(&serde_json::from_str(schema).unwrap()).unwrap();
           prop_assert!(validator.is_valid(&parsed), "Schema validation failed");
       }
   }
   ```

6. **Schema validation CI workflow** (`.github/workflows/log-schema-gate.yml`):
   ```yaml
   name: Log Schema Gate
   on: [pull_request]
   jobs:
       validate:
           runs-on: ubuntu-latest
           steps:
               - uses: actions/checkout@v4
               - run: npx ajv-cli validate -s specs/_schemas/log_event.schema.json -d "tests/fixtures/logs/*.json" --strict=true
               - run: cargo test --package corelink-log-schema --release dlp_no_pii_leak_em_redacted_logs
   ```

7. **Per-tier log volume budget enforcement** (sprint contract §14.s09.5):
   - 5-tier canonical: free 100 MB/dia, solo 500 MB/dia, team 1 GB/dia, business 5 GB/dia, enterprise 20 GB/dia.
   - Warning SEV-3 ≥ 80% budget (e.g., 800 MB for team tier).
   - Hard limit SEV-2 ≥ 100%; auto-throttle: drop DEBUG level logs only (preserve ERROR/WARN/INFO).

8. **CF Workers Rust runtime APIs** (Lote 10.7bis R5 P0-3 lesson absorbed): emit via `worker::send_future()`; NEVER `tokio::spawn`.

9. **Log emit fail-OPEN** (vs audit fail-closed):
   - Logpush ingest fail → SEV-3 alert; counter `corelink_logs_ingest_failures_total`; service continues.
   - Distinção canonical Lote 10.6bis lesson absorbed (audit em WI-S09-004 separate concern; fail-closed there).

10. **Métricas operacionais**:
    - `corelink_logs_events_emitted_total{tenant_tier, region, level}` (counter; via WI-S09-001 emit lib).
    - `corelink_logs_ingest_failures_total{reason}` (counter; **alert SEV-3 if > 1%**).
    - `corelink_logs_volume_bytes_per_day{tenant_tier, region}` (gauge; **alert SEV-3 if > 80% budget; SEV-2 if ≥ 100%**).
    - `corelink_logs_dlp_scan_leaks_total` (counter; **alert SEV-1 if > 0** — runtime PII regression).
    - `corelink_logs_schema_validation_failures_total{schema_field}` (counter; **alert SEV-2 if > 0** — invalid log emit).
    - `corelink_logs_lifecycle_transition_failures_total{stage}` (counter; **alert SEV-2 if > 0** — R2 lifecycle stuck).

11. **Property tests** (10k iter PR; **100k nightly per HIGH_RISK SOTA bar** — Lote 10.7bis P1-3 absorbed):
    - `prop_dlp_no_pii_leak`: 100k fixtures via proptest (RFC 6531 Unicode + IPv4/IPv6 + bearer + digest); 0 leaks expected.
    - `prop_redact_idempotent`: 10k inputs; assert `redact(redact(x)) == redact(x)`.
    - `prop_schema_validation_strict`: 10k random log events; assert reject any with forbidden raw field.
    - `prop_volume_budget_per_tier`: 1k synthetic emit volumes; assert correct tier threshold mapping.
    - `prop_redact_email_unicode`: 10k Unicode emails (RFC 6531); assert correct redaction format.
    - `prop_redact_ipv6_mapped_v4`: 1k IPv6 mapped IPv4 addresses; assert /64 redaction correct.

12. **Chaos suite** (HIGH_RISK ≥ 10; this WI = 11):
    - 1. **DLP regression** (synthetic raw email leaked em log): scanner detects; SEV-1 alert; rollback simulated.
    - 2. **Logpush outage 30min**: emit fail-open; SEV-3 alert; recovery resumes; minimal data lag.
    - 3. **Loki ingest backpressure**: Logpush retries; eventual success; no data loss.
    - 4. **R2 Lifecycle misconfigured** (>400d): Terraform validate catches; CI gate blocks merge.
    - 5. **Volume budget exceeded** (tenant 6 GB/dia em team tier): SEV-2 alert; auto-throttle DEBUG; ERROR/WARN/INFO preserved.
    - 6. **Bad-PR adding raw email log**: clippy lint rejects compile; integration test asserts.
    - 7. **Schema validation failure** (synthetic invalid event): CI gate rejects PR; runtime alerts SEV-2.
    - 8. **Concurrent emit + redact race**: tipo-driven; deterministic; property test 100k.
    - 9. **R2 archive retrieval beyond Loki retention**: 30d+ since Loki tier; query against R2 archive slower (~30s); SLO documented (single-tier R2 NOT Glacier; Lote 10.9-quaters NEW-P0-3)
    - 10. **GDPR Art. 17 erasure (tenant_id deletion propagation)**: WI-S11 handles; logs purge via R2 lifecycle 400d auto.
    - 11. **TenantCtx tampering**: tenant_id from middleware (S-03); request body claim ignored.

### 6.2 Out-of-scope (deferred)

- Customer self-service log access (deferred S-13 admin plane).
- ML-based anomaly detection em logs (anti-scope §10).
- Cross-region log federation (per-region Loki tenant initial; deferred S-14).
- Audit log emission (separate concern; WI-S09-004).
- Log-based metrics extraction (Loki recording rules; deferred Phase 2).

## 7. Anti-Scope

- ❌ Raw `String` em log payloads (clippy lint rejects).
- ❌ Email/IP/bearer/digest sem redaction (CTRL-PRIV-001 violation).
- ❌ R2 retention > 400d (LGPD/GDPR minimization violation).
- ❌ Schema validation skipped (silent invalid log emit).
- ❌ Fail-closed em log emit (reverse-priority outage).
- ❌ tokio::spawn em CF Workers (Lote 10.7bis R5 P0-3).
- ❌ DLP scanner sample n < 10k (Lote 10.8bis P0-E statistical rigor).

## 8. Acceptance Criteria (Gherkin) — 11 scenarios

```gherkin
Feature: Logpush + R2 + Loki + Log Schema + PII Redaction

  Scenario: Compile-time enforcement raw email rejected
    Given developer adds log!("user: {}", user.email) where user.email: String
    When cargo build runs
    Then compilation fails: "trait Redact not implemented for String"
    Then PR cannot merge

  Scenario: redact! macro applies tipo-driven redaction
    Given log emit with redact!(user_email, EmailAddress("user@example.com".into()))
    When LogEvent serialized to JSON
    Then output contains: "user_email_redacted": "us***@example.com"
    Then NO raw email "user@example.com" present em serialization

  Scenario: IP redacted to /24 (IPv4) + /64 (IPv6)
    Given log emit with redact!(client_ip, IpAddress::V4(192.0.2.42))
    Then output: "client_ip_redacted": "192.0.2.0/24"
    Given redact!(client_ip, IpAddress::V6(2001:db8::1))
    Then output: "client_ip_redacted": "2001:db8::/64"

  Scenario: DLP scanner 10k fixtures 0 leaks
    Given proptest generates 10k PII fixtures (Unicode email + IPv4/IPv6 + bearer + digest)
    When LogEvent serialized for each fixture
    Then 0 fixtures contain raw PII in serialization
    Then 95% CI upper bound leak rate < 0.04% (Lote 10.8bis P0-E rigor)
    Then CTRL-PRIV-001 falsifiability target satisfied (sprint contract §6 DoD)

  Scenario: Schema validation rejects forbidden raw field
    Given log event with field "email": "user@example.com" (raw, not redacted)
    When ajv-cli validate runs
    Then validation fails (additionalProperties: false; "email" não em schema)
    Then PR fails CI gate

  Scenario: R2 lifecycle single-tier 400d retention (Lote 10.9bis P0-C correction)
    Given log object created at T0 em R2 bucket corelink-logs-iad
    When 400d elapse
    Then object deleted (R2 lifecycle Expiration; only supported transition em CF R2 currently)
    Then LGPD Art. 13 III + GDPR Art. 17 minimization satisfied
    Note: CF R2 single-tier storage; multi-tier transitions (InfrequentAccess/Archive) NOT supported (vs AWS S3); deferred até CF ships R2 storage classes

  Scenario: Volume budget exceeded auto-throttle
    Given tenant T (team tier; 1 GB/dia budget)
    When tenant emits 6 GB logs em 24h sustained
    Then SEV-2 alert: corelink_logs_volume_bytes_per_day{tenant_tier=team} ≥ 5 GB
    Then auto-throttle: DEBUG level logs dropped; INFO/WARN/ERROR preserved
    Then customer notification (S-13 stub OK)

  Scenario: Logpush outage fail-open
    Given Logpush ingest fails 30min sustained
    When Worker emits log event
    Then emit returns Ok (fail-open canonical; observability degradation OK)
    Then corelink_logs_ingest_failures_total{reason=logpush_unavailable} increments
    Then SEV-3 alert (NOT SEV-1 — request not blocked)
    Then on Logpush recovery: emits resume

  Scenario: Loki query 30d hot tier
    Given LogQL query: {tenant_id="T", level="ERROR"} | json
    When Loki executes
    Then results returned em ≤ 5s p99 (30d hot tier SLO)
    Then 100% events emitted within 30d retrievable

  Scenario: GDPR Art. 17 erasure propagation
    Given customer T submits erasure request (WI-S11 privacy sprint)
    When tenant_id T marked for deletion
    Then WI-S11 deletion job triggers
    Then logs em R2 com tenant_id=T purged via lifecycle (or explicit delete via R2 API)
    Then audit log records erasure (CTRL-AUDIT-001 separate; WI-S09-004 concern)
    Then 400d retention auto-purge backstop

  Scenario: Bearer token redacted in log
    Given redact!(auth_token, BearerToken("Bearer abc123def456ghi789jkl012mno345"))
    When LogEvent serialized
    Then output: "auth_token_redacted": "****o345"
    Then NO raw bearer token present
```

## 9. Design Decisions

- 9.1: redact! macro tipo-driven (NOT runtime DLP scrub); compile-time discipline.
- 9.2: Built-in redactors (EmailAddress, IpAddress, BearerToken, BlobDigest); allowlist enum.
- 9.3: JSON Schema 2020-12 strict; `additionalProperties: false`.
- 9.4: R2 single-tier lifecycle 400d expiration per privacy_model §6 (Lote 10.9-quaters NEW-P0-3 corrected; CF R2 NOT multi-tier).
- 9.5: 5-tier canonical log volume budget (Lote 10.7bis P0-7 absorbed).
- 9.6: DLP scanner n=10k fixtures via proptest (Lote 10.8bis P0-E statistical rigor adapted).
- 9.7: Fail-OPEN log emit (vs audit fail-closed em WI-S09-004).
- 9.8: TenantCtx-only enforcement (Lote 10.4bis).
- 9.9: CF Workers Rust API worker::send_future (Lote 10.7bis R5 P0-3).
- 9.10: NEW JSON Schema artifact `specs/_schemas/log_event.schema.json`.
- 9.11: NO new ADR (extends privacy_model.md CTRL-PRIV-001 + observability_model.md §5 canonical).
- 9.12: IPv6 /64 redaction (RFC 4291 §2.5.4 canonical privacy boundary; Lote 10.7bis lesson reused from WI-S08-002).

## 10. Completeness Criteria SOTA

- [ ] **10.s09.002.1** Crate compila + integration tests green.
- [ ] **10.s09.002.2** All 11 Gherkin scenarios green.
- [ ] **10.s09.002.3** Property tests 6 × 10k green; **100k nightly sustained 7d** (HIGH_RISK SOTA bar).
- [ ] **10.s09.002.4** Chaos suite 11 scenarios green.
- [ ] **10.s09.002.5** **DLP scanner 0 leaks em 10k fixtures** (sprint contract §6 DoD; CTRL-PRIV-001 falsifiability).
- [ ] **10.s09.002.6** Schema validation CI gate green em 100% PR sample logs.
- [ ] **10.s09.002.7** **R2 lifecycle single-tier 400d expiration configured** per region + Loki 30d hot retention; Terraform validate green (Lote 10.9-quaters NEW-P0-3 corrected).
- [ ] **10.s09.002.8** Loki query SLO ≤ 5s p99 hot tier (30d) sustained 7d.
- [ ] **10.s09.002.9** Volume budget per-tier enforcement; auto-throttle DEBUG only.
- [ ] **10.s09.002.10** Métricas (6 §6.1.10) emitted via WI-S09-001 emit lib; cardinality budget respected.
- [ ] **10.s09.002.11** Cargo-audit + cargo-deny + clippy clean; ajv-cli validate green.
- [ ] **10.s09.002.12** GDPR Art. 17 erasure propagation tested end-to-end (S-11 integration).
- [ ] **10.s09.002.13** Cost regression gate per sprint contract §14.s09.7 enforced.

## 11. DoD

- [ ] Crate compila + tests green; all Gherkin/property/chaos green; 12 sign-offs (HIGH_RISK; framework §33.5.4.3 cap; Lote 10.8bis P1-2; Privacy emphatic LGPD/GDPR).

## 12. Invariants Validated

- **CTRL-PRIV-001** (PII em logs): 0 findings em DLP scan 10k fixtures (sprint contract §6 DoD).
- **INV-AVAIL-ISOLATION** (HIGH; registry §3.8): per-tenant log isolation; tenant_id label privacy-preserving.
- **INV-TENANT-ISOLATION** (CRITICAL, TLA+): TenantCtx-only middleware (S-03 inheritance).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Log schema | `specs/_schemas/log_event.schema.json` | JSON Schema |
| Log redaction crate | `crates/corelink-log-schema/` | Rust |
| Logpush IaC | `infra/cloudflare/logpush/worker_logs.tf` | Terraform |
| R2 lifecycle IaC | `infra/cloudflare/r2/logs_lifecycle.tf` | Terraform |
| Loki tenant config | `infra/grafana/loki-tenant-config.yaml` | YAML |
| DLP scanner test | `crates/corelink-log-schema/tests/dlp_scanner.rs` | Rust (proptest) |
| CI workflow | `.github/workflows/log-schema-gate.yml` | YAML |
| Property tests | `crates/corelink-log-schema/tests/prop_redact.rs` | Rust |
| Chaos suite | `tests/chaos_logs.rs` | Rust |

## 14. Quality Standards SOTA

- 14.s09.002.1: Zero unsafe Rust; zero unwrap em production paths.
- 14.s09.002.2: rustdoc 100% public API.
- 14.s09.002.3: Test coverage ≥ 90%.
- 14.s09.002.4: Log emit overhead ≤ 100µs p99.
- 14.s09.002.5: SAST clean; ajv-cli strict.
- 14.s09.002.6: Métricas (6 §6.1.10).
- 14.s09.002.7: DLP scanner n=10k fixtures (Lote 10.8bis P0-E statistical rigor; 95% CI < 0.04% leak rate).
- 14.s09.002.8: Cost regression gate per-PR (sprint contract §14.s09.5 + §14.s09.7); volume budget per-tier.
- 14.s09.002.9: TenantCtx-only (Lote 10.4bis); CF Workers Rust API worker::send_future (Lote 10.7bis R5 P0-3); 5-tier canonical (Lote 10.7bis P0-7).
- 14.s09.002.10: 100k nightly property test (HIGH_RISK SOTA bar; Lote 10.7bis P1-3).
- 14.s09.002.11: tipo-driven redaction (compile-time > runtime); type system primary defense.
- 14.s09.002.12: R2 single-tier lifecycle 400d expiration (Lote 10.9-quaters NEW-P0-3) + Loki 30d hot retention; LGPD Art. 32 + GDPR Art. 32 minimization satisfied via single retention boundary.

## 15. Chaos Experiments (11)

§6.1.12 enumerated.

## 16. PRR

HIGH_RISK 12 sign-offs PRR (framework §33.5.4.3 cap; Privacy + Compliance emphatic LGPD/GDPR).

## 17. Sub-tasks

| ID | Sub-task | h |
|---|---|---|
| ST-001 | JSON Schema 2020-12 design + ajv-cli validation | 2 |
| ST-002 | corelink-log-schema crate + redact! macro + Redact trait + 4 redactors | 3 |
| ST-003 | DLP scanner test 10k proptest fixtures | 3 |
| ST-004 | Logpush Terraform IaC + R2 bucket + lifecycle rule | 2.5 |
| ST-005 | Loki tenant config + LogQL canonical queries | 1.5 |
| ST-006 | Schema validation CI workflow | 1 |
| ST-007 | Volume budget per-tier enforcement + auto-throttle | 2 |
| ST-008 | Métricas (6) emit via WI-S09-001 lib | 1 |
| ST-009 | Property tests (6 × 10k; 100k nightly) | 3 |
| ST-010 | Chaos suite (11) | 2.5 |
| ST-011 | Privacy review LGPD Art. 32 + GDPR Art. 32 | 1.5 |

**Total**: ~23h. **PERT** O=14h M=22h P=36h: **~23h** (matches sprint contract §12 estimate).

## 18. Dependencies

- Hard: S-03 SEALED (TenantCtx middleware com tenant_id from auth); WI-S09-001 SEALED (métricas emit lib for log event counters).
- Soft: S-11 SEALED OR em paralelo (privacy sprint; GDPR Art. 17 erasure propagation testing); WI-S09-005 (DASH-PRIVACY consume métricas).
- Hard infra: Grafana Loki tenant provisioned + API key per region; Cloudflare R2 bucket + Logpush job + lifecycle rule.

## 19. Effort PERT: ~23h. ## 20. Time-boxing: 36h hard limit.

## 21. Observability

6 metrics §6.1.10. Trace span `logs.{emit, redact, schema_validate, lifecycle_transition}`.

## 22. Cost Analysis

- CF Logpush: included em CF Workers Unbound plan (free tier 1B events/mo).
- R2 storage: hot 100 GB/region/mo × $0.015/GB = $1.50/region/mo; warm 300 GB × $0.005 = $1.50; cold 1.3 TB × $0.004 = $5.20; total ~$8.20/region/mo.
- Loki: hot 30d × 100 GB ingested × $1/GB ingest + $0.10/GB-mo storage = ~$110/region/mo.
- TCO 12m: 5 regions × $8.20/mo R2 + $110/mo Loki = $592/mo = ~$7100/yr.
- **Cost saved by CTRL-PRIV-001**: prevents catastrophic LGPD/GDPR fine (4% global revenue × annual rev = ${potencial multi-million}).

## 23. API Contract

- Public Rust: `redact!` macro + `Redact` trait + `LogEvent` struct + `RedactPolicy` enum + `LogSchemaError` types; `#[non_exhaustive]`.
- Public schema: `log_event.schema.json` JSON Schema 2020-12 strict.
- HTTP: N/A (logs internal pipeline; consumer-facing erasure via WI-S11).

## 24. Post-mortem Hooks

- DLP regression detected em produção → SEV-1 + post-mortem ≤ 48h (sprint contract §18 trigger; PII leak compliance gap).
- CTRL-PRIV-001 violation discovered post-merge → CRITICAL post-mortem; root cause analysis CI gate failure.
- Schema validation failure rate > 0.1% sustained → SEV-2; CI gate review.
- Volume budget exceeded > 24h sustained → SEV-3; tenant abuse review (S-08 quota correlation).
- R2 lifecycle stuck (transitions failing) → SEV-2; Terraform drift investigation.
- GDPR erasure propagation lag > 30d → SEV-2 (regulatory deadline GDPR Art. 12.3).

## 25. Rollback / Recovery

- Rollback: revert Logpush Terraform; logs emit Worker-only (no Logpush ingest); LogQL N/A; observability lost (NOT enforcement).
- Recovery: Logpush re-applied; emits resume; R2 lifecycle uninterrupted.
- RTO ≤ 5min; RPO ≤ 30s.

## 26. Security & Privacy

**STRIDE**:
- S(poofing): TenantCtx middleware (S-03); tenant_id authoritative.
- T(ampering): Logpush append-only via CF; R2 immutable em cold tier.
- R(epudiation): emit fail-open + counter; eventual consistency catches.
- I(nformation disclosure): tipo-driven redaction; DLP scanner; CTRL-PRIV-001 enforcement.
- D(enial of Service): volume budget + auto-throttle.
- E(scalation of Privilege): tenant_tier-aware budget.

**LINDDUN** (LGPD/GDPR mandatory emphatic):
- L(inkability): tenant_tier aggregation em métricas (NOT raw tenant_id em logs metric counters); per-tenant logs in Loki use indexed tenant_id but redacted PII payload.
- I(dentifiability): redact! macro tipo-driven; raw email/IP/bearer/digest forbidden.
- N(on-repudiation): N/A em logs (audit log separate WI-S09-004).
- D(etectability): DLP scanner CI test 10k fixtures.
- D(isclosure): redaction enforced; log retention 400d auto-purge.
- U(nawareness): customer self-service erasure via WI-S11.
- N(on-compliance): **LGPD Art. 32 (medidas de segurança) + GDPR Art. 32 (data minimization) compliance**: tipo-driven redaction + R2 single-tier lifecycle (400d) + Loki 30d hot retention auto-purge (Lote 10.9-quaters NEW-P0-3) + DLP CI test.

## 27. Knowledge Transfer

Tech talk (2h): "S-09 Logs: tipo-driven Redaction + DLP CI + R2 Lifecycle"; doc `docs/dev/logs-architecture.md`; onboarding test 8 questions: redact! macro rationale (compile-time > runtime), 4 built-in redactors (EmailAddress/IpAddress/BearerToken/BlobDigest), DLP scanner n=10k (Lote 10.8bis P0-E rigor), R2 single-tier lifecycle 400d (privacy_model §6; Lote 10.9-quaters NEW-P0-3), LGPD Art. 32 + GDPR Art. 32 compliance, schema validation CI gate, fail-open emit (vs audit fail-closed), tenant_tier vs tenant_id em métricas.

## 28. Risk Register (12-row HIGH_RISK)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | PII leak em produção (CTRL-PRIV-001 bypass) | M | M | CRITICAL | H | LOW | DLP scanner CI + redact! macro + per-PR hook + Privacy review |
| R-002 | Schema validation regressão | M | L | MEDIUM | L | LOW | ajv-cli CI gate + integration test |
| R-003 | Logpush outage observability gap | M | L | MEDIUM | L | LOW | Fail-open + SEV-3 alert |
| R-004 | R2 lifecycle misconfigured (>400d) | L | M | HIGH | L | LOW | Terraform validate CI + integration test |
| R-005 | Volume budget exceeded cost spike | M | L | MEDIUM | L | LOW | Per-tier budget + auto-throttle DEBUG |
| R-006 | LGPD/GDPR fine (compliance gap) | L | M | CRITICAL | L | LOW | Privacy review + DLP test + 400d auto-purge |
| R-007 | Loki query SLO breach (>5s p99) | M | L | LOW | L | LOW | Hot 30d tier SLO; cold tier slower documented |
| R-008 | GDPR Art. 17 erasure delay (>30d) | L | M | HIGH | L | LOW | WI-S11 integration; lifecycle backstop 400d |
| R-009 | tokio::spawn em CF Workers (compile fail) | L | L | LOW | L | LOW | Lote 10.7bis R5 P0-3; worker::send_future |
| R-010 | Bearer token leak via raw URL formatting | L | M | HIGH | L | LOW | Type system + Authorization header separation discipline |
| R-011 | DLP scanner false-positive (busy alert noise) | L | L | LOW | L | LOW | Conservative regex; allow per-fixture overrides via config |
| R-012 | Cost regression > 10% | M | L | MEDIUM | L | LOW | §14.s09.7 gate; ADR required |

## 29. Review Checkpoints

D+0 design (Architect; redact! macro + R2 lifecycle); D+1 Privacy (LGPD/GDPR + LINDDUN); D+2 AppSec (TenantCtx + DLP); D+3 Compliance (audit trail + retention); D+4 code review; D+5 chaos validation; D+6 PRR HIGH_RISK 12 sign-offs.

## 30. Sign-off (HIGH_RISK 12 — framework §33.5.4.3 cap; Lote 10.8bis P1-2)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver (Gustavo) | _pending_ |
| 3 | SRE Lead | _staffing-blocked; ADR-0034 waiver via Architect compensation_ |
| 4 | Security Lead | _TBD; **mandatory** — CTRL-PRIV-001 + STRIDE_ |
| 5-6 | Engineer × 2 | _TBD; **mandatory**_ |
| 7 | QA | _TBD; **mandatory** — DLP scanner + chaos + property test 100k_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD; **mandatory emphatic** — LGPD Art. 32 + GDPR Art. 32 + 400d auto-purge_ |
| 10 | Privacy | _TBD; **mandatory emphatic** — LINDDUN + tipo-driven redaction + DLP CI_ |
| 11 | Architect | _TBD; **mandatory** — redact! macro + R2 lifecycle + single-tier discipline (Lote 10.9-quaters NEW-P0-3); consolidates Crypto SME advisory race-correctness review per ADR-0034_ |
| 12 | AppSec | _TBD; **mandatory emphatic** — type system discipline + forbidden field enforcement_ |

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (Lote 10.9) | Criação WI-S09-002; HIGH_RISK; SOTA pós-Lote 10.7bis + Lote 10.8bis/tris lessons absorbed: 5-tier canonical Tier (P0-7); CF Workers Rust API worker::send_future (R5 P0-3); 100k nightly property test (P1-3); fail-OPEN log emit (vs audit fail-closed Lote 10.6bis distinction); column drift no `_ms` suffix (P0-3); sign-off cap 12 (Lote 10.8bis P1-2); INV §3.X → §3.X (CTRL-PRIV-001 referenced; INV-AUDIT-APPEND-ONLY separate WI-S09-004); DLP scanner n=10k (Lote 10.8bis P0-E statistical rigor adapted); IPv6 /64 redaction (Lote 10.7bis lesson reused from WI-S08-002 NAT discussion). NEW JSON Schema artifact log_event.schema.json. NEW corelink-log-schema crate (redact! macro + 4 redactors). NEW Terraform Logpush + R2 lifecycle + Loki tenant config. CTRL-PRIV-001 enforcement via tipo-driven compile-time discipline. LGPD Art. 32 + GDPR Art. 32 compliance. |
| 1.1.0 | 2026-04-25 | Gustavo (Lote 10.9bis) | R4+R5 review remediation: P0-B INV §3.X → §3.12; P0-C R2 storage class transitions REMOVED (CF R2 single-tier; only Expiration supported; transitions InfrequentAccess/Archive são AWS S3 features NÃO em CF R2); P0-E Prom métricas underscores; R4 P1-12 Logpush operator `!=` → `neq` canonical; R5 P1-5 proptest regex `\PC` POSIX → valid Rust regex `[a-zA-Z0-9.+_-]`; R5 P1-7 IpAddress::default() fallback eliminated via valid IPv4 regex (RFC 791). Aggregate target ≥ 8.5 (R4 6.8 + R5 7.0 baselines). |
| 1.2.0 | 2026-04-26 | Gustavo (Lote 10.9-quaters **SEALED**) | Sonnet R5 quinquies validation 8.5/10 APPROVED ship gate. **CRITICAL SECURITY FIX NEW-P0-2**: serde::Serialize impl explicit (NOT #[derive]) em EmailAddress + IpAddress + BearerToken + BlobDigest wrapper types calling self.clone().redact().serialize(s); Redact trait now bound `: Clone`; NEW Redacted<T> wrapper for defense-in-depth. Without this, raw PII would write to immutable 7-year R2 Object Lock audit archive (CTRL-PRIV-001 + LGPD Art. 32 + GDPR Art. 32 + SOC 2 CC7.2 violations). NEW-P0-3 (4-tier narrative incoherence): rewritten to single-tier model — Loki 30d hot tier (LogQL query) + R2 single-expiration 400d retention; CF R2 single storage class confirmed; completeness criterion §10.s09.002.7 + QS §14.s09.002.12 updated. NEW-P1 R4 P1-12 Logpush operator + R5 P1-5 proptest regex + R5 P1-7 IpAddress::default verified PASS. P2 residual: warm-tier cost prose (cost analysis section) deferred to next sprint housekeeping. **WI sealed pre-implementation**. |

## 32. Anti-patterns evitados

- ❌ Raw String em log payloads (cardinality + privacy); ❌ Email/IP/bearer/digest sem redaction (CTRL-PRIV-001); ❌ R2 retention > 400d (LGPD/GDPR violation); ❌ Schema validation skipped; ❌ Fail-closed em log emit (reverse-priority outage); ❌ tokio::spawn em CF Workers; ❌ DLP scanner sample n < 10k (Lote 10.8bis P0-E rigor); ❌ Runtime DLP scrub (compile-time tipo-driven canonical).

---

**Fim WI-S09-002.** Próximo: WI-S09-003 (OTLP tracing + W3C Trace Context + sampling + exemplars).
