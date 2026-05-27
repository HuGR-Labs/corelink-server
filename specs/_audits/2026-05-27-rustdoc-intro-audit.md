---
id: "AUDIT-2026-05-27-RUSTDOC-INTRO-AUDIT"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "rustdoc", "wave-33", "wave-34", "wave-35", "wave-36", "umbrella", "docs"]
references:
  - "specs/_audits/2026-05-22-wave33-code-reorg-spec.md"
  - "specs/_audits/2026-05-26-wave-33-34-closure-followups.md"
  - "specs/_audits/2026-05-26-w35-p2-cas-absorption.md"
  - "specs/_audits/2026-05-26-w35-p2-telemetry-absorption.md"
  - "specs/_audits/2026-05-26-w35-p2-adapter-host-absorption.md"
  - "specs/_audits/2026-05-26-w35-p2-replication-absorption.md"
  - "specs/_audits/2026-05-26-w35-p2-billing-absorption.md"
  - "specs/_audits/2026-05-26-w35-p2-privacy-absorption.md"
  - "specs/_audits/2026-05-26-w35-p2-ops-absorption.md"
  - "specs/_audits/2026-05-26-w35-p2-ac-absorption.md"
  - "specs/_audits/2026-05-26-w35-p2-byok-absorption.md"
  - "specs/_audits/2026-05-27-w36-trigger-a-seal.md"
---

# Rustdoc Intro Audit — Wave 33-36 Umbrella Crates

## §1. Scope

Verify SOTA crate-level `//!` rustdoc intros across all 11 Wave 33-36
umbrella crates. Each intro must document:

1. Purpose / role of the umbrella.
2. What was absorbed and from where.
3. Canonical consumer path.
4. Key invariants preserved (charter IDs).
5. Wave / sprint provenance (with SEAL audit refs).

## §2. Crate-by-crate verdict

| # | Crate | Pre-audit state | W35-P2 absorbed list | Fix applied |
|---|---|---|---|---|
| 1 | `corelink-cas` | `//!` block with Wave-33 Stage-1 Stream A prose, charter constraints, 10 Option-A aggregator tenants — but **no W35-P2 section**. | 6 (chunker, dedup, edge, lru-tracker, manifest, multipart-schema) | **APPENDED** `## Wave 35 Phase 2 absorption` section listing the 6 physically absorbed crates + SEAL audit ref. Existing prose untouched. |
| 2 | `corelink-telemetry` | `//!` block with Wave-33 Stage-0 sub-step 3 prose, 7 Option-A tenants — but **no W35-P2 section**. | 5 (canary, lighthouse-tracker, logpush, otel-export, synthetic-pager) | **APPENDED** `## Wave 35 Phase 2 absorption` section listing the 5 physically absorbed crates + SEAL audit ref. Existing prose untouched. |
| 3 | `corelink-adapter-host` | `//!` block with Wave-35 bridge-crate prose, module map for 5 bridges — but **no W35-P2 absorbed-list section**. | 5 (adapter-{brew,cargo,npm,oci,pip}) | **APPENDED** `# Wave 35 Phase 2 absorption` section listing the 5 physically absorbed adapters with LOC + test counts + SEAL audit ref. Existing prose untouched. |
| 4 | `corelink-replication` | `//!` block already mentions Wave 35 Phase 2 inline at the `rollout_controller` bullet with SEAL audit ref. | 1 (rollout-controller) | **NONE** — already SOTA. |
| 5 | `corelink-auth` | `//!` block with Wave-33 Stage-1 Stream B prose, 6 Option-A tenants — but **no W35-P2 section**. | 2 (webauthn, auth-schema) | **APPENDED** `## Wave 35 Phase 2 absorption` section listing the 2 physically absorbed crates + closure-followups doc ref (no dedicated `2026-05-26-w35-p2-auth-absorption.md` was filed; the absorption was sealed via commits `9fb5a4fd` + `44988a11` per the closure-followups doc). Existing prose untouched. |
| 6 | `corelink-billing` | `//!` block with Wave-33 Stage-1 Stream B prose, 14 Option-A tenants, charter constraints — but **no W35-P2 section**. | 5 (abuse, billing-replay, quota, quota-cas, quota-fsm) | **APPENDED** `## Wave 35 Phase 2 absorption` section listing the 5 physically absorbed crates + SEAL audit ref. Existing prose untouched. |
| 7 | `corelink-privacy` | `//!` block with Wave-33 Stage-1 Stream B prose, 11 Option-A tenants, charter constraints — but **no W35-P2 section**. | 6 (dpa-versioning, breach, consent, notice, residency, sub-processor) | **APPENDED** `## Wave 35 Phase 2 absorption` section listing the 6 physically absorbed crates + SEAL audit ref. Existing prose untouched. |
| 8 | `corelink-ops` | `//!` block with Wave-33 Stage-1 Stream C prose, 28 Option-A tenants, charter constraints — but **no W35-P2 section** (LARGEST absorption batch). | 15 (admin-api, admin-dry-run, backup-verify, config-api, customer-alerts, d1-migrations, deploy-verifier, dr-drill, drata-sync, oncall, rotation-worker, supply-chain-policy, supply-verify, survey, tenant-offboarding) | **APPENDED** `## Wave 35 Phase 2 absorption (LARGEST Wave-35 batch)` section listing the 15 physically absorbed crates + SEAL audit ref + post-absorption test count (405 green). Existing prose untouched. |
| 9 | `corelink-ac` | `//!` block already mentions Wave 35 Phase 2 explicitly with 2 absorbed crates (ac-core, ac-schema), charter constraints, and closure-followups ref. | 2 (ac-core, ac-schema) | **NONE** — already SOTA. |
| 10 | `corelink-byok` | `//!` block with Wave-33 Stage-1 Stream B sub-step B.2b microkernel prose, cargo feature reference, charter compliance — but **W35-P2 content lives in regular `//` code comments**, not in the rustdoc section. | 6 (byok-core, byok-revocation, byok-aws, byok-gcp, byok-azure, byok-vault) | **APPENDED** `## Wave 35 Phase 2 absorption` section to the rustdoc block listing the 6 physically absorbed crates + SEAL audit ref. Existing rustdoc + inline `//` comments untouched. |
| 11 | `corelink-billing-stripe-traits` | `//!` block with Wave-36 Trigger A "Why this crate exists" prose, layering invariants, symbol catalogue, full cycle-break rationale. | N/A (NEW leaf — does not absorb) | **NONE** — already SOTA. |

## §3. Inline fixes summary

- **11 crates audited.**
- **8 crates required appended W35-P2 absorption sections** (cas, telemetry, adapter-host, auth, billing, privacy, ops, byok).
- **3 crates already SOTA** (replication, ac, billing-stripe-traits).
- **Existing rustdoc prose was NOT rewritten** in any crate — only `## Wave 35 Phase 2 absorption` sections were APPENDED to the end of each crate's existing rustdoc block immediately before the `#![forbid(unsafe_code)]` attribute.
- **W36 zones were NOT touched** (clerk*, stripe.rs, statuspage.rs, slack.rs).

## §4. Acceptance

```
cargo doc --workspace --no-deps --document-private-items
# Finished `dev` profile [unoptimized + debuginfo] target(s)
# Generated /target/doc/<crate>/index.html for all umbrellas
# Zero broken intra-doc-link errors

cargo build --workspace
# Finished `dev` profile [unoptimized + debuginfo] target(s)
```

Acceptance results captured at commit-time and recorded in the
commit message body.

## §5. Charter compliance

- No `--no-verify` used.
- No existing rustdoc prose was rewritten.
- No W36 Stage 2.C zones (`statuspage.rs`, `slack.rs`,
  `clerk*`, `stripe.rs`) were touched.
- All added `//!` content references the matching W35-P2 SEAL audit
  (or, for `corelink-auth` where no dedicated SEAL doc was filed, the
  closure-followups doc §2 + the two sealing commit hashes).
