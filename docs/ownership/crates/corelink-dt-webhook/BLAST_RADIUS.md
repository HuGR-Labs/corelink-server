---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-dt-webhook
manifest: crates/corelink-dt-webhook/Cargo.toml
source_commit: 398e586ccef712477f2a4ce51e026443b67e5747
profile: S
state: draft
evidence_set: corelink-dt-webhook-source-static-20260921
---

# corelink-dt-webhook — blast radius

Atomic static relations only. An arrow records source declarations, values, or
calls; it does not prove compilation, invocation, webhook receipt, endpoint
operation, provider traffic/behavior/delivery, persistence, metrics emission,
deployment, or runtime reachability.

[Root](#b01) · [Trait/types](#b02) · [HMAC](#b03) · [Severity](#b04) · [State](#b05) · [Manifest](#b06).

[REL-001](#rel-001) · [REL-002](#rel-002) · [REL-003](#rel-003) · [REL-004](#rel-004) · [REL-005](#rel-005) · [REL-006](#rel-006).

<a id="b01"></a>
## B01 — Root export relation

<a id="rel-001"></a>
### REL-001 — Root → module and re-export surface

**Direction:** `src/lib.rs` → six public modules and named handler/type exports.
**Flow:** module declarations and `pub use` make local paths source-visible.
**Impact:** an export, module, type, or trait-signature edit can require static
consumer compatibility review. **Limit:** no caller, import, compilation, or
invocation is established. **Evidence:** `src/lib.rs`.
[Root](#b01) [Relation index](#b03)

<a id="b02"></a>
## B02 — Trait and domain-type relation

<a id="rel-002"></a>
### REL-002 — Trait input → result/error types

**Direction:** `DtWebhookHandler` methods → `DtWebhookEvent`/`DtProjectUuid`/
`SyntheticCve` → `AlertDelivered`/`MockInjected`/`DtWebhookError`.
**Flow:** the trait signatures name these public values at its local interface.
**Impact:** a signature, field, enum, or error edit changes implementor/caller
source compatibility. **Limit:** no concrete route, implementor selection, or
method call is known. **Evidence:** `src/lib.rs`; `src/types.rs`.
[Trait/types](#b02) [Relation index](#b03)

<a id="b03"></a>
## B03 — HMAC byte relation

<a id="rel-003"></a>
### REL-003 — Supplied bytes → local verification result

**Direction:** secret/body/header string → `verify_signature` → unit or
`HmacInvalid` result. **Flow:** prefix stripping, hex decoding, HMAC-SHA256,
and `ct_eq` precede the result branch. **Impact:** changing any stage or error
mapping changes local validation semantics. **Limit:** no request/header,
secret provenance, authentication event, endpoint, or timing observation is
established. **Evidence:** `src/hmac.rs`.
[HMAC](#b03)

<a id="b04"></a>
## B04 — Score-to-channel-vector relation

<a id="rel-004"></a>
### REL-004 — CVSS value → severity → channel vector

**Direction:** supplied `f64` → `classify_cvss` → `routing_channels`.
**Flow:** clamp/threshold branches select a severity, then a named vector.
**Impact:** a threshold, enum arm, or vector edit alters source-level output
for a supplied value. **Limit:** no score provenance, alert policy execution,
channel configuration, provider request, or delivery is established.
**Evidence:** `src/severity.rs`; `src/types.rs`.
[Severity](#b04) [Relation index](#b03)

<a id="b05"></a>
## B05 — In-memory state relation

<a id="rel-005"></a>
### REL-005 — Handler branch → local vectors, DLQ, and snapshot

**Direction:** `handle_webhook` local branches → delivery vector/DLQ/
`MetricsSnapshot`. **Flow:** the handler's source loop appends a returned
channel to a local vector on its `Ok(())` branch; its error branch constructs a
`DlqEntry` and calls local `push`. **Impact:** ordering, cap, or field edits
change fixture semantics. **Limit:** neither branch proves alerting, queue
durability, metric collection, retry, or cross-system ordering. **Evidence:**
`src/handler.rs`; `src/dlq.rs`; `src/metrics.rs`.
[State](#b05) [Relation index](#b03)

<a id="b06"></a>
## B06 — Manifest target relation

<a id="rel-006"></a>
### REL-006 — Manifest declarations → source/test/example paths

**Direction:** package manifest → direct dependencies, two `[[test]]`, and
three `[[example]]` paths. **Flow:** `Cargo.toml` associates each declared
target name with a checked-in relative path. **Impact:** dependency or target
path/name edits can change static build-surface selection. **Limit:** resolved
features, compilation, test/example execution, and operational behavior are
unknown. **Evidence:** `crates/corelink-dt-webhook/Cargo.toml`.
[Manifest](#b06)

### Known inverse source edges

`tools/dt-cli/Cargo.toml` and `tools/dt-reconcile/Cargo.toml` each declare a direct package dependency; their `src/main.rs` files import `corelink_dt_webhook` APIs and construct/use `InMemoryDtWebhookHandler` (the reconcile source also uses `InMemoryDlq`). `crates/corelink-ops/Cargo.toml` declares the dependency and `crates/corelink-ops/src/dt.rs` re-exports the public API under `corelink_ops::dt::webhook`. These edges establish checked-in dependency/import/re-export text only; target selection, compilation, invocation, endpoint registration, and runtime reachability remain unknown.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Ownership guide](../../../../.claude/skills/own-corelink-dt-webhook/SKILL.md#s01). [Relation index](#b03)
