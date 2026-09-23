---
schema: corelink-ownership/1.1
document: reference
package: chaos-campaign
manifest: tests/chaos/Cargo.toml
source_commit: cb94e251c0f17382565bf863f517945cbb2a84d6
profile: S
state: candidate
evidence_set: chaos-campaign-static-cb94e251c
---

# chaos-campaign — ownership reference

Static `chaos-campaign` record; manifest prose is intent. Source does not prove execution, wiring, external effects, or delivery. [OKF](../../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md) is route-only; not copied/revalidated.

[Identity](#r01) · [Boundary](#r02) · [Implementation](#r03) · [Contracts](#r04) · [State](#r05) · [Configuration](#r06) · [Failures](#r07) · [Evidence](#r08).

[Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Skill](../../../../.claude/skills/own-chaos-campaign/SKILL.md#s01).

<a id="r01"></a>
## R01 — Identity and declared targets

| Field | Static value / evidence |
|---|---|
| Cargo identity | `package.name = "chaos-campaign"`; manifest `tests/chaos/Cargo.toml`; `publish = false`; lines 1–8. |
| Inherited values | `version`, `edition`, `rust-version`, `license` use workspace values; manifest lines 4–7. |
| Feature/lints | `default = []`; `chaos = []`; `[lints] workspace = true`; lines 10–16. |
| Library | implicit library name `chaos_campaign`, path `src/lib.rs`; lines 18–19. |
| Workspace | root `Cargo.toml:329` lists `tests/chaos`; membership is not target selection. |
| Dependency declarations | No normal, build, dev, or target-specific dependency table exists in complete manifest lines 1–66. |

All 11 `[[test]]` declarations below are individual targets; paths are relative, and every source has `#![cfg(feature = "chaos")]`.

| Target name | Declared source path | Manifest lines |
|---|---|---:|
| `campaign_network_partition_failover` | `tests/campaign_network_partition_failover.rs` | 21–23 |
| `campaign_d1_pool_exhaustion_degrades_gracefully` | `tests/campaign_d1_pool_exhaustion_degrades_gracefully.rs` | 25–27 |
| `campaign_neon_shadow_silent_failure_alerts` | `tests/campaign_neon_shadow_silent_failure_alerts.rs` | 29–31 |
| `campaign_rls_guc_dropout_rejects_insert` | `tests/campaign_rls_guc_dropout_rejects_insert.rs` | 33–35 |
| `campaign_byok_provider_503_fails_closed` | `tests/campaign_byok_provider_503_fails_closed.rs` | 37–39 |
| `campaign_stripe_webhook_timestamp_drift_rejected` | `tests/campaign_stripe_webhook_timestamp_drift_rejected.rs` | 41–43 |
| `campaign_clerk_jwks_rotation_recovers` | `tests/campaign_clerk_jwks_rotation_recovers.rs` | 45–47 |
| `campaign_cas_multipart_abort_cleanup` | `tests/campaign_cas_multipart_abort_cleanup.rs` | 49–51 |
| `campaign_combined_partition_plus_byok_503` | `tests/campaign_combined_partition_plus_byok_503.rs` | 56–58 |
| `campaign_combined_d1_exhaustion_plus_stripe_drift` | `tests/campaign_combined_d1_exhaustion_plus_stripe_drift.rs` | 60–62 |
| `campaign_combined_neon_shadow_failure_plus_audit_export` | `tests/campaign_combined_neon_shadow_failure_plus_audit_export.rs` | 64–66 |

<a id="r02"></a>
## R02 — Ownership boundary

| Surface | What source owns | Explicit boundary |
|---|---|---|
| Library | 8 in-memory models, 2 helpers; `std` only. | No production calls. |
| Test targets | 11 declared files; 16 source functions have source-level cfg gates. | Cargo target eligibility, compilation, and invocation are UNKNOWN absent execution evidence. |
| Combined target C | Private `ExportTrailerModel`. | Fixture, not production export. |
| Workspace/package | Membership, inherited values. | Graph, inverse consumer, CI, artifact unknown. |

Source owner is UNKNOWN; names do not prove production ownership. OKF is route/reference only.

<a id="r03"></a>
## R03 — Implementation map

| Source/module | Role and owned state | Evidence |
|---|---|---|
| `src/lib.rs` | Public models/transitions, vectors, helpers; three lints. | `lib.rs:55–59,65–809`. |
| Failover | `CampaignFailoverModel`; health, audit/SEV-1. | `lib.rs:62–174`. |
| D1 pool | `CampaignD1Pool`; capacity/use, audit/SEV-2. | `lib.rs:177–245`. |
| Dual write | `CampaignAuditDualWrite`; archive/shadow, silent flag, audit/SEV-2. | `lib.rs:247–343`. |
| RLS | `CampaignRlsTable`; tenant/row map, audit/SEV-1. | `lib.rs:345–426`. |
| BYOK | `CampaignByokModel`; availability, audit/SEV-1. | `lib.rs:428–528`. |
| Webhook | `CampaignWebhookVerifier`; fixed window, audit/SEV-2. | `lib.rs:530–601`. |
| JWKS | `CampaignClerkJwks`; cached/upstream IDs, audit/INFO. | `lib.rs:603–673`. |
| Multipart | `CampaignMultipart`; state/queue, audit/INFO. | `lib.rs:675–772`. |
| Helpers | Audit-once/alert-present inspect supplied slices. | `lib.rs:774–809`. |
| Test sources | Eight isolated, three combined; 16 tests/11 files. | `tests/chaos/tests/`; not run. |

<a id="r04"></a>
## R04 — Public contracts

Static `lib.rs` declarations; test names are not results.

Public structs (private fields): `CampaignFailoverModel`, `CampaignD1Pool`, `CampaignAuditDualWrite`, `CampaignRlsTable`, `CampaignByokModel`, `CampaignWebhookVerifier`, `CampaignClerkJwks`, `CampaignMultipart`. Enum variants and methods follow.

Trait sets name exact public derives in `lib.rs`.

<a id="api-index"></a>API index: [API-001](#api-001),[API-002](#api-002),[API-003](#api-003),[API-004](#api-004),[API-005](#api-005),[API-006](#api-006),[API-007](#api-007),[API-008](#api-008),[API-009](#api-009),[API-010](#api-010),[API-011](#api-011),[API-012](#api-012),[API-013](#api-013),[API-014](#api-014),[API-015](#api-015),[API-016](#api-016),[API-017](#api-017),[API-018](#api-018)

[API-019](#api-019),[API-020](#api-020),[API-021](#api-021),[API-022](#api-022),[API-023](#api-023),[API-024](#api-024),[API-025](#api-025),[API-026](#api-026),[API-027](#api-027),[API-028](#api-028),[API-029](#api-029),[API-030](#api-030),[API-031](#api-031),[API-032](#api-032),[API-033](#api-033),[API-034](#api-034),[API-035](#api-035),[API-036](#api-036)

[API-037](#api-037),[API-038](#api-038),[API-039](#api-039),[API-040](#api-040),[API-041](#api-041),[API-042](#api-042),[API-043](#api-043),[API-044](#api-044),[API-045](#api-045),[API-046](#api-046),[API-047](#api-047),[API-048](#api-048),[API-049](#api-049),[API-050](#api-050),[API-051](#api-051),[API-052](#api-052),[API-053](#api-053),[API-054](#api-054),[API-055](#api-055)

| API | Exact public symbols/signatures | Inputs/preconditions beyond signature | Output / effects; errors | INV / evidence / status |
|---|---|---|---|---|
| <a id="api-001"></a>[API-001](#api-001)&nbsp;[↩](#api-index) | `CampaignRegion::{UsEast,EuWest}` | No input. | Fixed region identifiers; derives `Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord`. | INV-001; `lib.rs:67–68`; source only. |
| <a id="api-002"></a>[API-002](#api-002)&nbsp;[↩](#api-index) | `RegionHealth::{Healthy,Degraded,Down}` | No input. | Health labels; derives `Debug, Clone, Copy, PartialEq, Eq`. | INV-001; `lib.rs:76–77`; source only. |
| <a id="api-003"></a>[API-003](#api-003)&nbsp;[↩](#api-index) | `RouteOutcome::{Served(CampaignRegion),FailedClosed503}` | Constructed by routing methods. | Local result, not HTTP; derives `Debug, Clone, PartialEq, Eq`. | INV-001; `lib.rs:87–88`; source only. |
| <a id="api-004"></a>[API-004](#api-004)&nbsp;[↩](#api-index) | `CampaignFailoverModel::new()->Self` | No input. | Fresh map with both regions Healthy; empty vectors; type derives `Debug, Default`. | INV-001; `lib.rs:98–115`; source only. |
| <a id="api-005"></a>[API-005](#api-005)&nbsp;[↩](#api-index) | `CampaignFailoverModel::inject_partition(&mut self,r:CampaignRegion)` | — | Sets region Down; appends degraded audit and `region_isolated` SEV-1 strings. | INV-001; network-partition source; SOURCE_ONLY. |
| <a id="api-006"></a>[API-006](#api-006)&nbsp;[↩](#api-index) | `CampaignFailoverModel::heal(&mut self,r:CampaignRegion)` | — | Sets region Healthy; appends recovered audit string; no alert clear. | INV-001; network-partition source; SOURCE_ONLY. |
| <a id="api-007"></a>[API-007](#api-007)&nbsp;[↩](#api-index) | `CampaignFailoverModel::route_write(&self,region:CampaignRegion)->RouteOutcome` | — | `Served(region)` iff mapped Healthy; otherwise `FailedClosed503`. | INV-001; network-partition source; SOURCE_ONLY. |
| <a id="api-008"></a>[API-008](#api-008)&nbsp;[↩](#api-index) | `CampaignFailoverModel::route_read(&self,primary:CampaignRegion)->RouteOutcome` | — | Serve Healthy primary; else Healthy partner; else `FailedClosed503`. | INV-001; network-partition source; SOURCE_ONLY. |
| <a id="api-009"></a>[API-009](#api-009)&nbsp;[↩](#api-index) | `D1AcquireOutcome::{Acquired,Pool503{retry_after_secs:u32}}` | Constructed by pool acquire. | Local outcome/hint, no HTTP header; derives `Debug, Clone, PartialEq, Eq`. | INV-002; `lib.rs:181–182`; source only. |
| <a id="api-010"></a>[API-010](#api-010)&nbsp;[↩](#api-index) | `CampaignD1Pool::new(capacity:u32)->Self` | — | Any `u32` including zero; initializes `in_use=0` and empty audit/SEV-2 vectors; type derives `Debug`. | INV-002; `lib.rs:195–196`; source only. |
| <a id="api-011"></a>[API-011](#api-011)&nbsp;[↩](#api-index) | `CampaignD1Pool::inject_saturation(&mut self)` | — | Sets `in_use=capacity`; no connection allocation. | INV-002; D1 exhaustion source; SOURCE_ONLY. |
| <a id="api-012"></a>[API-012](#api-012)&nbsp;[↩](#api-index) | `CampaignD1Pool::acquire(&mut self)->D1AcquireOutcome` | — | If `in_use >= capacity`, appends audit/SEV-2 and returns `Pool503{retry_after_secs:2}`; otherwise increments and returns Acquired. | INV-002; D1 exhaustion source; SOURCE_ONLY. |
| <a id="api-013"></a>[API-013](#api-013)&nbsp;[↩](#api-index) | `SinkPersistResult::{Persisted,SilentDrop}` | Returned from `emit`. | Local labels, no sink acknowledgement; derives `Debug, Clone, PartialEq, Eq`. | INV-003; `lib.rs:252–253`; source only. |
| <a id="api-014"></a>[API-014](#api-014)&nbsp;[↩](#api-index) | `CampaignAuditDualWrite::new()->Self` | No input. | Empty vectors; silent flag false; type derives `Debug`. | INV-003; `lib.rs:263–264`; source only. |
| <a id="api-015"></a>[API-015](#api-015)&nbsp;[↩](#api-index) | `CampaignAuditDualWrite::inject_shadow_silent_failure(&mut self)` | — | Sets local silent flag true; no external sink fault. | INV-003; silent-failure source; SOURCE_ONLY. |
| <a id="api-016"></a>[API-016](#api-016)&nbsp;[↩](#api-index) | `CampaignAuditDualWrite::emit(&mut self,row:&str)->SinkPersistResult` | — | Always appends to archive; appends shadow and returns Persisted unless silent, when shadow is unchanged and SilentDrop returns. | INV-003; silent-failure source; SOURCE_ONLY. |
| <a id="api-017"></a>[API-017](#api-017)&nbsp;[↩](#api-index) | `CampaignAuditDualWrite::reconcile(&mut self)->bool` | — | Compares vector lengths only; mismatch appends audit/SEV-2 and returns false; equal lengths return true even if contents differ. | INV-003; silent-failure source; SOURCE_ONLY. |
| <a id="api-018"></a>[API-018](#api-018)&nbsp;[↩](#api-index) | `CampaignAuditDualWrite::r2_archive(&self)->&[String]`, `CampaignAuditDualWrite::neon_shadow(&self)->&[String]` | — | Read-only slice of named local vector; not R2/Neon data. | INV-003; silent-failure source; unrun. |
| <a id="api-019"></a>[API-019](#api-019)&nbsp;[↩](#api-index) | `RlsInsertOutcome::{Inserted,PolicyRejected}` | Returned by local insert model. | Local label, no SQL result; derives `Debug, Clone, PartialEq, Eq`. | INV-004; `lib.rs:350–351`; source only. |
| <a id="api-020"></a>[API-020](#api-020)&nbsp;[↩](#api-index) | `CampaignRlsTable::new()->Self` | No input. | Empty tenant/map/vectors; type derives `Debug, Default`. | INV-004; `lib.rs:360–373`; source only. |
| <a id="api-021"></a>[API-021](#api-021)&nbsp;[↩](#api-index) | `CampaignRlsTable::set_session_tenant(&mut self,tenant:impl Into<String>)` | — | Stores local tenant string; no database GUC. | INV-004; RLS dropout source; SOURCE_ONLY. |
| <a id="api-022"></a>[API-022](#api-022)&nbsp;[↩](#api-index) | `CampaignRlsTable::drop_session_tenant(&mut self)` | — | Clears local tenant option. | INV-004; RLS dropout source; SOURCE_ONLY. |
| <a id="api-023"></a>[API-023](#api-023)&nbsp;[↩](#api-index) | `CampaignRlsTable::insert(&mut self,claimed_tenant:&str,row_id:&str)->RlsInsertOutcome` | — | Inserts row iff current tenant exists and equals claimed; else emits local audit/SEV-1 and rejects. | INV-004; RLS dropout source; SOURCE_ONLY. |
| <a id="api-024"></a>[API-024](#api-024)&nbsp;[↩](#api-index) | `CampaignRlsTable::rows_for(&self,tenant:&str)->Vec<&str>` | — | Returns sorted borrowed row IDs for tenant, or empty vector. | INV-004; RLS dropout source; SOURCE_ONLY. |
| <a id="api-025"></a>[API-025](#api-025)&nbsp;[↩](#api-index) | `ByokProvider::{AwsKms,GcpKms,AzureKv,Vault}` | No input. | Ordered keys; derives `Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord`. | INV-005; `lib.rs:433–434`; source only. |
| <a id="api-026"></a>[API-026](#api-026)&nbsp;[↩](#api-index) | `ByokOutcome::{KeyAcquired(ByokProvider),ProviderUnavailable503}` | Returned by acquire. | Local provider label, no key bytes; derives `Debug, Clone, PartialEq, Eq`. | INV-005; `lib.rs:446–447`; source only. |
| <a id="api-027"></a>[API-027](#api-027)&nbsp;[↩](#api-index) | `CampaignByokModel::new()->Self` | No input. | Four entries available; empty vectors; type derives `Debug`. | INV-005; `lib.rs:457–458`; source only. |
| <a id="api-028"></a>[API-028](#api-028)&nbsp;[↩](#api-index) | `CampaignByokModel::inject_provider_503(&mut self,p:ByokProvider)` | — | Sets that map entry false. | INV-005; BYOK source; SOURCE_ONLY. |
| <a id="api-029"></a>[API-029](#api-029)&nbsp;[↩](#api-index) | `CampaignByokModel::acquire_data_key(&mut self,preferred:ByokProvider)->ByokOutcome` | — | Return preferred if true; else first true BTreeMap entry; else append local audit/SEV-1 and return unavailable. | INV-005; BYOK source; SOURCE_ONLY. |
| <a id="api-030"></a>[API-030](#api-030)&nbsp;[↩](#api-index) | `WebhookOutcome::{Accepted,OutsideReplayWindow,SignatureMismatch}` | Returned by verifier. | Local labels; no cryptographic verification; derives `Debug, Clone, PartialEq, Eq`. | INV-006; `lib.rs:535–536`; source only. |
| <a id="api-031"></a>[API-031](#api-031)&nbsp;[↩](#api-index) | `CampaignWebhookVerifier::new()->Self` | No input. | 300 signed seconds; empty vectors; type derives `Debug`. | INV-006; `lib.rs:547–571`; source only. |
| <a id="api-032"></a>[API-032](#api-032)&nbsp;[↩](#api-index) | `CampaignWebhookVerifier::verify(&mut self,signed_ok:bool,sender_ts:i64,verifier_now:i64)->WebhookOutcome` | — | False signature appends only invalid-signature audit and returns SignatureMismatch. Otherwise computes `(verifier_now-sender_ts).abs()`; drift `>300` appends audit/SEV-2, else Accepted. | INV-006; webhook source; SOURCE_ONLY; extreme arithmetic caveat in R07. |
| <a id="api-033"></a>[API-033](#api-033)&nbsp;[↩](#api-index) | `JwksOutcome::{VerifiedCached(String),VerifiedAfterRefetch(String),Unverified}` | Returned by verifier. | Local key labels, no token/crypto; derives `Debug, Clone, PartialEq, Eq`. | INV-007; `lib.rs:608–609`; source only. |
| <a id="api-034"></a>[API-034](#api-034)&nbsp;[↩](#api-index) | `CampaignClerkJwks::new(kid:impl Into<String>)->Self` | — | Initial cached/upstream strings clone input; type derives `Debug`. | INV-007; `lib.rs:621–622`; source only. |
| <a id="api-035"></a>[API-035](#api-035)&nbsp;[↩](#api-index) | `CampaignClerkJwks::inject_rotation(&mut self,new_kid:impl Into<String>)` | — | Replaces local upstream key only. | INV-007; JWKS source; SOURCE_ONLY. |
| <a id="api-036"></a>[API-036](#api-036)&nbsp;[↩](#api-index) | `CampaignClerkJwks::verify(&mut self,signing_kid:&str)->JwksOutcome` | — | Cached equality verifies; miss copies upstream, appends audit/INFO, then verifies equality or returns Unverified. | INV-007; JWKS source; SOURCE_ONLY. |
| <a id="api-037"></a>[API-037](#api-037)&nbsp;[↩](#api-index) | `MultipartState::{Open,Aborted,Completed}` | No input. | Local states; derives `Debug, Clone, PartialEq, Eq`. | INV-008; `lib.rs:680–681`; source only. |
| <a id="api-038"></a>[API-038](#api-038)&nbsp;[↩](#api-index) | `CampaignMultipart::open()->Self` | No input. | Open state, empty queue/vectors; type derives `Debug`. | INV-008; `lib.rs:693–694`; source only. |
| <a id="api-039"></a>[API-039](#api-039)&nbsp;[↩](#api-index) | `CampaignMultipart::stage_part(&mut self,n:u32)` | — | Pushes number only while Open; otherwise no-op. | INV-008; multipart source; SOURCE_ONLY. |
| <a id="api-040"></a>[API-040](#api-040)&nbsp;[↩](#api-index) | `CampaignMultipart::abort(&mut self)` | — | Open→Aborted once and appends local audit/INFO; non-Open is no-op. | INV-008; multipart source; SOURCE_ONLY. |
| <a id="api-041"></a>[API-041](#api-041)&nbsp;[↩](#api-index) | `CampaignMultipart::complete(&mut self)->Result<(),&'static str>` | — | Open→Completed, `Ok(())`; other state returns static error. | INV-008; multipart source; SOURCE_ONLY. |
| <a id="api-042"></a>[API-042](#api-042)&nbsp;[↩](#api-index) | `CampaignMultipart::gc(&mut self)->usize` | — | Aborted returns prior queue length and clears queue; otherwise returns 0. | INV-008; multipart source; SOURCE_ONLY. |
| <a id="api-043"></a>[API-043](#api-043)&nbsp;[↩](#api-index) | `CampaignMultipart::state(&self)->&MultipartState` | — | Returns `&MultipartState` for this model. | INV-008; multipart source; unrun. |
| <a id="api-044"></a>[API-044](#api-044)&nbsp;[↩](#api-index) | `assert_audit_emitted_once(events:&[String],canonical:&str)->Result<(),String>` | — | `Ok` iff exactly one equal value; else descriptive `String` error. | INV-009; assertion-helper source; SOURCE_ONLY. |
| <a id="api-045"></a>[API-045](#api-045)&nbsp;[↩](#api-index) | `assert_alert_fired(alerts:&[String],canonical:&str)->Result<(),String>` | — | `Ok` iff at least one equal value; else descriptive `String` error. | INV-010; assertion-helper source; SOURCE_ONLY. |
| <a id="api-046"></a>[API-046](#api-046)&nbsp;[↩](#api-index) | `CampaignFailoverModel::audit_events(&self)->&[String]`; `CampaignD1Pool::audit_events(&self)->&[String]`; `CampaignAuditDualWrite::audit_events(&self)->&[String]`; `CampaignRlsTable::audit_events(&self)->&[String]`; `CampaignByokModel::audit_events(&self)->&[String]`; `CampaignWebhookVerifier::audit_events(&self)->&[String]`; `CampaignClerkJwks::audit_events(&self)->&[String]`; `CampaignMultipart::audit_events(&self)->&[String]` | — | Each returns its own model's append-only `&[String]`; no transport/delivery. | INV-001–008; source-only. |
| <a id="api-047"></a>[API-047](#api-047)&nbsp;[↩](#api-index) | `CampaignFailoverModel::sev1_alerts(&self)->&[String]`; `CampaignRlsTable::sev1_alerts(&self)->&[String]`; `CampaignByokModel::sev1_alerts(&self)->&[String]` | — | Each returns its own local `&[String]`; not an alert-manager call. | INV-001,004,005; source-only. |
| <a id="api-048"></a>[API-048](#api-048)&nbsp;[↩](#api-index) | `CampaignD1Pool::sev2_alerts(&self)->&[String]`; `CampaignAuditDualWrite::sev2_alerts(&self)->&[String]`; `CampaignWebhookVerifier::sev2_alerts(&self)->&[String]` | — | Each returns its own local `&[String]`; not a delivered alert. | INV-002,003,006; source-only. |
| <a id="api-049"></a>[API-049](#api-049)&nbsp;[↩](#api-index) | `CampaignClerkJwks::info_events(&self)->&[String]`; `CampaignMultipart::info_events(&self)->&[String]` | — | Each returns its own local `&[String]`; no event delivery. | INV-007,008; source-only. |
| <a id="api-050"></a>[API-050](#api-050)&nbsp;[↩](#api-index) | `CampaignMultipart::staged_parts(&self)->&VecDeque<u32>` | — | Returns `&VecDeque<u32>` for this model. | INV-008; multipart source; unrun. |
| <a id="api-051"></a>[API-051](#api-051)&nbsp;[↩](#api-index) | `<CampaignFailoverModel as Default>::default()->CampaignFailoverModel` | No input; derived at `lib.rs:98–103`. | Empty health map/vectors; unlike `new()`, default routes fail closed. | INV-011; `lib.rs:98–115,138–160`; SOURCE_ONLY. |
| <a id="api-052"></a>[API-052](#api-052)&nbsp;[↩](#api-index) | `<CampaignAuditDualWrite as Default>::default()->CampaignAuditDualWrite` | No input; at `lib.rs:272–275`. | Calls `new()`: empty vectors, `shadow_silent=false`. | INV-012; `lib.rs:272–289`; SOURCE_ONLY. |
| <a id="api-053"></a>[API-053](#api-053)&nbsp;[↩](#api-index) | `<CampaignRlsTable as Default>::default()->CampaignRlsTable` | No input; derived at `lib.rs:360`. | No tenant; empty rows/vectors; `new()` returns `default()`. | INV-012; `lib.rs:360–373`; SOURCE_ONLY. |
| <a id="api-054"></a>[API-054](#api-054)&nbsp;[↩](#api-index) | `<CampaignByokModel as Default>::default()->CampaignByokModel` | No input; explicit at `lib.rs:464–467`. | Calls `new()`: four providers available, empty vectors. | INV-012; `lib.rs:464–488`; SOURCE_ONLY. |
| <a id="api-055"></a>[API-055](#api-055)&nbsp;[↩](#api-index) | `<CampaignWebhookVerifier as Default>::default()->CampaignWebhookVerifier` | No input; explicit at `lib.rs:554–557`. | Calls `new()`: 300-second window, empty vectors. | INV-012; `lib.rs:554–571`; SOURCE_ONLY. |

**Compatibility (API-001–055):** No stability or semver guarantee is evidenced; consumer set is UNKNOWN.

**API blast/evidence.** Relation links: R02=[REL-002](BLAST_RADIUS.md#rel-002), R03=[REL-003](BLAST_RADIUS.md#rel-003), R04=[REL-004](BLAST_RADIUS.md#rel-004), R05=[REL-005](BLAST_RADIUS.md#rel-005), R06=[REL-006](BLAST_RADIUS.md#rel-006), R07=[REL-007](BLAST_RADIUS.md#rel-007), R08=[REL-008](BLAST_RADIUS.md#rel-008), R09=[REL-009](BLAST_RADIUS.md#rel-009), R10=[REL-010](BLAST_RADIUS.md#rel-010), R11=[REL-011](BLAST_RADIUS.md#rel-011), R12=[REL-012](BLAST_RADIUS.md#rel-012). API ranges: 001–008→R02/R10; 009–012→R03/R11; 013–018→R04/R12; 019–024→R05; 025–029→R06/R10; 030–032→R07/R11; 033–036→R08; 037–043→R09; 044–045→R02–R12; 051→R02; 052→R04; 053→R05; 054→R06; 055→R07. Atomized accessors: 046 `Failover.audit_events`→R02/R10, `D1.audit_events`→R03/R11, `DualWrite.audit_events`→R04/R12, `Rls.audit_events`→R05, `Byok.audit_events`→R06/R10, `Webhook.audit_events`→R07/R11, `Jwks.audit_events`→R08, `Multipart.audit_events`→R09; 047 `Failover.sev1`→R02/R10, `Rls.sev1`→R05, `Byok.sev1`→R06/R10; 048 `D1.sev2`→R03/R11, `DualWrite.sev2`→R04/R12, `Webhook.sev2`→R07/R11; 049 `Jwks.info`→R08, `Multipart.info`→R09; 050 `Multipart.staged_parts`→R09. `lib.rs:<range>` means `tests/chaos/src/lib.rs:<range>`; `cb94e251c0f17382565bf863f517945cbb2a84d6`; blob `3ed5d277170ae5b80f171d1c440797e18f6a2759`; static. Extra lines: 005–006=120–134, 011=217–220, 015=292–296, 021–022=376–384, 028=491–495, 035=643–646, 039–040=714–729, 044–045=785–809.

<a id="r05"></a>
## R05 — State, five axioms, and falsifiable invariants

Collections/counters are local, transient state; webhook seconds are caller inputs.

### Five source axioms

| Axiom | Falsifiable predicate and enforcement | Violation / check / state |
|---|---|---|
| A1 — Detached models | Docs say self-contained; no deps. | Adapter/dependency falsifies; docs + manifest; SOURCE_ONLY. |
| A2 — Default-off scenarios | `default=[]`, `chaos=[]`; 11 source cfg gates. | Gate change falsifies; inspect all; NOT EXECUTED. |
| A3 — Local rejection branches | Pool/tenant/provider/webhook/key/multipart failures are local. | Changed branch falsifies; INV-002/004–008; NOT EXECUTED. |
| A4 — Vectors are local records | Errors append strings; getters borrow slices. | External emission falsifies; source only, no delivery proof. |
| A5 — Recovery is bounded | Failover `heal`; multipart `abort`/`gc`; combined fixtures fresh. | New recovery falsifies; source inspection; NOT EXECUTED. |

### Invariant register

<a id="inv-index"></a>Invariant index: [INV-001](#inv-001),[INV-002](#inv-002),[INV-003](#inv-003),[INV-004](#inv-004),[INV-005](#inv-005),[INV-006](#inv-006),[INV-007](#inv-007),[INV-008](#inv-008),[INV-009](#inv-009),[INV-010](#inv-010),[INV-011](#inv-011),[INV-012](#inv-012)

| ID | Falsifiable predicate | Enforcement; violation | Test/check; status |
|---|---|---|---|
| <a id="inv-001"></a>[INV-001](#inv-001)&nbsp;[↩](#inv-index) | Writes serve only Healthy mapped regions; reads choose Healthy primary, partner, or fail closed. | `lib.rs:137–160`; unhealthy serve falsifies it. | `campaign_network_partition_failover.rs`; NOT_EXECUTED. |
| <a id="inv-002"></a>[INV-002](#inv-002)&nbsp;[↩](#inv-index) | At `in_use >= capacity`, acquire returns hint 2 + audit/SEV-2; else increments once. | `lib.rs:221–231`; wrong outcome/counter/event falsifies. | `campaign_d1_pool_exhaustion_degrades_gracefully.rs`; NOT_EXECUTED. |
| <a id="inv-003"></a>[INV-003](#inv-003)&nbsp;[↩](#inv-index) | Emit appends archive; silent skips shadow; reconcile compares lengths only. | `lib.rs:298–317`; equal-length drift passing falsifies stronger claims. | `campaign_neon_shadow_silent_failure_alerts.rs`; `campaign_combined_neon_shadow_failure_plus_audit_export.rs`; NOT_EXECUTED. |
| <a id="inv-004"></a>[INV-004](#inv-004)&nbsp;[↩](#inv-index) | Insert iff current tenant exists and equals claimed; rejection adds no row. | `lib.rs:385–404`; mismatched row falsifies. | `campaign_rls_guc_dropout_rejects_insert.rs`; NOT_EXECUTED. |
| <a id="inv-005"></a>[INV-005](#inv-005)&nbsp;[↩](#inv-index) | Prefer available requested provider, else first ordered available, else unavailable + audit/SEV-1. | `lib.rs:497–515`; wrong fallback/outcome falsifies. | `campaign_byok_provider_503_fails_closed.rs`; NOT_EXECUTED. |
| <a id="inv-006"></a>[INV-006](#inv-006)&nbsp;[↩](#inv-index) | If arithmetic is representable, false signature mismatches; drift `>300` rejects. | `lib.rs:574–585`; extremes uncovered. | `campaign_stripe_webhook_timestamp_drift_rejected.rs`; NOT_EXECUTED; extremes untested. |
| <a id="inv-007"></a>[INV-007](#inv-007)&nbsp;[↩](#inv-index) | Cache miss copies upstream string; verification requires key equality. | `lib.rs:647–659`; mismatch verifying falsifies. | `campaign_clerk_jwks_rotation_recovers.rs`; NOT_EXECUTED. |
| <a id="inv-008"></a>[INV-008](#inv-008)&nbsp;[↩](#inv-index) | Only Open stages/completes; abort is one-way; gc clears Aborted queue only. | `lib.rs:714–747`; other transition falsifies. | `campaign_cas_multipart_abort_cleanup.rs`; NOT_EXECUTED. |
| <a id="inv-009"></a>[INV-009](#inv-009)&nbsp;[↩](#inv-index) | Audit helper succeeds iff equal event count is one. | `lib.rs:785–794`; accepting zero/duplicate falsifies. | Helper source; NOT_EXECUTED. |
| <a id="inv-010"></a>[INV-010](#inv-010)&nbsp;[↩](#inv-index) | Alert helper succeeds iff an equal string exists. | `lib.rs:801–809`; accepting no-match falsifies. | Helper source; NOT_EXECUTED. |
| <a id="inv-011"></a>[INV-011](#inv-011)&nbsp;[↩](#inv-index) | Derived failover `Default` has an empty health map; `new()` has both regions Healthy, so default and new route outcomes differ. | `lib.rs:98–115,138–160`; a healthy default route falsifies this source contract. | Static source check; existing tests use `new()`, no public-default assertion found; unrun. |
| <a id="inv-012"></a>[INV-012](#inv-012)&nbsp;[↩](#inv-index) | Audit, RLS, BYOK and webhook `Default` return the same initialized state as their `new()` source paths. | `lib.rs:272–289,360–373,464–488,554–571`; state mismatch falsifies it. | Static source check; no direct public-default assertions found; unrun. |

<a id="r06"></a>
## R06 — Configuration and feature/target boundary

| Name/source | Default | Read/activation | Effect / failure limit |
|---|---|---|---|
| Cargo feature `chaos` | Off (`default=[]`) | Manifest declares 11 targets; each source has crate-level cfg. | Opt-in intent only; Cargo target eligibility, compilation, and invocation are UNKNOWN. |
| Webhook replay window | Literal `300` signed seconds. | Verifier call. | Rejects strict `>`; see R07 arithmetic caveat. |
| Pool capacity | Constructor `u32`; no package value. | Model construction. | Zero satisfies `in_use >= capacity`, returns saturated branch. |
| Other scenario state | Constructor-local constants/maps. | Synchronous calls. | No env/network/persistence/worker/runtime config declared. |

No mutually exclusive feature; `--all-features` unvalidated.

<a id="r07"></a>
## R07 — Critical failures and observability limits

| Source branch | Exact result and side effects | Boundary / recovery |
|---|---|---|
| Failover `lib.rs:138–160` | Write serves only mapped Healthy region; read falls back to Healthy partner or `FailedClosed503`. | Local enum; no router/HTTP response. |
| Audit dual-write `lib.rs:300–317` | Silent emit appends archive, skips shadow, returns `SilentDrop`; reconcile flags only unequal lengths with audit + SEV-2. | Length check; no external sink, schedule, or alert. |
| Webhook signature `lib.rs:574–579` | False `signed_ok` appends invalid-signature audit and returns `SignatureMismatch`; no alert. | Boolean, not cryptographic verification. |
| Webhook time arithmetic `lib.rs:580–585` | For representable subtraction/abs, drift `>300` appends replay audit + SEV-2 and returns `OutsideReplayWindow`; exactly 300 accepts. | `i64` subtraction/abs can overflow; checked panic/wrap behavior UNKNOWN. |
| D1 saturation `lib.rs:221–231` ([API-012](#api-012), [INV-002](#inv-002)) | `in_use >= capacity` returns `Pool503{retry_after_secs:2}` and appends exhaustion audit + SEV-2. | Local vectors; no connection, HTTP, or alert delivery. |
| JWKS refetch mismatch `lib.rs:647–659` ([API-036](#api-036), [INV-007](#inv-007)) | Cache miss copies upstream, appends rotation audit/INFO, then returns `Unverified` if key still differs. | String equality; no token, crypto, network, or event delivery. |
| Multipart after abort `lib.rs:731–747` ([API-041](#api-041), [API-042](#api-042), [INV-008](#inv-008)) | `complete()` returns `Err("multipart not open")`; `gc()` returns staged count and clears queue only in Aborted state. | Local queue; no remote cleanup. |
| RLS `lib.rs:385–404` | Missing/mismatched tenant rejects with audit/SEV-1; match inserts. | No SQL/database RLS. |
| BYOK `lib.rs:497–515` | Falls back to first available BTreeMap provider; none returns unavailable with audit/SEV-1. | No key bytes, provider, crypto or plaintext path. |

Observability is local vector mutation, not delivered events or operator visibility.

<a id="r08"></a>
## R08 — Evidence and unknowns

Evidence `chaos-campaign-static-cb94e251c` pins manifests/library/11 sources at `cb94e251c0f17382565bf863f517945cbb2a84d6`. Immutable source anchor `1177dad2ca2a9f21c29b5a118aa7944b77147798` has the identical package tree and source blobs; cb94 is the later capture commit. No execution/external activity; static.

Unknown: target eligibility/selection, compilation/invocation, graph/consumers, compatibility, overflow profile, external activity, deployment, owner; static absence is not proof.

[Back to identity](#r01)
