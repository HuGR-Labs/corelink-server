---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-billing-stripe
manifest: crates/corelink-billing-stripe/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-billing-stripe-structural-normalization-20260921
---

# corelink-billing-stripe — maintenance

[M01](#m01) · [M02](#m02) · [M03](#m03) · [M04](#m04) · [M05](#m05) · [M06](#m06)

This manual separates safe static examination from implementation and runtime
operations. This ownership pass executed no Cargo build/test, network action,
deployment, secret read, live Stripe action, or production audit operation.

<a id="m01"></a>
## M01 — Prepare a bounded review

**Mode:** READ_ONLY. **Prerequisites:** isolated checkout, intended baseline,
and explicit target path. **Predicate:** package identity is
`corelink-billing-stripe` and source boundary is the crate’s nine modules.
Read the manifest and `src/lib.rs`, then the affected module. **Stop:** baseline,
package, or source boundary differs. **Recovery:** make no edit; request a
corrected target. **Evidence:** SHA, paths inspected, and `git status --short`.

<a id="m02"></a>
## M02 — Select the procedure

| Situation | Procedure | Mode | Stop condition |
|---|---|---|---|
| Key/canonicalization change | M03-A | SOURCE review | aggregate contract or compatibility unknown |
| Signature/parser/skew change | M03-B | SOURCE review | request needs external delivery/clock/secret evidence |
| Adapter/audit/ledger/log change | M03-C | SOURCE review | request asserts durable/remote atomicity |
| Ownership artifact change | M04 | DOCUMENTARY | changed path is outside the four owned artifacts |
| Real Stripe, HTTP, route, secret, audit, or deployment work | M06 | ESCALATION | do not operate from this guide |

<a id="m03"></a>
## M03 — Source procedures

### M03-A — Canonical key diagnosis

**Mode:** SOURCE review. **Prerequisites:** affected aggregate/type and known
compatibility decision. **Predicate:** implementation remains
`serde_jcs::to_vec(AggregatedCounter)` followed by BLAKE3 bytes into the
32-byte key, with 64-character hex rendering. Inspect `src/idempotency.rs` and
`src/event.rs`. **Stop:** consumer compatibility, aggregate semantics, or a
persisted key format is not bounded. **Recovery:** retain the prior interface
and escalate to aggregator/consumer owners. **Evidence:** code location,
before/after API shape, and explicit unknowns.

### M03-B — Signature primitive diagnosis

**Mode:** SOURCE review. **Prerequisites:** requested parser/verifier behavior
is stated in code terms. **Predicate:** parser keeps valid `t=` and all valid
32-byte `v1=` candidates; HMAC input is decimal timestamp + dot + raw payload;
candidate comparison uses `ConstantTimeEq::ct_eq`; skew rejects only above
`300_000` ms. Inspect `src/signature.rs`. **Stop:** external Stripe behavior,
real clock, secret, raw HTTP body, or endpoint timing is needed. **Recovery:**
leave source unchanged and obtain direct integration evidence. **Evidence:**
exact function/branch and stated input values (never a secret).

### M03-C — Local orchestration diagnosis

**Mode:** SOURCE review. **Prerequisites:** identify adapter or webhook path.
**Predicate:** audit calls precede the relevant local ledger/log mutation and
errors propagate through `StripeError`; the in-memory ledger/log keying remains
visible. Inspect `src/{adapter,webhook,audit,ledger,webhook_log,error}.rs`.
**Stop:** proposed conclusion requires a remote transaction, durable backend,
or live outcome. **Recovery:** split the integration work and escalate.
**Evidence:** branch ordering and traits involved, not comments describing a
provider.

<a id="m04"></a>
## M04 — Documentary validation matrix

| Check | Expected predicate | Result in this ownership pass |
|---|---|---|
| Skill structural check (S) | target exists with S01–S07 and decision form | run after authoring |
| Reference structural check (S) | target exists with R01–R08 | run after authoring |
| Blast structural check (S) | target exists with B01–B06 | run after authoring |
| Maintenance structural check (S) | target exists with M01–M06 | run after authoring |
| `git diff --check` | no whitespace error | run after authoring |

These are documentation checks only. They neither compile nor execute the
crate and must not be reported as semantic approval, runtime evidence, or
cold review.

<a id="m05"></a>
## M05 — Recovery and compatibility

| Surface | Safe recovery | Do not infer |
|---|---|---|
| Ownership documents | amend only the owned artifact, rerun M04, request fresh review | approval from a checker pass |
| Public trait/type | coordinate static consumers and preserve/migrate API intentionally | complete reverse graph or deployed compatibility |
| In-memory fake | restore local invariant with bounded source review | rollback of remote Stripe/D1/audit state |
| Signature/key format | stop for compatibility and security review | secret rotation, clock health, or external acceptance |

<a id="m06"></a>
## M06 — Escalation and completion record

Escalate real Stripe API activity, route mounting, raw HTTP body capture,
secret provisioning/rotation, clock source, durable ledger/log/audit behavior,
alerts, retry, deployment, and production reconciliation to their responsible
integration/runtime operators. Provide a redacted request, baseline, source
paths, exact uncertainty, and required direct evidence. On completion report:
baseline SHA, exact changed paths, documentary check results, static dependency
edges, and unknowns. A separate cold reviewer owns approval.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) ·
[Ownership guide](../../../../.claude/skills/own-corelink-billing-stripe/SKILL.md#s01)
