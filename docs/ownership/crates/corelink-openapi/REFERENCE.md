---
schema: corelink-ownership/1.1
document: reference
package: corelink-openapi
manifest: tools/openapi/Cargo.toml
source_commit: 398e586ccef712477f2a4ce51e026443b67e5747
profile: S
state: candidate
evidence_set: corelink-openapi-source-static-20260921
---

# corelink-openapi — ownership reference

S-profile SOURCE reference at the pinned revision. A predicate below is falsifiable from checked-in text; it does not establish generation, publication, dependency resolution, consumer reachability, parsing, route reachability, or runtime.

[Identity](#r01) · [Inputs](#r02) · [Versions](#r03) · [Paths](#r04) · [Axioms](#r05) · [Parser/tests](#r06) · [Dependencies](#r07) · [Limits](#r08)

<a id="r01"></a>
## R01 — Package identity invariant

**Predicate:** `tools/openapi/Cargo.toml` names the package `corelink-openapi`. This record separately inspects source text at `tools/openapi/src/lib.rs`; the manifest alone does not establish that file as the crate root or resolve an implementation target. **Falsifier:** alter/remove that package name; a target/root resolution is UNKNOWN. **SOURCE:** `tools/openapi/Cargo.toml:1-18`; inspected text `tools/openapi/src/lib.rs:1-289`. **Unknown:** selected workspace member, crate-root/target resolution, package build, artifact, or publication.

<a id="r02"></a>
## R02 — Embedded-input declaration invariant

**Predicate:** `SPEC_YAML` and `SPEC_JSON` are public `&str` constants declared with `include_str!` paths `../../../openapi/corelink-v1.yaml` and `../../../openapi/corelink-v1.json`. **Falsifier:** alter a constant name, type, macro, or path. **SOURCE:** `tools/openapi/src/lib.rs:44-49`. **Unknown:** either input’s generation, contents, synchronization, materialization, publication, or use.

<a id="r03"></a>
## R03 — Version declaration invariant

**Predicate:** `SPEC_VERSION` is exactly `"v1"`; `PACKAGE_VERSION` is declared by `env!("CARGO_PKG_VERSION")`. **Falsifier:** alter either literal or macro. **SOURCE:** `tools/openapi/src/lib.rs:51-63`. **Unknown:** environment expansion, package version value, OpenAPI document value, compatibility, or release behavior.

<a id="r04"></a>
## R04 — Path-surface declaration invariant

**Predicate:** public module `paths` declares and `ALL` lists these 27 name/value pairs: `SIGNUP=/v1/signup`, `DPA_ACCEPT=/v1/onboarding/dpa-accept`, `TIER_SELECT=/v1/onboarding/tier-select`, `PATS=/v1/pats`, `PATS_ITEM=/v1/customer/keys/{pat_id}/revoke`, `ACCOUNT_DELETE=/v1/customer/account/delete`, `ACCOUNT_EXPORT=/v1/customer/account/export`, `STRIPE_WEBHOOK=/v1/billing/stripe-webhook`, `DSR_SUBMIT=/v1/privacy/dsr/{action}`, `DSR_STATUS=/v1/privacy/dsr/{request_id}/status`, `USERS_ME=/v1/users/me`, `HEALTH=/api/health`, `SIGNUP_PILOT=/v1/signup/pilot/{token}`, `ADMIN_READ=/v1/admin/read/{resource}`, `ADMIN_MUTATE=/v1/admin/mutate`, `ADMIN_PILOTS=/v1/admin/pilots`, `ADMIN_PILOTS_GRANT_TIER=/v1/admin/pilots/{tenant_id}/grant-tier`, `ADMIN_PILOTS_CHECKIN=/v1/admin/pilots/{tenant_id}/checkin`, `AUDIT_EXPORT=/v1/audit/{tenant}/export`, `AUDIT_ANALYTICS_EVENT_COUNT=/v1/audit/analytics/event-count`, `AUDIT_ANALYTICS_TIMELINE=/v1/audit/analytics/timeline`, `CAS_READ_TENANT=/v1/cas/{tenant}/{hash}`, `CAS_READ_DIGEST=/v1/cas/{digest}`, `CAS_BATCH=/v1/cas/{tenant}/batch`, `CAS_BATCH_READ=/v1/cas/{tenant}/batch-read`, `CAS_BATCH_EXISTS=/v1/cas/{tenant}/batch-exists`, and `AC_LOOKUP=/v1/ac/{tenant}/{action_digest}`. **Falsifier:** change a listed name/value, module visibility, or `ALL` membership. **SOURCE:** `tools/openapi/src/lib.rs:68-88,145-185,187-218`. **Unknown:** path presence in an external document, route registration, serving, client use, and reachability. These are declared canonical path strings, not proof of implemented endpoints.

<a id="r05"></a>
## R05 — Five source axioms

| Axiom | Falsifiable predicate | SOURCE / falsifier |
|---|---|---|
| AX-001 | The inspected local text declares `#![forbid(unsafe_code)]`. | local inspected text:42; remove/alter `#![forbid(unsafe_code)]`. |
| AX-002 | `SPEC_YAML` uses the stated YAML `include_str!` path. | `src/lib.rs:44-45`; alter macro/path. |
| AX-003 | `SPEC_JSON` uses the stated JSON `include_str!` path. | `src/lib.rs:47-49`; alter macro/path. |
| AX-004 | `SPEC_VERSION` is literal `"v1"`. | `src/lib.rs:51-55`; alter literal. |
| AX-005 | `paths::ALL` is a public slice of named path constants. | `src/lib.rs:187-218`; alter visibility, slice, or membership. |

None of AX-001–AX-005 proves compilation, file inclusion, document validity, safety beyond the local declaration, or external behavior.

<a id="r06"></a>
## R06 — Parser and test-text invariant

**Predicate:** `parse_json` returns `serde_json::from_str(SPEC_JSON)`; the `cfg(test)` module declares four named tests, including JSON/object, path-membership, version-match, and major/version-distinction assertions. **Falsifier:** alter the parser call, test gate, function names, or assertions. **SOURCE:** `tools/openapi/src/lib.rs:221-289`. **Unknown:** parsing result, test execution, test discovery, CI, and any drift conclusion.

<a id="r07"></a>
## R07 — Dependency declaration invariant

**Predicate:** the manifest declares workspace `serde` and `serde_json` dependencies, plus workspace `serde_json` as a dev-dependency. **Falsifier:** alter/remove a named declaration. **SOURCE:** `tools/openapi/Cargo.toml:13-18`. **Unknown:** resolved versions, enabled features, availability, compilation, or dependency behavior.

<a id="r08"></a>
## R08 — Evidence boundary and OKF route

Evidence is limited to the manifest and inspected `tools/openapi/src/lib.rs` text at the pinned commit. The designated canonical OKF route is [Worker edge plane](../../../knowledge/planes/worker-edge.md); it is a reference destination only and is not copied, redefined, or revalidated here. Unknown: input-document generation and synchronization, publication, artifact creation, resolved consumers, reverse graph, parsing, tests, CI, route registration/serving, network, deployment, runtime, independent review, and crate-root/target resolution.

[Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Back to identity](#r01)
