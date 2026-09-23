---
schema: corelink-ownership/1.1
document: reference
package: corelink-stripe-real
manifest: crates/corelink-stripe-real/Cargo.toml
source_commit: 16d9f0303d849a1ab3df14688bd2c7cdbfee8140
profile: S
state: author_validated
evidence_set: w011-stripe-real-source-static-20260920
---

# corelink-stripe-real — ownership reference

SOURCE-only reference for local declarations, target gates, ports, and fakes.
It does not claim credential use, network activity, external effects,
configuration, deployment, or runtime reachability.

[Identity](#r01) · [Targets](#r02) · [Surface](#r03) · [Interfaces](#r04) · [Axioms](#r05) · [Local state](#r06) · [Fakes](#r07) · [Unknowns](#r08)

<a id="r01"></a>
## R01 — Package identity invariant

| Atomic SOURCE predicate | Static falsifier / evidence |
|---|---|
| The manifest package is `corelink-stripe-real`; the crate root declares `clock`, `dlq`, `error`, `portal`, `retry`, `webhook`, and `webhook_dispatch`, and conditionally declares `client`. | Renaming the package or changing a stated module declaration in `Cargo.toml` or `src/lib.rs` falsifies this inventory. |

<a id="r02"></a>
## R02 — Target-gate invariant

| Atomic SOURCE predicate | Static falsifier / evidence |
|---|---|
| `reqwest` is declared only under `cfg(not(target_arch = "wasm32"))`; `js-sys` is declared only under `cfg(target_arch = "wasm32")`; `client` and its re-exports use the native predicate, while the clock re-export selects `SystemClock` or `WasmWorkerClock` by opposing predicates. | Changing either manifest target block or a named crate-root `cfg` falsifies the split. This does not prove selected target, build, linking, or execution. |

<a id="r03"></a>
## R03 — Root surface invariant

| Atomic SOURCE predicate | Static falsifier / evidence |
|---|---|
| `src/lib.rs` re-exports clocks, DLQ types/store, error types, portal types/ports/fakes, retry types, the webhook verifier/tolerance, and dispatcher port/recording types; native-only client symbols are separately gated. | Changing a named `pub use` or its target predicate in `src/lib.rs` falsifies this public inventory. It does not prove any caller selects it. |

<a id="r04"></a>
## R04 — Local-interface invariant

| Atomic SOURCE predicate | Static falsifier / evidence |
|---|---|
| `WebhookDlqStore`, `PortalAuditSink`, and `BillingPortalSessionCreator` are local traits; dispatcher trait/identity types are re-exported from `corelink-billing-stripe-traits`; `Clock` is locally defined. | Editing the declared trait signatures, the `pub use` list in `webhook_dispatch.rs`, or the `Clock` declaration falsifies the source contract. A declared trait or dependency does not prove a selected implementation or an external effect. |

<a id="r05"></a>
## R05 — Five source axioms

| ID | Falsifiable SOURCE axiom | Static falsifier / limit |
|---|---|---|
| AX-SR-01 | Native-only client source and wasm-specific clock source are separated by the target predicates recorded in R02. | Change a target block or crate-root `cfg`; no target selection or build claim follows. |
| AX-SR-02 | `RetryPolicy::next_sleep_ms` returns `None` when `attempt >= max_retries`, otherwise prefers a supplied retry-after value or a capped shifted base; `is_retryable_status` accepts only 429 or values at least 500. | Change the guard, override branch, cap expression, or status predicate in `src/retry.rs`; no observed response or wait follows. |
| AX-SR-03 | The verifier parses `t=` plus one or more `v1=` values, checks symmetric tolerance before comparing HMAC bytes, and accepts when a same-length candidate is `ct_eq` to the computed bytes. | Change a parse branch, skew comparison, HMAC input update, candidate loop, or `ct_eq` branch in `src/webhook.rs`; no request, secret, clock, or endpoint claim follows. |
| AX-SR-04 | `WebhookDispatcher::process` calls signature verification before envelope parsing and asks its injected idempotency store before the later materializer branch. | Reorder/remove the named calls in `src/webhook_dispatch.rs`; this is source ordering only, not dispatch or persistence evidence. |
| AX-SR-05 | `InMemoryPortalSessionCreator` validates local inputs, calls its injected audit sink before appending an issued URL and returning it, and maps an audit error to `AuditFailed`. | Reorder/remove the audit call, vector append, return, or error mapping in `src/portal.rs`; no durable audit or external session result follows. |

<a id="r06"></a>
## R06 — Local state invariant

| Atomic SOURCE predicate | Static falsifier / evidence |
|---|---|
| `InMemoryIdempotencyStore` stores token-keyed rows and returns `FirstSight` for an absent token or `AlreadyProcessed` for a present one; `InMemoryWebhookDlqStore` stores rows in a mutex-protected map and maps a duplicate event ID to `Updated`. | Change the map key, conditional branch, or outcome arm in `src/{webhook_dispatch,dlq}.rs` falsifies the local-fake relation. It does not establish persistence or replay behavior outside the fake. |

<a id="r07"></a>
## R07 — Clock and fake invariant

| Atomic SOURCE predicate | Static falsifier / evidence |
|---|---|
| `default_clock` constructs `SystemClock` on native and `WasmWorkerClock` on wasm32; `InMemoryFakeClock`, portal creator/audit sink, dispatcher recorders, and DLQ store retain local injectable or mutex-protected state. | Changing a target branch, named fake, injected port, or backing collection in `src/{clock,portal,webhook_dispatch,dlq}.rs` falsifies the inventory. It does not demonstrate a real clock, audit record, consumer, or test result. |

<a id="r08"></a>
## R08 — Evidence limit and five unknowns

Evidence mode is SOURCE: the package manifest and named local Rust source at
the pinned commit. DOCUMENTARY evidence may establish only structural-document
checks. The verified canonical OKF route is [SRE operations hub](../../../knowledge/ops/sre-operations-hub.md), used for routing only and neither copied nor revalidated here.

1. Selected targets, feature resolution, compilation, linking, and tests are UNKNOWN.
2. Caller graph, compatibility, route composition, and reachability are UNKNOWN.
3. Credentials, configuration, request transmission, external behavior, and responses are UNKNOWN.
4. Clock quality, persistence, audit durability, retry timing, and state effects in an environment are UNKNOWN.
5. Deployment, bindings, production data, monitoring, and independent-review outcome are UNKNOWN.

Claiming any unknown without separately selected evidence falsifies this
record's boundary.

[Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Ownership guide](../../../../.claude/skills/own-corelink-stripe-real/SKILL.md#s01)
