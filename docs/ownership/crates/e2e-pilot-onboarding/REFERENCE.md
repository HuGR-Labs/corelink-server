---
schema: corelink-ownership/1.1
document: reference
package: e2e-pilot-onboarding
manifest: tests/e2e-pilot-onboarding/Cargo.toml
source_commit: cb94e251c0f17382565bf863f517945cbb2a84d6
profile: S
state: draft
evidence_set: w014-e2e-pilot-onboarding-source-static-20260921
---

# e2e-pilot-onboarding — ownership reference

Pinned SOURCE inventory of Rust contracts and checked-in assertions. The manifest description and comments express intent; neither they, test names, nor receipt strings establish provider use, execution, or runtime. Canonical signup/onboarding policy is routed to the [designated OKF concept](../../../../docs/knowledge/launch/signup-onboarding.md).

[Identity](#r01) · [Boundaries](#r02) · [Modules](#r03) · [Contracts](#r04) · [State](#r05) · [Targets](#r06) · [Failures](#r07) · [Evidence](#r08).

<a id="r01"></a>
## R01 — Identity and function

`e2e-pilot-onboarding` is a test harness package with a public library and five integration targets. Checked source composes deterministic fixtures and local in-memory state; no direct first-party Cargo package dependency is declared. The end-to-end narrative is not evidence of production composition or a run.

| Field | Verified declaration |
|---|---|
| Repository / manifest | `repo:1232040291`; `tests/e2e-pilot-onboarding/Cargo.toml` |
| Cargo package identity | `[package].name = e2e-pilot-onboarding` |
| Library | `src/lib.rs` (`[lib] path`) |
| Integration targets | `test_01_tenant_signup`, `test_02_first_cas_upload`, `test_03_audit_export_roundtrip`, `test_04_dsr_erasure`, `test_05_tenant_offboarding` |
| License / publish | workspace-inherited license; `publish = false` |
| Implementation / wiring / runtime | local implementation present; workspace member declared; target resolution and runtime UNKNOWN |

Evidence: package manifest lines 1–43 and root `Cargo.toml:357`. Baseline is `cb94e251c0f17382565bf863f517945cbb2a84d6`.

<a id="r02"></a>
## R02 — Boundaries and ownership

| Surface | Implementation owner | Contract owner | Operation / route |
|---|---|---|---|
| Harness API and local scenario source | `UNASSIGNED`; ask a repository administrator to assign a package implementation owner, then record the assignment before owner-authorized work | Package-local Rust contract | Static evidence in R03–R05; stop on work requiring owner authority until assigned |
| Production signup implementation | Not owned or proven used here | Production signup owner route | [own-corelink-signup](../../../../.claude/skills/own-corelink-signup/SKILL.md); implementation ownership only |
| Canonical signup/onboarding policy | Not owned here | OKF | Designated [OKF policy](../../../../docs/knowledge/launch/signup-onboarding.md); route only, do not duplicate |
| Production providers / operation | Not established | External service owners not verified | No deployment, provider, customer-data, or operational authority |

**Does not establish:** production signup, checkout, D1, R2, audit sink, DSR worker,
subscription cancellation, hard deletion, compliance, or a real user journey.
Cross-cutting [built-not-wired](../../../../.claude/skills/built-not-wired/SKILL.md)
and [OKF context](../../../../.claude/skills/okf-context/SKILL.md) skills exist;

they do not change this package's authority. No local independent-review skill
was verified. `.github/CODEOWNERS` has a catch-all review request to `@gmhelmold`;
CODEOWNERS requests review and grants no implementation, operations, or independent-
approval authority. Route owner assignment to a repository administrator; owner
remains `UNASSIGNED` until the assignment is recorded. Stop any action needing that
authority. This is distinct from the fresh reviewer route.

<a id="r03"></a>
## R03 — Implementation map

| Module / entry | Owned behavior and data | Nature | Contract/evidence |
|---|---|---|---|
| `src/lib.rs` | lints, `pub mod harness`, root re-exports | public façade over local module, not external adapter | lines 56–69; REL-002 |
| `src/harness.rs` | fixtures, public errors/receipts/enums, mutex/map state, signup/upload/read/export/verify/inspectors | package-owned in-memory implementation | lines 17–894; REL-004–REL-008 |
| `src/harness_lifecycle.rs` | DSR request/finalize, cancel, offboarding completion | path-included extension `impl PilotHarness` | lines 17–235; REL-003, REL-009–REL-012 |
| `tests/test_01_...rs`–`test_05_...rs` | scenario calls and local assertions | integration test sources; target activation only declared | manifest lines 21–37; REL-013–REL-017 |

**Inventory:** one library, two implementation modules (one path-included), five
integration targets with 14 test functions, and three source-local unit tests. No
binary, build script, example, or feature section; this is static, not resolution or execution.

<a id="r04"></a>
## R04 — Public Rust contracts

**Contract index:** [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004) · [API-005](#api-005) · [API-006](#api-006) · [API-007](#api-007) · [API-008](#api-008) · [API-009](#api-009) · [API-010](#api-010) · [API-011](#api-011) · [API-012](#api-012) · [API-013](#api-013) · [API-014](#api-014) · [API-015](#api-015) · [API-016](#api-016) · [API-017](#api-017) · [API-018](#api-018) · [API-019](#api-019) · [API-020](#api-020) · [API-021](#api-021) · [API-022](#api-022) · [API-023](#api-023) · [API-024](#api-024) · [API-025](#api-025).

<a id="api-001"></a>
### API-001 — `canonical_pilot_tenant`

**Symbol:** `canonical_pilot_tenant(&str) -> PilotTenant`. **Input/output:** slug deterministically derives local tenant, subscription, and admin fields. **Failure/effect:** no `Result`; no provider identity lookup. **Compatibility:** public re-export; changed derivation changes fixture IDs. **INV/REL/proof:** INV-001; REL-002; `harness.rs:45–76`; unit-test source checks deterministic output.
[Contract index](#r04)

<a id="api-002"></a>
[↩](#r01)
### API-002 — `canonical_blob_payload`

**Symbol:** `canonical_blob_payload(&str, usize) -> Vec<u8>`. **Input/output:** tenant slug and index deterministically produce 4 KiB bytes. **Failure/effect:** no `Result`; local fixture generation only. **Compatibility:** public re-export; byte change changes digest/test fixture. **INV/REL/proof:** INV-001; REL-002; `harness.rs:78–91`; unit-test source checks size/determinism.
[Contract index](#r04)

<a id="api-003"></a>
[↩](#r01)
### API-003 — `canonical_blob_payloads`

**Symbol:** `canonical_blob_payloads(&str) -> Vec<Vec<u8>>`. **Input/output:** slug yields the declared canonical-count sequence of locally generated payloads. **Failure/effect:** no `Result`; no storage write. **Compatibility:** public re-export; count/order/content feed scenario contracts. **INV/REL/proof:** INV-001; REL-002; `harness.rs:27,93–99`; target sources consume it.
[Contract index](#r04)

<a id="api-004"></a>
[↩](#r01)
### API-004 — `blob_digest_hex`

**Symbol:** `blob_digest_hex(&[u8]) -> String`. **Input/output:** returns BLAKE3 digest as hex for caller bytes. **Failure/effect:** no `Result`; pure local hash helper. **Compatibility:** public re-export; digest representation is consumed by receipts/tests. **INV/REL/proof:** INV-001/004; REL-002; `harness.rs:101`; test 02 source compares fixture digests.
[Contract index](#r04)

<a id="api-005"></a>
[↩](#r01)
### API-005 — Harness construction

**Symbol:** `PilotHarness::new() -> Self` (also `Default`). **Precondition:** none. **Postcondition:** empty mutex-protected map keyed by slug. **Failure/effect:** no returned error; later lock poison is surfaced by fallible operations as `PilotHarnessError::Invariant`, while count/snapshot inspectors use fallback values. **Compatibility:** local memory lifetime is the harness instance. **INV/REL/proof:** INV-002; REL-004–REL-012; `harness.rs:447–464`.
[Contract index](#r04)

<a id="api-006"></a>
[↩](#r01)
### API-006 — `complete_signup`

**Symbol:** `PilotHarness::complete_signup(&PilotTenant) -> Result<SignupReceipt, PilotHarnessError>`. **Guard/output:** rejects already Active, CancelledInGrace, or HardDeleted; otherwise returns tenant/subscription IDs and fixed activation ms. **Effects:** emits four local audit rows and updates local lifecycle/subscription/checkout flag. CheckoutPending/PendingActivation is assigned before `CheckoutSessionCreated`; Active is assigned after `TenantProvisioned`. Audit failure after the pending assignment can leave partial local state; no transaction is promised. **Errors:** `SignupError::AlreadyProvisioned`, wrapped lock/audit invariant. **INV/REL/proof:** INV-003; REL-004, REL-013–REL-017; `harness.rs:525–585`. Tests assert final state/event sequence, not operation order.
[Contract index](#r04)

<a id="api-007"></a>
[↩](#r01)
### API-007 — `batch_upload_blobs`

**Symbol:** `PilotHarness::batch_upload_blobs(&PilotTenant, &[Vec<u8>]) -> Result<BatchUploadReceipt, PilotHarnessError>`. **Precondition:** lifecycle exists and subscription is Active. **Output/effects:** BLAKE3 digests in input order, count, and formatted prefix; writes bytes into local slug state's `BTreeMap` keyed only by digest. Existing same digest with different bytes returns `CasError::DigestMismatch`. **Other errors:** not provisioned, inactive, poisoned/audit invariant.

CAS insert precedes each `BlobPutCommitted` emission; a later audit failure can leave an inserted unaudited blob. Receipt prefix is text, not storage key. Tests assert final sequence, not operation order. **INV/REL/proof:** INV-004; REL-005, REL-014–REL-017; `harness.rs:607–659`.
[Contract index](#r04)

<a id="api-008"></a>
[↩](#r01)
### API-008 — `read_blob`

**Symbol:** `PilotHarness::read_blob(&PilotTenant, &str) -> Result<Vec<u8>, PilotHarnessError>`. **Input:** digest hex; lookup first uses caller slug's local map. **Output/effect:** cloned bytes for own map; if absent locally but present under another slug, returns `CasError::TenantPrefixViolation`; if absent everywhere, returns `TenantNotProvisioned`. **Compatibility:** scan is across harness-local maps; receipt prefix does not participate in key lookup. **INV/REL/proof:** INV-004; REL-006, REL-014; `harness.rs:668–692`.
[Contract index](#r04)

<a id="api-009"></a>
[↩](#r01)
### API-009 — `export_audit_window`

**Symbol:** `PilotHarness::export_audit_window(&PilotTenant, u64, u64) -> Result<(Vec<u8>, AuditExportManifest), PilotHarnessError>`. **Window:** `[start,end)`; `start > end` returns `InvalidWindow`; missing local lifecycle returns `TenantNotProvisioned`. **Output/effects:** emits local start marker, snapshots earlier in-window rows, serializes NDJSON, hashes body, returns manifest, then emits seal marker. **Failure:** serialization/lock/audit failures can return invariant error; no rollback guarantee stated. **INV/REL/proof:** INV-005; REL-007, REL-015; `harness.rs:717–803`.
[Contract index](#r04)

<a id="api-010"></a>
[↩](#r01)
### API-010 — `verify_audit_export`

**Symbol:** `PilotHarness::verify_audit_export(&[u8], &AuditExportManifest) -> Result<(), PilotHarnessError>`. **Checks:** body digest, NDJSON parsing, row count, and adjacent `prev_hash_hex == prior.row_hash_hex`. **Errors:** manifest digest/count mismatch, chain break, or invariant on parse. **Limit:** does not recompute each row hash or compare manifest tenant/window/first/last fields. **State:** does not mutate harness state. **INV/REL/proof:** INV-006; REL-008, REL-015; `harness.rs:805–846`.
[Contract index](#r04)

<a id="api-011"></a>
[↩](#r01)
### API-011 — `request_dsr_erasure`

**Symbol:** `PilotHarness::request_dsr_erasure(&PilotTenant, &str) -> Result<DsrErasureReceipt, PilotHarnessError>`. **Guard:** local lifecycle must exist and request ID must be new. **Effect/output:** audit event then inserts request ID and fixed request time; receipt deadline is fixed time plus seven days. **Errors:** `TenantNotProvisioned`, `AlreadyRequested`, wrapped local audit/lock invariant. **INV/REL/proof:** INV-007; REL-009, REL-016–REL-017; `harness_lifecycle.rs:17–56`.
[Contract index](#r04)

<a id="api-012"></a>
[↩](#r01)
### API-012 — `finalise_dsr_erasure`

**Symbol:** `PilotHarness::finalise_dsr_erasure(&PilotTenant, u64) -> Result<(), PilotHarnessError>`. **Guard:** local lifecycle and prior request timestamp; caller time must reach fixed seven-day deadline. **Effect:** emits completion, clears CAS/prior audit rows, reanchors one completion tombstone. **Errors:** missing tenant/request is `TenantNotProvisioned`; early time is `SlaWindowOpen { remaining_ms }`; tombstone serialization/decode/lock invariant may fail after clearing and leave partial local state. **No external erasure or compensation is established.** **INV/REL/proof:** INV-007; REL-010, REL-016–REL-017; `harness_lifecycle.rs:58–128`.
[Contract index](#r04)

<a id="api-013"></a>
[↩](#r01)
### API-013 — `cancel_subscription`

**Symbol:** `PilotHarness::cancel_subscription(&PilotTenant) -> Result<(), PilotHarnessError>`. **Guard:** local lifecycle exists; source does not require Active state. **Effect:** emits cancellation row, sets Cancelled, fixed cancellation time, and CancelledInGrace with 30-day deadline. **Error:** `TenantNotProvisioned` or wrapped local audit/lock invariant. Name/event do not prove billing-provider cancellation. **INV/REL/proof:** INV-008; REL-011, REL-017; `harness_lifecycle.rs:142–170`.
[Contract index](#r04)

<a id="api-014"></a>
[↩](#r01)
### API-014 — `complete_offboarding`

**Symbol:** `PilotHarness::complete_offboarding(&PilotTenant, u64) -> Result<OffboardingReceipt, PilotHarnessError>`. **Guard:** local state exists, cancellation timestamp exists, and 30-day grace elapsed. **Effect:** emits hard-delete row, clears CAS and local audit, sets HardDeleted, then checks local residuals and returns receipt. **Errors:** not provisioned, not cancelled, grace open, residual rows/blobs, wrapped audit/lock invariant. No external sink is present in `PilotHarness`. **INV/REL/proof:** INV-008; REL-012, REL-017; `harness_lifecycle.rs:172–235`.
[Contract index](#r04)

<a id="api-015"></a>
[↩](#r01)
### API-015 — Local state inspectors

| Symbol and exact output | Missing slug or poisoned mutex |
|---|---|
| `cas_blob_count(&self, &PilotTenant) -> usize` | 0 |
| `audit_row_count(&self, &PilotTenant) -> usize` | 0 |
| `lifecycle_state(&self, &PilotTenant) -> Option<TenantLifecycleState>` | `None` |
| `subscription_state(&self, &PilotTenant) -> SubscriptionState` | `SubscriptionState::None` |
| `audit_snapshot(&self, &PilotTenant) -> Vec<AuditRecord>` | empty vector |

All are read-only local inspectors; defaults can mask lock poison. They return no
provider evidence. Source: `harness.rs:697–714,850–894`; REL-004–012.
[Contract index](#r04)

<a id="api-016"></a>
[↩](#r01)
### API-016 — `PilotTenant` and harness shape

`PilotTenant { slug: String, tenant_id: String, subscription_id: String, admin_email: String }`; `PilotHarness` is opaque (API-005). `PilotTenant` is `#[non_exhaustive]`; `slug` keys state and `tenant_id` feeds audit/receipts (`lib.rs:56–69`; REL-002). [Contract index](#r04)

<a id="api-017"></a>
[↩](#r01)
### API-017 — Receipt shapes

`SignupReceipt { tenant_id: String, subscription_id: String, activated_at_ms: u64 }`; `BatchUploadReceipt { blobs_uploaded: usize, tenant_prefix: String, digests_hex: Vec<String> }`; `DsrErasureReceipt { request_id: String, sla_deadline_ms: u64 }`; `OffboardingReceipt { tenant_id: String, deleted_at_ms: u64, residual_cas_blobs: usize, residual_audit_rows: usize }`. All are public-field `#[non_exhaustive]` structs; values are local (`harness.rs:251–298`). [Contract index](#r04)

<a id="api-018"></a>
[↩](#r01)
### API-018 — Signup and CAS error variants

`PilotHarnessError` wraps Signup, Cas, AuditExport, AuditChain, DsrErasure, Offboarding, or `Invariant(&'static str)`. `SignupError`: AlreadyProvisioned(String), CheckoutSessionMissing(String), WebhookSignatureRejected. `CasError`: SubscriptionNotActive(String), TenantNotProvisioned(String), DigestMismatch, TenantPrefixViolation. All are `#[non_exhaustive]` (`harness.rs:112–170`). [Contract index](#r04)

<a id="api-019"></a>
[↩](#r01)
### API-019 — Export and chain errors

`AuditExportError`: InvalidWindow, TenantNotProvisioned(String). `AuditChainError`: ChainBreak { prev_index: usize, next_index: usize }, ManifestDigestMismatch, ManifestRowCountMismatch. Both are `#[non_exhaustive]` and wrapped by `PilotHarnessError` (`harness.rs:172–203`). [Contract index](#r04)

<a id="api-020"></a>
[↩](#r01)
### API-020 — DSR and offboarding errors

`DsrErasureError`: TenantNotProvisioned(String), SlaWindowOpen { remaining_ms: u64 }, AlreadyRequested(String). `OffboardingError`: TenantNotProvisioned(String), SubscriptionNotCancelled(String), GraceNotElapsed { remaining_ms: u64 }, ResidualCasData(usize), ResidualAuditRows(usize). Both are `#[non_exhaustive]` (`harness.rs:205–249`). [Contract index](#r04)

<a id="api-021"></a>
[↩](#r01)
### API-021 — Local state enum values

`TenantLifecycleState`: PendingActivation, Active, CancelledInGrace { grace_ends_at_ms: u64 }, HardDeleted. `SubscriptionState`: None (default), CheckoutPending, Active, Cancelled. Both are `#[non_exhaustive]`; values do not establish provider state (`harness.rs:300–333`). [Contract index](#r04)

<a id="api-022"></a>
[↩](#r01)
### API-022 — `AuditEventKind` variants

Variants: SignupRequested, CheckoutSessionCreated, CheckoutWebhookVerified, TenantProvisioned, BatchUploadAccepted, BlobPutCommitted, AuditExportStarted, AuditExportSealed, DsrErasureRequested, DsrErasureCompleted, SubscriptionCancelled, TenantHardDeleted. The `#[non_exhaustive]` enum order is source-only, not an external audit sink (`harness.rs:335–365`). [Contract index](#r04)

<a id="api-023"></a>
[↩](#r01)
### API-023 — `AuditRecord` public shape

`AuditRecord { tenant_id: String, seq: u64, ts_ms: u64, kind: AuditEventKind, detail: serde_json::Value, row_hash_hex: String, prev_hash_hex: String }`; public `#[non_exhaustive]` struct; stored by slug, with `tenant_id` receiving `tenant.tenant_id` (`harness.rs:367–385`; `harness_lifecycle.rs:17–235`). [Contract index](#r04)

<a id="api-024"></a>
[↩](#r01)
### API-024 — `AuditExportManifest` public shape

`AuditExportManifest { tenant_id: String, window_start_ms: u64, window_end_ms: u64, row_count: usize, body_digest_hex: String, first_row_hash_hex: Option<String>, last_row_hash_hex: Option<String> }`; public-field `#[non_exhaustive]` serializable struct; export window is half-open (`harness.rs:388–405`). [Contract index](#r04)

<a id="api-025"></a>
[↩](#r01)
### API-025 — Exported constants

`FIXED_NOW_MS: u64 = 1_700_000_000_000`; `ONE_DAY_MS: u64 = 86_400_000`; `CANONICAL_BLOB_COUNT: usize = 100`; `DSR_ERASURE_WINDOW_MS: u64 = 604_800_000`; `OFFBOARDING_GRACE_MS: u64 = 2_592_000_000`. Compile-time source values, not runtime clocks/SLA evidence (`harness.rs:18–35`). [Contract index](#r04)
[↩](#r01)

<a id="r05"></a>
## R05 — State, flows, and falsifiable invariants

| State/resource | Key and owner | Lifetime/time | Write/read and durability |
|---|---|---|---|
| Harness tenants | `Mutex<HashMap<String,TenantState>>`, key is slug; `PilotHarness` owns it | Instance lifetime; `FIXED_NOW_MS` or method argument | In-memory only; no persistence claim (`harness.rs:412–449`) |
| CAS-shaped bytes | Each tenant state's `BTreeMap<String,Vec<u8>>`, key is digest hex | Until local erase/offboard or instance drop | `batch_upload_blobs` inserts by digest; `read_blob` searches local maps |
| Audit rows and sequence/hash head | Per-slug vector in `TenantState`; each row's `tenant_id` is the caller's `PilotTenant.tenant_id` | Local, ordered by emitted sequence; fixed or explicit timestamp | `emit_audit` receives `&tenant.tenant_id`, hashes JSON, and appends; no external sink field |
| Erasure/cancellation times and IDs | Per-slug fields/sets | Local; fixed request/cancel time, supplied completion time | Lifecycle methods only; no scheduler or real clock |

**Invariant index:** [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003) · [INV-004](#inv-004) · [INV-005](#inv-005) · [INV-006](#inv-006) · [INV-007](#inv-007) · [INV-008](#inv-008) · [FLOW-001](#flow-001).

<a id="inv-001"></a>
### INV-001 — Fixture derivation

**Predicate:** tenant fields derive from slug, payload from slug/index, and digest is BLAKE3(payload). Falsify by changing helper inputs/derivation. Source `harness.rs:58–101`; unit assertions unrun. No external identity provenance.

[Invariant index](#r05)

<a id="inv-002"></a>
[↩](#r01)
### INV-002 — Tenant partition and local audit chain

**Predicate:** `with_tenant` partitions by slug; `emit_audit` advances sequence and prior digest in local state. Falsify by changing either helper. Source `harness.rs:465–511`; no concurrency/runtime observation.

[Invariant index](#r05)

<a id="inv-003"></a>
[↩](#r01)
### INV-003 — Signup local transition and audit ordering

**Predicate:** signup ends Active; PendingActivation/CheckoutPending precede `CheckoutSessionCreated`, Active follows `TenantProvisioned`. Test 01 asserts final state/event order, not operation order, falsifying audit-before-every-mutation prose. Source `harness.rs:525–585`.

[Invariant index](#r05)

<a id="inv-004"></a>
[↩](#r01)
### INV-004 — Upload eligibility and digest-keyed local bytes

**Predicate:** upload requires lifecycle and Active subscription; local bytes use digest keys in the slug partition; unequal bytes for same digest error. Prefix is receipt text only. Test 02 covers digest/count/round-trip/negative lookups, not external CAS. Insert precedes blob audit (`harness.rs:607–659`).

[Invariant index](#r05)

<a id="inv-005"></a>
[↩](#r01)
### INV-005 — Export window and output manifest

**Predicate:** half-open window selects prior rows; digest/count cover NDJSON. Falsify by altering filter/serialization/manifest. Test 03 covers output; start precedes snapshot, seal follows composition (`harness.rs:717–803`). No delivery/storage claim.

[Invariant index](#r05)

<a id="inv-006"></a>
[↩](#r01)
### INV-006 — Export verifier limits

**Predicate:** verifier checks digest, parse, count, adjacent links; it does not recompute row hashes or all manifest fields. Test 03 covers round-trip/tamper/invalid window. Source assertions are unrun (`harness.rs:805–846`).

[Invariant index](#r05)

<a id="inv-007"></a>
[↩](#r01)
### INV-007 — DSR request/finalization

**Predicate:** request ID unique per local tenant; finalization needs a request plus seven elapsed days, then drains CAS/prior rows and keeps one tombstone. Test 04 covers duplicate/missing/early/success. Source `harness_lifecycle.rs:17–128`; not a real DSR/SLA result.

[Invariant index](#r05)

<a id="inv-008"></a>
[↩](#r01)
### INV-008 — Cancellation/offboarding local grace

**Predicate:** cancel sets local Cancelled and fixed-time grace; completion needs 30 elapsed days, clears maps, sets HardDeleted, checks residuals. Test 05 covers no-cancel/early/success/post-DSR (`harness_lifecycle.rs:142–235`). No external deletion.

[Invariant index](#r05)

<a id="flow-001"></a>
[↩](#r01)
### FLOW-001 — Local lifecycle composition

Five tests compose local calls on one mutex-backed harness, not a runtime workflow;
rollback is not established. Pending signup state precedes checkout audit and CAS
inserts precede blob audit. Tests assert final state/event sequence, not order.
Sources: harness modules and tests 01–05.

[Invariant index](#r05)
[↩](#r01)

<a id="r06"></a>
## R06 — Configuration, targets, and features

| Declaration | Origin | Default / read point | Target | Effect | Validation / failure |
|---|---|---|---|---|---|
| Package metadata/deps | `Cargo.toml:1–19` | Workspace values; no env read | Package/library; 5 deps | Identity/dependency inputs; unresolved values | Source only; resolution/build unknown |
| Library/unit tests | `Cargo.toml:13–14`; `src/lib.rs` | Library default; Cargo selects units | Library; 3 units | Façade/helper checks | Static only; compile/test unknown |
| Integration targets | `Cargo.toml:21–37` | No flags/target env reads | 5 listed targets | Activates selected scenarios; no provider effect | Static; selection/run unknown |
| Features/environment | No `[features]` or package env read | Empty features; fixed constants/args | No feature/env branch | No configurable effect | Static absence; external env unknown |

<a id="r07"></a>
## R07 — Failures and observability

| Error/signal | Source cause | State after failure | Diagnosis / observability |
|---|---|---|---|
| Signup duplicate | Active/grace/deleted guard | Unchanged | `SignupError`; test 01; no alert |
| Upload missing/inactive/digest mismatch/cross-slug | Guards/digest/map; audit may fail after insert | Guard unchanged; prior insert may persist | Error/test 02; local inspectors |
| Export invalid window/missing lifecycle | Window/lifecycle; post-start audit may fail | Precondition unchanged; no rollback after start | Error/test 03; in-memory rows |
| Verify digest/count/chain/parse | Digest, parse/count, or link check | No state mutation | Error/test 03; no alert |
| DSR duplicate/missing/early | Request/lifecycle/deadline guard | Early unchanged; cleanup may be partial | Error/test 04; local counts |
| Offboard missing/not-cancelled/early/residual | Lifecycle/cancel/grace/residual checks | Guard unchanged; post-wipe may be partial | Error/test 05; no external telemetry |
| `Invariant` | Lock, serialization, decode, or hash failure | Rollback varies | Error + inspectors; no runtime observability |

No metrics/alerts contract is established. Audit rows are in-memory, not an external sink.

<a id="r08"></a>
## R08 — Verification, fixed axioms, and unknowns

| Surface | Evidence boundary |
|---|---|
| Manifests/methods | SOURCE declarations and inspection; no resolution/build/test |
| Scenarios | five test files, 14 functions; assertions unrun |
| Consumers | two verifier scripts and two importing tests; not run |
| CI | four workflow command declarations; no invocation/result |

**Five axioms:** identity is `[package].name`; declaration does not prove resolution;
workspace/name/fake/feature gate does not prove reachability/runtime; providers are
out of scope; changed bytes need fresh independent review.

**Unresolved domains:**

1. Cargo target/feature resolution, build, selected tests, and CI results.
2. Reverse consumers; production composition/provider/runtime, durability, auth, and recovery.
3. Timing/no-network/compliance claims beyond source; owner and cold-review authority.

**Documentary conflict, not runtime truth:** `docs/testing/2026-06-23-gapmap-journeys.md`
and `docs/testing/2026-06-23-gapmap-quality.md` claim D1/R2 behavior despite no
direct first-party dependency and local slug/digest maps.
`specs/_runbooks/RB-PILOT-ONBOARDING-E2E.md` and source comments claim
audit-before-mutation; source mutates pending signup state before checkout audit and
inserts CAS before blob audit. Tests assert final state/event sequence, not order.
Treat these as conflicting prose.

[Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Ownership skill](../../../../.claude/skills/own-e2e-pilot-onboarding/SKILL.md#s01)
