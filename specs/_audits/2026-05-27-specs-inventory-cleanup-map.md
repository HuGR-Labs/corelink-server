---
id: "AUDIT-2026-05-27-SPECS-INVENTORY-CLEANUP-MAP"
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
tags: ["audit", "inventory", "cleanup", "specs-bloat", "seal"]
---

# Specs corpus inventory + cleanup map

## §1 Scope

Read-only categorization of all 890 `.md` files under `specs/` (16.86 MB total) into ten mutually-exclusive buckets so a follow-up cleanup wave can move/mark/merge files with confidence. **No file under `specs/` was modified by this audit.** Output is this map.

Method: enumerate via `find specs/ -name '*.md' | sort` (890 hits, matches `wc -l` baseline), then for each file (i) read YAML frontmatter, (ii) classify by path + frontmatter (`doc_status`, `audit_status`, `superseded_by`), (iii) for live (CHARTER/REFERENCE) files do a body-grep override to `DEFASADO-CODE-DRIFT` when an absorbed-crate path appears without a historical marker. Categorization script: `/tmp/classify_specs.py` (reproducible). Duplicate-basename detection by `basename | sort | uniq -d` then content diff.

Sprint sealed status is derived from `specs/04_sprints/S<NN>/_spec_contract.md` frontmatter (`doc_status` ∈ {FROZEN, SEALED} OR `audit_status` == AUDITED). Active sprints (per their contracts): S00, S02, S08, S10, S11, S13, S14. All others are treated as sealed.

## §2 Distribution

| Category | Count | Size (KB) | % of corpus |
|---|---:|---:|---:|
| CHARTER | 78 | 1133.3 | 8.8% |
| SEALED-HISTORICAL | 365 | 6116.2 | 41.0% |
| SPRINT-WI-SEALED | 93 | 3343.8 | 10.4% |
| SPRINT-CONTRACT-SEALED | 56 | 1343.3 | 6.3% |
| SUPERSEDED | 0 | 0.0 | 0.0% |
| DEFASADO-CODE-DRIFT | 27 | 712.5 | 3.0% |
| DEFASADO-NOT-LANDED | 8 | 133.8 | 0.9% |
| DUPLICATE | 4 | 36.9 | 0.4% |
| SCAFFOLD | 15 | 221.4 | 1.7% |
| REFERENCE | 244 | 4224.9 | 27.4% |
| **TOTAL** | **890** | **17266.2** | **100.0%** |

## §3 Cleanup recommendations (3 waves)

### §3.1 Wave A — Safe archival (MECHANICAL, NO CONTENT CHANGE)

Move sealed/historical artifacts into archive subdirs so the top-level corpus reflects what is currently live. Bytes unchanged; only `git mv` operations.

- **SEALED-HISTORICAL (365 files, 6116 KB)** → `specs/_audits/sealed/` (`_pentest/` and `_postmortems/` can move under `specs/_audits/sealed/pentest/` and `specs/_audits/sealed/postmortems/` respectively, or stay; the goal is signalling immutability via subdir naming).
- **SPRINT-WI-SEALED (93 files, 3344 KB)** → `specs/04_sprints/_sealed/S<NN>/work_items/`. Sprints affected: S01, S03, S04, S05, S06, S07, S09, S12, S15, S16, S17, S18, S19, S20.
- **SPRINT-CONTRACT-SEALED (56 files, 1343 KB)** → `specs/04_sprints/_sealed/S<NN>/` (sprint.md, _spec_contract.md, PRR-S*.md, review/signoff bundles for the same 14 sealed sprints).
- **SCAFFOLD (15 files, 221 KB)** → confirm under `specs/_templates/` or per-domain `templates/` subdirs (most already are; only `specs/_audits/lia-template.md` and `specs/_pentest/findings-template.md` are loose at parent-dir level — consider `_templates/` or leave).

Expected impact: ~514 files (58% of corpus by count, ~63% by bytes) move out of top-level visibility. Top-level reduces from 890 → ~376 visible. Path-rewrite script must be run on:
- `scripts/validate_specs.py` allow-list,
- any `references:` frontmatter cross-link that hard-codes `specs/_audits/...` or `specs/04_sprints/S<NN>/...`,
- `CLAUDE.md` and root memory references.

### §3.2 Wave B — Defasado triage (SEMANTIC TOUCHES)

- **DEFASADO-CODE-DRIFT (27 files, 712 KB)** — these reference absorbed crate paths (`crates/corelink-<chunker|dedup|edge|adapter-*|byok-*|quota|webauthn|...>/`) without any historical marker. Each gets a top-of-body callout:
  > **Post Wave 35 Phase 2 update 2026-05-27**: `crates/corelink-<X>` was absorbed into `crates/corelink-<umbrella>` via Wave 35 P2 (`specs/_audits/2026-05-26-w35-p2-<umbrella>-absorption.md`). Canonical path is now `corelink_<umbrella>::<mod>`. Historical references below preserved verbatim.

  Frontmatter version bump (e.g., `version: "1.2.0"` → `"1.3.0"`) and `updated: "2026-05-27"`. **Mechanical** if the umbrella mapping is provided as a lookup table — see Appendix §A.1 below for proposed mapping.

- **DEFASADO-NOT-LANDED (8 files, 134 KB)** — all under `specs/_proposals/`. Two options per file:
  1. Add frontmatter `doc_status: "DEFASADO"` (preserves history in place), OR
  2. Move to `specs/_proposals/_dead/` (preserves URL-stability less, but cleans index).

  Recommendation: option 1 (in-place mark). All 8 are pre-implementation drafts the actual code/ADRs already supersede (e.g., adapters/cargo.md vs the `corelink_adapter_host` absorption; framework-reviewer-roles vs ADR-0034b).

### §3.3 Wave C — Duplicate consolidation (REVIEW-REQUIRED)

- **DUPLICATE (4 files, 2 pairs, 37 KB)** — same basename, different content under different roots:
  1. `specs/_runbooks/RB-FM-SIGNUP-FAILED.md` (10 KB) vs `specs/05_quality/runbooks/RB-FM-SIGNUP-FAILED.md` (6.5 KB)
  2. `specs/05_runbooks/RB-BYOK-REVOKE.md` (6.7 KB) vs `specs/05_quality/runbooks/RB-BYOK-REVOKE.md` (14.6 KB)

  Larger file is likely the canonical one (more elaborated), but content diff is required. Not autonomous — picks must be human.

  Additionally, three `INDEX.md` files exist (`specs/_dashboards/`, `specs/05_quality/runbooks/`, `specs/_audits/stride-per-crate/`) and three `README.md` (`specs/tla/`, `specs/_proposals/adapters/`, `specs/_compliance/weekly-digests/`). These are **scope-local indexes**, not duplicates — no action.

## §4 File-level listing

Each entry: `path<TAB>size_bytes<TAB>reason`.



<details>
<summary><b>§4.1 CHARTER — 78 files, 1133.3 KB</b></summary>

```
specs/00_framework.md	182645	top-level framework charter
specs/03_architecture/adrs/ADR-0012-ff-hr-011-gc-reachability.md	3093	architecture charter doc
specs/03_architecture/adrs/ADR-0013-promote-remote-cache-canonical.md	3495	architecture charter doc
specs/03_architecture/adrs/ADR-0014-sbom-format-cyclonedx.md	3707	architecture charter doc
specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md	11161	architecture charter doc
specs/03_architecture/adrs/ADR-0016-ffi-wrappers-vs-native-http.md	8005	architecture charter doc
specs/03_architecture/adrs/ADR-0017-grafana-cloud-vs-self-hosted-observability.md	2570	architecture charter doc
specs/03_architecture/adrs/ADR-0018-key-overlap-per-asset.md	3794	architecture charter doc
specs/03_architecture/adrs/ADR-0019-ttl-ownership-s04-s07.md	9030	architecture charter doc
specs/03_architecture/adrs/ADR-0020-quota-ownership-s07-s08.md	4533	architecture charter doc
specs/03_architecture/adrs/ADR-0021-hkdf-vs-ed25519-ac-signing.md	7639	architecture charter doc
specs/03_architecture/adrs/ADR-0022-chunk-size-vs-part-size-decoupling.md	6498	architecture charter doc
specs/03_architecture/adrs/ADR-0023-constant-time-timing-padding.md	10675	architecture charter doc
specs/03_architecture/adrs/ADR-0028-missreason-uniform-404-freeze.md	9433	architecture charter doc
specs/03_architecture/adrs/ADR-0030-revocation-propagation.md	7458	architecture charter doc
specs/03_architecture/adrs/ADR-0031-neon-schema-pgcrypto.md	10837	architecture charter doc
specs/03_architecture/adrs/ADR-0032-webauthn-level3.md	13445	architecture charter doc
specs/03_architecture/adrs/ADR-0033-audit-events-cloudevents.md	15392	architecture charter doc
specs/03_architecture/adrs/ADR-0034-prr-staffing-waiver-solo-tier.md	5319	architecture charter doc
specs/03_architecture/adrs/ADR-0034b-framework-reviewer-dual-hat-fallback.md	21503	architecture charter doc
specs/03_architecture/adrs/ADR-0035-ac-handler-invariants.md	4328	architecture charter doc
specs/03_architecture/adrs/ADR-0036-d1-schema-migration-governance.md	4350	architecture charter doc
specs/03_architecture/adrs/ADR-0037-dependency-track-self-host.md	4352	architecture charter doc
specs/03_architecture/adrs/ADR-0037-merkle-action-protocol-result-hash.md	9946	architecture charter doc
specs/03_architecture/adrs/ADR-0039-chunker-public-api-stability.md	6895	architecture charter doc
specs/03_architecture/adrs/ADR-0040-multipart-d1-sharding.md	7987	architecture charter doc
specs/03_architecture/adrs/ADR-0042-gc-worker-scheduler.md	10541	architecture charter doc
specs/03_architecture/adrs/ADR-0043-hmac-tenant-prefix-algorithm.md	10751	architecture charter doc
specs/03_architecture/adrs/ADR-0044-deploy-gate-hard-cosign-keyless.md	8753	architecture charter doc
specs/03_architecture/adrs/ADR-0044-digest-pluggability.md	11135	architecture charter doc
specs/03_architecture/adrs/ADR-0044-sbom-cyclonedx-toolchain.md	8157	architecture charter doc
specs/03_architecture/adrs/ADR-0045-slsa-l3-rekor-mandatory.md	7271	architecture charter doc
specs/03_architecture/adrs/ADR-S11-001-mfa-step-up-destructive-arms-only.md	2833	architecture charter doc
specs/03_architecture/adrs/ADR-S11-002-split-tier-audit-fail-closed.md	3400	architecture charter doc
specs/03_architecture/adrs/ADR-S11-003-erasure-salt-interim.md	4899	architecture charter doc
specs/03_architecture/adrs/ADR-S11-004-cross-backend-eventual-consistency.md	6504	architecture charter doc
specs/03_architecture/adrs/ADR-S11-005-consent-symmetric-grant-revoke-schema.md	2677	architecture charter doc
specs/03_architecture/adrs/ADR-S11-006-consent-purpose-12-arm-closed-enum.md	3130	architecture charter doc
specs/03_architecture/adrs/ADR-S11-007-privacy-notice-material-vs-minor.md	4427	architecture charter doc
specs/03_architecture/adrs/ADR-S11-008-sub-processor-default-subscribed-tier-team-plus.md	4204	architecture charter doc
specs/03_architecture/adrs/ADR-S11-009-breach-notification-3-jurisdictional-coverage-rationale.md	6201	architecture charter doc
specs/03_architecture/adrs/ADR-S11-010-tla-residency-deferred-s14.md	2945	architecture charter doc
specs/03_architecture/adrs/ADR-S11-011-region-migration-cooldown-30d.md	2623	architecture charter doc
specs/03_architecture/adrs/ADR-S11-012-tla-scope-discipline-s11-erasure-only.md	7241	architecture charter doc
specs/03_architecture/adrs/ADR-S12-001-sbom-cyclonedx-ntia-tsa-dt.md	5829	architecture charter doc
specs/03_architecture/adrs/ADR-S12-045-dep-policy-cargo-audit-deny-dependabot.md	5358	architecture charter doc
specs/03_architecture/adrs/ADR-S12-046-license-allowlist-7-osi.md	5888	architecture charter doc
specs/03_architecture/adrs/ADR-S12-047-license-review-quarterly.md	5309	architecture charter doc
specs/03_architecture/adrs/ADR-S13-001-do-config-singleton-cas.md	4618	architecture charter doc
specs/03_architecture/adrs/ADR-S13-002-dual-approval-collusion-rotation.md	4561	architecture charter doc
specs/03_architecture/adrs/ADR-S13-005-progressive-rollout-budget-cap.md	6714	architecture charter doc
specs/03_architecture/adrs/ADR-S14-001-multi-region-terraform-module.md	8276	architecture charter doc
specs/03_architecture/adrs/ADR-S14-002-region-pinning-enforcement.md	5627	architecture charter doc
specs/03_architecture/adrs/ADR-S14-003-hot-blob-replica-offline-aggregation.md	7083	architecture charter doc
specs/03_architecture/adrs/ADR-S14-004-byok-trait-envelope-encryption.md	6159	architecture charter doc
specs/03_architecture/adrs/ADR-S14-005-byok-gcp-azure-vault.md	7881	architecture charter doc
specs/03_architecture/adrs/ADR-S14-006-byok-kill-switch-no-operator-override.md	5827	architecture charter doc
specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md	5012	architecture charter doc
specs/03_architecture/adrs/ADR-S14-008-dpa-amendment-schrems-ii-tia-legal-externo.md	8856	architecture charter doc
specs/03_architecture/adrs/ADR-S15-009-windows-codesign-deferral.md	5329	architecture charter doc
specs/03_architecture/adrs/ADR-S20-RSA-MARVIN-MITIGATION.md	9755	architecture charter doc
specs/03_architecture/adrs/ADR-S30-001-byok-mutually-exclusive-providers.md	16214	architecture charter doc
specs/03_architecture/adrs/WAIVER-S14-001-legal-externo-timeline.md	5167	architecture charter doc
specs/03_architecture/auth_model.md	21438	architecture charter doc
specs/03_architecture/auth_stub_contract.md	11164	architecture charter doc
specs/03_architecture/compliance_matrix.md	28017	architecture charter doc
specs/03_architecture/data_model.md	30279	architecture charter doc
specs/03_architecture/error_taxonomy.md	24307	architecture charter doc
specs/03_architecture/failure_modes.md	23519	architecture charter doc
specs/03_architecture/invariant_registry.md	161060	architecture charter doc
specs/03_architecture/key_management.md	16079	architecture charter doc
specs/03_architecture/observability_model.md	32582	architecture charter doc
specs/03_architecture/privacy_model.md	41102	architecture charter doc
specs/03_architecture/remote_cache_product_profile.md	23399	architecture charter doc
specs/03_architecture/resilience_patterns.md	28593	architecture charter doc
specs/03_architecture/security_model.md	48683	architecture charter doc
specs/03_architecture/slo_catalog.md	38827	architecture charter doc
specs/03_architecture/storage_semantics_matrix.md	16235	architecture charter doc
```
</details>


<details>
<summary><b>§4.2 SEALED-HISTORICAL — 365 files, 6116.2 KB</b></summary>

```
specs/_audits/2026-04-24-codex-sota-review-r2.md	42338	audit/seal artifact
specs/_audits/2026-04-24-codex-sota-review-sprint-contracts.md	25214	audit/seal artifact
specs/_audits/2026-04-24-gpt-audit-lote3-4.md	12747	audit/seal artifact
specs/_audits/2026-04-24-gpt-audit-lote5.md	20667	audit/seal artifact
specs/_audits/2026-04-24-gpt-audit-lote6.md	13296	audit/seal artifact
specs/_audits/2026-04-24-gpt-audit-v1.md	37827	audit/seal artifact
specs/_audits/2026-04-24-gpt-review-lote1.md	14043	audit/seal artifact
specs/_audits/2026-04-24-gpt-review-lote1bis.md	7513	audit/seal artifact
specs/_audits/2026-04-24-gpt-review-lote1quater.md	4336	audit/seal artifact
specs/_audits/2026-04-24-gpt-review-lote1quinquies.md	2347	audit/seal artifact
specs/_audits/2026-04-24-gpt-review-lote1ter.md	5151	audit/seal artifact
specs/_audits/2026-04-24-opus-independent-sota-review-r2.md	33814	audit/seal artifact
specs/_audits/2026-04-24-self-audit-lote2.md	16077	audit/seal artifact
specs/_audits/2026-04-24-sonnet-audit-lote3-4.md	32859	audit/seal artifact
specs/_audits/2026-04-24-sonnet-audit-lote5.md	30414	audit/seal artifact
specs/_audits/2026-04-24-sonnet-audit-lote6.md	25401	audit/seal artifact
specs/_audits/2026-04-25-agent-r4-s01-wi-review.md	37674	audit/seal artifact
specs/_audits/2026-04-25-agent-r4-s02-wi-review.md	34434	audit/seal artifact
specs/_audits/2026-04-25-agent-r4-s03-part1-wi-review.md	50625	audit/seal artifact
specs/_audits/2026-04-25-agent-r4-s03-part2-wi-review.md	49561	audit/seal artifact
specs/_audits/2026-04-25-agent-r4-s04-part1-wi-review.md	60671	audit/seal artifact
specs/_audits/2026-04-25-agent-r4-s04-part2-wi-review.md	61498	audit/seal artifact
specs/_audits/2026-04-25-agent-r4-s05-part1-wi-review.md	69946	audit/seal artifact
specs/_audits/2026-04-25-agent-r4-s05-part2-wi-review.md	77955	audit/seal artifact
specs/_audits/2026-04-25-agent-r4-s06-part1-wi-review.md	59467	audit/seal artifact
specs/_audits/2026-04-25-agent-r4-s06-part2a-wi-review.md	36691	audit/seal artifact
specs/_audits/2026-04-25-agent-r4-s06-part2b-wi-review.md	53703	audit/seal artifact
specs/_audits/2026-04-25-agent-r4-s07-wi-review.md	53240	audit/seal artifact
specs/_audits/2026-04-25-agent-r4-s08-wi-review.md	23622	audit/seal artifact
specs/_audits/2026-04-25-agent-r4-s09-wi-review.md	22351	audit/seal artifact
specs/_audits/2026-04-25-codex-r4-s01-wi-review.md	19671	audit/seal artifact
specs/_audits/2026-04-25-codex-r4-s02-wi-review.md	14005	audit/seal artifact
specs/_audits/2026-04-25-codex-sota-review-r3.md	21176	audit/seal artifact
specs/_audits/2026-04-25-opus-independent-sota-review-r3.md	43114	audit/seal artifact
specs/_audits/2026-04-25-sonnet-r5-s03-wi-review.md	33062	audit/seal artifact
specs/_audits/2026-04-25-sonnet-r5-s04-wi-review.md	44241	audit/seal artifact
specs/_audits/2026-04-25-sonnet-r5-s05-wi-review.md	37233	audit/seal artifact
specs/_audits/2026-04-25-sonnet-r5-s06-wi-review.md	36668	audit/seal artifact
specs/_audits/2026-04-25-sonnet-r5-s07-wi-review.md	39249	audit/seal artifact
specs/_audits/2026-04-25-sonnet-r5-s08-wi-review.md	32597	audit/seal artifact
specs/_audits/2026-04-25-sonnet-r5-s09-wi-review.md	25861	audit/seal artifact
specs/_audits/2026-04-25-sonnet-r5-tris-s08-wi-review.md	28345	audit/seal artifact
specs/_audits/2026-04-25-sonnet-r5-tris-s09-wi-review.md	34580	audit/seal artifact
specs/_audits/2026-04-26-agent-r4-s10-quinquies-validation.md	26716	audit/seal artifact
specs/_audits/2026-04-26-agent-r4-s10-septies-validation.md	17880	audit/seal artifact
specs/_audits/2026-04-26-agent-r4-s10-tris-validation.md	28011	audit/seal artifact
specs/_audits/2026-04-26-agent-r4-s10-wi-review.md	31289	audit/seal artifact
specs/_audits/2026-04-26-sonnet-r5-quinquies-s09-wi-review.md	19719	audit/seal artifact
specs/_audits/2026-04-26-sonnet-r5-s10-quinquies-validation.md	33852	audit/seal artifact
specs/_audits/2026-04-26-sonnet-r5-s10-septies-validation.md	17958	audit/seal artifact
specs/_audits/2026-04-26-sonnet-r5-s10-tris-validation.md	31977	audit/seal artifact
specs/_audits/2026-04-26-sonnet-r5-s10-wi-review.md	29426	audit/seal artifact
specs/_audits/2026-04-30-pentest-s02-internal.md	9073	audit/seal artifact
specs/_audits/2026-05-01-S04-WIP-FINDINGS.md	4385	audit/seal artifact
specs/_audits/2026-05-01-adversarial-s03.md	11201	audit/seal artifact
specs/_audits/2026-05-01-adversarial-s04.md	11850	audit/seal artifact
specs/_audits/2026-05-01-adversarial-s05.md	13506	audit/seal artifact
specs/_audits/2026-05-01-pentest-s03-internal.md	15394	audit/seal artifact
specs/_audits/2026-05-01-pentest-s04-internal.md	16687	audit/seal artifact
specs/_audits/2026-05-01-pentest-s05-internal.md	14272	audit/seal artifact
specs/_audits/2026-05-01-rb-fm-060-dry-run.md	7551	audit/seal artifact
specs/_audits/2026-05-01-rb-fm-160-dry-run.md	5895	audit/seal artifact
specs/_audits/2026-05-01-rb-fm-303-dry-run.md	6707	audit/seal artifact
specs/_audits/2026-05-02-adversarial-s06.md	12466	audit/seal artifact
specs/_audits/2026-05-02-adversarial-s07.md	10261	audit/seal artifact
specs/_audits/2026-05-02-adversarial-s08.md	9924	audit/seal artifact
specs/_audits/2026-05-02-pentest-s06-internal.md	8686	audit/seal artifact
specs/_audits/2026-05-02-rb-fm-059-dry-run.md	6286	audit/seal artifact
specs/_audits/2026-05-02-rb-fm-250-dry-run.md	7487	audit/seal artifact
specs/_audits/2026-05-02-rb-fm-300-dry-run.md	4781	audit/seal artifact
specs/_audits/2026-05-02-rb-fm-305-dry-run.md	5891	audit/seal artifact
specs/_audits/2026-05-02-rb-fm-305-s07-dry-run.md	6746	audit/seal artifact
specs/_audits/2026-05-02-rb-fm-404-dry-run.md	6170	audit/seal artifact
specs/_audits/2026-05-03-adversarial-s09.md	9106	audit/seal artifact
specs/_audits/2026-05-03-adversarial-s10.md	14251	audit/seal artifact
specs/_audits/2026-05-03-rb-billing-001-replay-forensic-dry-run.md	8819	audit/seal artifact
specs/_audits/2026-05-03-rb-fm-151-stripe-outage-dry-run.md	7804	audit/seal artifact
specs/_audits/2026-05-03-rb-fm-153-dry-run.md	7650	audit/seal artifact
specs/_audits/2026-05-03-rb-fm-302-billing-drift-dry-run.md	7222	audit/seal artifact
specs/_audits/2026-05-03-rb-obs-cardinality-001-dry-run.md	7868	audit/seal artifact
specs/_audits/2026-05-13-rb-fm-156-dry-run.md	7823	audit/seal artifact
specs/_audits/2026-05-14-adversarial-summary-s12.md	9651	audit/seal artifact
specs/_audits/2026-05-14-adversarial-summary-s13.md	9684	audit/seal artifact
specs/_audits/2026-05-14-byok-kill-switch-drill-aws.md	820	audit/seal artifact
specs/_audits/2026-05-14-cargo-fuzz-summary-s15.md	7115	audit/seal artifact
specs/_audits/2026-05-14-coverage-baseline.md	3204	audit/seal artifact
specs/_audits/2026-05-14-legal-review-dpa-v1.0.0.md	1829	audit/seal artifact
specs/_audits/2026-05-14-license-audit.md	4663	audit/seal artifact
specs/_audits/2026-05-14-linddun-cli-telemetry.md	7511	audit/seal artifact
specs/_audits/2026-05-14-mutation-baseline.md	18265	audit/seal artifact
specs/_audits/2026-05-14-pentest-s14-byok.md	7459	audit/seal artifact
specs/_audits/2026-05-14-perf-baseline.md	8691	audit/seal artifact
specs/_audits/2026-05-14-property-test-summary-s13.md	8606	audit/seal artifact
specs/_audits/2026-05-14-property-test-summary-s19.md	7769	audit/seal artifact
specs/_audits/2026-05-14-rb-byok-revoke-dry-run.md	2362	audit/seal artifact
specs/_audits/2026-05-14-rb-fm-054-dry-run.md	4688	audit/seal artifact
specs/_audits/2026-05-14-rb-fm-105-dry-run.md	5783	audit/seal artifact
specs/_audits/2026-05-14-rb-fm-157-dry-run.md	8923	audit/seal artifact
specs/_audits/2026-05-14-rb-fm-201-dry-run.md	6504	audit/seal artifact
specs/_audits/2026-05-14-rb-fm-205-dry-run.md	6444	audit/seal artifact
specs/_audits/2026-05-14-rb-fm-206-dry-run.md	6961	audit/seal artifact
specs/_audits/2026-05-14-region-outage-chaos-s14.md	6316	audit/seal artifact
specs/_audits/2026-05-14-roadmap-r1-8-dependency-audit.md	14235	audit/seal artifact
specs/_audits/2026-05-14-runbook-coverage.md	15466	audit/seal artifact
specs/_audits/2026-05-14-s15-adversarial-summary.md	12134	audit/seal artifact
specs/_audits/2026-05-14-s15-sprint-close-review-round1.md	13205	audit/seal artifact
specs/_audits/2026-05-14-s15-sprint-close-review-round2.md	15290	audit/seal artifact
specs/_audits/2026-05-14-s16-adversarial-summary.md	8078	audit/seal artifact
specs/_audits/2026-05-14-s16-sprint-close-review-round1.md	11201	audit/seal artifact
specs/_audits/2026-05-14-s16-sprint-close-review-round2.md	11291	audit/seal artifact
specs/_audits/2026-05-14-s16-ux-workshop.md	5300	audit/seal artifact
specs/_audits/2026-05-14-s17-adversarial-summary.md	8942	audit/seal artifact
specs/_audits/2026-05-14-s17-sprint-close-review-round1.md	13224	audit/seal artifact
specs/_audits/2026-05-14-s17-sprint-preflight-review.md	23411	audit/seal artifact
specs/_audits/2026-05-14-s17-tabletop-byok-revoke.md	9073	audit/seal artifact
specs/_audits/2026-05-14-s18-adversarial-summary.md	8452	audit/seal artifact
specs/_audits/2026-05-14-s18-sprint-close-review-round1.md	11392	audit/seal artifact
specs/_audits/2026-05-14-s18-sprint-close-review-round2.md	13743	audit/seal artifact
specs/_audits/2026-05-14-s18-sprint-preflight-review.md	13773	audit/seal artifact
specs/_audits/2026-05-14-s18-ux-research.md	5889	audit/seal artifact
specs/_audits/2026-05-14-s19-adversarial-summary.md	12478	audit/seal artifact
specs/_audits/2026-05-14-s19-sprint-close-review-round1.md	19782	audit/seal artifact
specs/_audits/2026-05-14-s19-sprint-preflight-review.md	20271	audit/seal artifact
specs/_audits/2026-05-14-s20-30d-staging-evidence.md	7662	audit/seal artifact
specs/_audits/2026-05-14-s20-adversarial-summary.md	11338	audit/seal artifact
specs/_audits/2026-05-14-s20-oncall-24-7-readiness.md	11548	audit/seal artifact
specs/_audits/2026-05-14-s20-prr-global-coverage.md	15511	audit/seal artifact
specs/_audits/2026-05-14-s20-sbom-90d-retention.md	8193	audit/seal artifact
specs/_audits/2026-05-14-s20-sprint-close-review-round1.md	23466	audit/seal artifact
specs/_audits/2026-05-14-s20-sprint-close-review-round2.md	19788	audit/seal artifact
specs/_audits/2026-05-14-s20-sprint-preflight-review.md	18275	audit/seal artifact
specs/_audits/2026-05-14-sbom-coverage.md	4985	audit/seal artifact
specs/_audits/2026-05-14-security-walkthrough-s12.md	7620	audit/seal artifact
specs/_audits/2026-05-14-security-walkthrough-s13.md	9619	audit/seal artifact
specs/_audits/2026-05-14-slo-instrumentation-gaps.md	18326	audit/seal artifact
specs/_audits/2026-05-14-soc2-readiness-score.md	8026	audit/seal artifact
specs/_audits/2026-05-15-action-sha-pinning-baseline.md	8060	audit/seal artifact
specs/_audits/2026-05-15-actionlint-baseline.md	17448	audit/seal artifact
specs/_audits/2026-05-15-api-stability-baseline.md	11825	audit/seal artifact
specs/_audits/2026-05-15-audit-chain-retention.md	15649	audit/seal artifact
specs/_audits/2026-05-15-audit-viz-spec.md	11397	audit/seal artifact
specs/_audits/2026-05-15-byok-real-provider-pattern.md	24500	audit/seal artifact
specs/_audits/2026-05-15-canonical-consistency-baseline.md	19837	audit/seal artifact
specs/_audits/2026-05-15-cf-binding-real-pattern.md	23337	audit/seal artifact
specs/_audits/2026-05-15-ci-workflow-optimization.md	23208	audit/seal artifact
specs/_audits/2026-05-15-codeql-semgrep-baseline.md	14479	audit/seal artifact
specs/_audits/2026-05-15-customer-breach-notification-templates.md	21110	audit/seal artifact
specs/_audits/2026-05-15-customer-dashboard-spec.md	6566	audit/seal artifact
specs/_audits/2026-05-15-dangling-refs-closure.md	9181	audit/seal artifact
specs/_audits/2026-05-15-debt-014-ft3-ft4-waivers.md	6440	audit/seal artifact
specs/_audits/2026-05-15-debt-register.md	136794	audit/seal artifact
specs/_audits/2026-05-15-dependabot-policy.md	12252	audit/seal artifact
specs/_audits/2026-05-15-dsr-worker-production.md	32764	audit/seal artifact
specs/_audits/2026-05-15-framework-v1-0-0-ga-audit.md	39771	audit/seal artifact
specs/_audits/2026-05-15-ga-readiness-consolidation-wave-13-17.md	40735	audit/seal artifact
specs/_audits/2026-05-15-mutation-expansion.md	17906	audit/seal artifact
specs/_audits/2026-05-15-mutation-full-sweep.md	20161	audit/seal artifact
specs/_audits/2026-05-15-neon-analytics-shadow.md	13596	audit/seal artifact
specs/_audits/2026-05-15-otel-export-spec.md	13026	audit/seal artifact
specs/_audits/2026-05-15-perf-opt-validation-report.md	10359	audit/seal artifact
specs/_audits/2026-05-15-perf-optimization-audit.md	24978	audit/seal artifact
specs/_audits/2026-05-15-proptest-density.md	17097	audit/seal artifact
specs/_audits/2026-05-15-ratelimit-ux-audit.md	10885	audit/seal artifact
specs/_audits/2026-05-15-replica-coordinator-production.md	13819	audit/seal artifact
specs/_audits/2026-05-15-replication-audit.md	29746	audit/seal artifact
specs/_audits/2026-05-15-s11-legal-citation-revalidation.md	37601	audit/seal artifact
specs/_audits/2026-05-15-s11-round-2-validation.md	18245	audit/seal artifact
specs/_audits/2026-05-15-s11-truth-table-sweep-v2.md	18466	audit/seal artifact
specs/_audits/2026-05-15-secrets-coverage-baseline.md	8452	audit/seal artifact
specs/_audits/2026-05-15-static-analysis-baseline.md	6578	audit/seal artifact
specs/_audits/2026-05-15-stripe-customer-portal-spec.md	14145	audit/seal artifact
specs/_audits/2026-05-15-stripe-webhook-production.md	32307	audit/seal artifact
specs/_audits/2026-05-15-tla-coverage-audit.md	14525	audit/seal artifact
specs/_audits/2026-05-15-webhook-retry-dlq.md	11340	audit/seal artifact
specs/_audits/2026-05-16-24h-endurance-harness.md	8195	audit/seal artifact
specs/_audits/2026-05-16-audit-chain-viz-ui.md	6297	audit/seal artifact
specs/_audits/2026-05-16-auth-pat-revoke-tla.md	16098	audit/seal artifact
specs/_audits/2026-05-16-beta-feedback-triage-harness.md	8484	audit/seal artifact
specs/_audits/2026-05-16-byok-ap11-adr-formalization.md	7553	audit/seal artifact
specs/_audits/2026-05-16-cf-worker-prefetch-wire.md	17645	audit/seal artifact
specs/_audits/2026-05-16-chaos-campaign-harness.md	9346	audit/seal artifact
specs/_audits/2026-05-16-chaos-combined-failures.md	10525	audit/seal artifact
specs/_audits/2026-05-16-corelink-py-linker-fix.md	10026	audit/seal artifact
specs/_audits/2026-05-16-cutover-dependency-map.md	28658	audit/seal artifact
specs/_audits/2026-05-16-debt-003-aws-artifact-placeholder.md	11748	audit/seal artifact
specs/_audits/2026-05-16-debt-008-mutation-sweep.md	17344	audit/seal artifact
specs/_audits/2026-05-16-debt-008-number-discrepancy-fix.md	6899	audit/seal artifact
specs/_audits/2026-05-16-debt-008-wave22-mutation-sweep.md	15347	audit/seal artifact
specs/_audits/2026-05-16-debt-008-wave23-mutation-sweep.md	22414	audit/seal artifact
specs/_audits/2026-05-16-debt-008-wave24-mutation-sweep.md	21439	audit/seal artifact
specs/_audits/2026-05-16-debt-015-build-closure.md	9041	audit/seal artifact
specs/_audits/2026-05-16-debt-015-build-final.md	20920	audit/seal artifact
specs/_audits/2026-05-16-debt-015-build-wave25-closure.md	7518	audit/seal artifact
specs/_audits/2026-05-16-debt-016-statuspage-urls.md	8039	audit/seal artifact
specs/_audits/2026-05-16-debt-026-rfp-tracker.md	17462	audit/seal artifact
specs/_audits/2026-05-16-debt-029-cas-route-syntax-fix.md	6255	audit/seal artifact
specs/_audits/2026-05-16-debt-029-route-syntax-fix.md	10766	audit/seal artifact
specs/_audits/2026-05-16-endurance-10min-dressrun.md	12892	audit/seal artifact
specs/_audits/2026-05-16-final-cutover-readiness-checklist.md	7879	audit/seal artifact
specs/_audits/2026-05-16-final-cutover-readiness.md	25925	audit/seal artifact
specs/_audits/2026-05-16-ga-1-feature-freeze.md	12629	audit/seal artifact
specs/_audits/2026-05-16-ga-cutover-dryrun.md	11419	audit/seal artifact
specs/_audits/2026-05-16-ga-final-checklist.md	11880	audit/seal artifact
specs/_audits/2026-05-16-ga-readiness-defer-scrub.md	6433	audit/seal artifact
specs/_audits/2026-05-16-ga-readiness-final.md	41483	audit/seal artifact
specs/_audits/2026-05-16-inv-critical-tla-coverage-final.md	15270	audit/seal artifact
specs/_audits/2026-05-16-inv-draft-sweep.md	9749	audit/seal artifact
specs/_audits/2026-05-16-inv-signup-token-tla.md	15672	audit/seal artifact
specs/_audits/2026-05-16-lfpdppp-mx-engagement-package-final.md	22403	audit/seal artifact
specs/_audits/2026-05-16-lfpdppp-mx-legal-review-package.md	25437	audit/seal artifact
specs/_audits/2026-05-16-lote-6-owner-signoff-prep.md	23352	audit/seal artifact
specs/_audits/2026-05-16-lote-6-v1-rc2-ready.md	12512	audit/seal artifact
specs/_audits/2026-05-16-lote-7-followons-closure.md	16158	audit/seal artifact
specs/_audits/2026-05-16-lote-7-raci-detail.md	20174	audit/seal artifact
specs/_audits/2026-05-16-neon-shadow-pg-testharness.md	12633	audit/seal artifact
specs/_audits/2026-05-16-neon-shadow-real-driver.md	22162	audit/seal artifact
specs/_audits/2026-05-16-p2-absorption-sweep-w25-28.md	14961	audit/seal artifact
specs/_audits/2026-05-16-pat-clerk-mutation-sweep.md	19819	audit/seal artifact
specs/_audits/2026-05-16-pentest-engagement-scope-freeze.md	35374	audit/seal artifact
specs/_audits/2026-05-16-pentest-finding-absorption-framework.md	13163	audit/seal artifact
specs/_audits/2026-05-16-pentest-rfp-send-ceremony.md	9805	audit/seal artifact
specs/_audits/2026-05-16-perf-baseline-ga-freeze.md	11939	audit/seal artifact
specs/_audits/2026-05-16-perf-benches-recapture.md	13944	audit/seal artifact
specs/_audits/2026-05-16-perf-regression-ci-tightened.md	9847	audit/seal artifact
specs/_audits/2026-05-16-pilot-admin-web-ui.md	8968	audit/seal artifact
specs/_audits/2026-05-16-pilot-comms-package.md	9033	audit/seal artifact
specs/_audits/2026-05-16-pilot-onboarding-e2e.md	9882	audit/seal artifact
specs/_audits/2026-05-16-pilot-signup-pipeline.md	11431	audit/seal artifact
specs/_audits/2026-05-16-pre-cutover-state-snapshot.md	14653	audit/seal artifact
specs/_audits/2026-05-16-pre-cutover-weekly-verify.md	14444	audit/seal artifact
specs/_audits/2026-05-16-pre-ga-pentest-scope.md	40561	audit/seal artifact
specs/_audits/2026-05-16-pre-ga-security-attestation.md	31189	audit/seal artifact
specs/_audits/2026-05-16-prexisting-test-failures-triage.md	12381	audit/seal artifact
specs/_audits/2026-05-16-pricing-page.md	6018	audit/seal artifact
specs/_audits/2026-05-16-prod-deploy-dressrun.md	17204	audit/seal artifact
specs/_audits/2026-05-16-release-notes-editorial-polish.md	13958	audit/seal artifact
specs/_audits/2026-05-16-secrets-matrix-tighten.md	15676	audit/seal artifact
specs/_audits/2026-05-16-secrets-x-false-positive-fix.md	9866	audit/seal artifact
specs/_audits/2026-05-16-shadow-sink-consumer-adoption.md	15943	audit/seal artifact
specs/_audits/2026-05-16-shadow-sink-full-adoption.md	16361	audit/seal artifact
specs/_audits/2026-05-16-signup-corelink-dev-backend.md	14144	audit/seal artifact
specs/_audits/2026-05-16-signup-landing-page.md	7340	audit/seal artifact
specs/_audits/2026-05-16-signup-live-d1-tests.md	11089	audit/seal artifact
specs/_audits/2026-05-16-statuspage-init-dressrun.md	13829	audit/seal artifact
specs/_audits/2026-05-16-stripe-wasm32-gate-lift.md	17060	audit/seal artifact
specs/_audits/2026-05-16-tenant-config-cf-prod-wire.md	10954	audit/seal artifact
specs/_audits/2026-05-16-tla-figure-refresh-wave27.md	12684	audit/seal artifact
specs/_audits/2026-05-16-trust-center-consolidation.md	16864	audit/seal artifact
specs/_audits/2026-05-16-v1-tag-application-prep.md	22872	audit/seal artifact
specs/_audits/2026-05-16-w25-p2-02-babel-patch-retest.md	10198	audit/seal artifact
specs/_audits/2026-05-16-w26-p2-01-freeze-merge-trailer.md	12706	audit/seal artifact
specs/_audits/2026-05-16-w26-p2-03-inv-inheritance.md	11219	audit/seal artifact
specs/_audits/2026-05-16-w26-p2-08-mutex-poison-telemetry.md	13761	audit/seal artifact
specs/_audits/2026-05-16-wallet-broker-stripe.md	32614	audit/seal artifact
specs/_audits/2026-05-16-wasm32-baseline-getrandom-fix.md	9403	audit/seal artifact
specs/_audits/2026-05-16-wave18-adversarial-review-streamA-audit-export.md	22976	audit/seal artifact
specs/_audits/2026-05-16-wave18-adversarial-review-streamB-neon-shadow.md	27012	audit/seal artifact
specs/_audits/2026-05-16-wave18-aggregate-closure.md	5626	audit/seal artifact
specs/_audits/2026-05-16-wave19-adversarial-review.md	17828	audit/seal artifact
specs/_audits/2026-05-16-wave20-adversarial-review.md	17949	audit/seal artifact
specs/_audits/2026-05-16-wave20-closure.md	18283	audit/seal artifact
specs/_audits/2026-05-16-wave21-adversarial-review.md	14618	audit/seal artifact
specs/_audits/2026-05-16-wave21-closure.md	17621	audit/seal artifact
specs/_audits/2026-05-16-wave22-adversarial-review.md	13036	audit/seal artifact
specs/_audits/2026-05-16-wave22-closure.md	27395	audit/seal artifact
specs/_audits/2026-05-16-wave23-adversarial-review.md	19661	audit/seal artifact
specs/_audits/2026-05-16-wave23-cleanup.md	13497	audit/seal artifact
specs/_audits/2026-05-16-wave23-closure.md	33417	audit/seal artifact
specs/_audits/2026-05-16-wave24-adversarial-review.md	20826	audit/seal artifact
specs/_audits/2026-05-16-wave24-closure.md	41050	audit/seal artifact
specs/_audits/2026-05-16-wave25-adversarial-review.md	26934	audit/seal artifact
specs/_audits/2026-05-16-wave25-closure.md	37654	audit/seal artifact
specs/_audits/2026-05-16-wave26-adversarial-review.md	34273	audit/seal artifact
specs/_audits/2026-05-16-wave26-closure.md	42216	audit/seal artifact
specs/_audits/2026-05-16-wave27-closure.md	29399	audit/seal artifact
specs/_audits/2026-05-16-wave28-adversarial-review.md	27556	audit/seal artifact
specs/_audits/2026-05-16-wave29-adversarial-review.md	15106	audit/seal artifact
specs/_audits/2026-05-16-wave29-closure.md	30343	audit/seal artifact
specs/_audits/2026-05-16-wave30-closure.md	36396	audit/seal artifact
specs/_audits/2026-05-22-w32-phaseA-betterstack-live.md	4774	audit/seal artifact
specs/_audits/2026-05-22-w33-stage0-foundation.md	17128	audit/seal artifact
specs/_audits/2026-05-22-w33-stage2-a-worker-moves.md	23230	audit/seal artifact
specs/_audits/2026-05-22-w33-stage2-c-adapter-splits.md	17800	audit/seal artifact
specs/_audits/2026-05-22-w33-stage2-d-out-of-tree.md	11417	audit/seal artifact
specs/_audits/2026-05-22-w33-stage2-pre-a-worker-megafiles.md	19054	audit/seal artifact
specs/_audits/2026-05-22-w33-stage2-pre-b-audit-megafiles.md	13822	audit/seal artifact
specs/_audits/2026-05-22-w33-stream-a-data-path.md	19491	audit/seal artifact
specs/_audits/2026-05-22-w33-stream-a2-megafiles.md	14870	audit/seal artifact
specs/_audits/2026-05-22-w33-stream-b-policy.md	15972	audit/seal artifact
specs/_audits/2026-05-22-w33-stream-c-infra-ops.md	18690	audit/seal artifact
specs/_audits/2026-05-22-wave32-prod-deploy-spec.md	17759	audit/seal artifact
specs/_audits/2026-05-22-wave33-code-reorg-spec.md	22167	audit/seal artifact
specs/_audits/2026-05-26-w32-phaseB-worker-shim.md	9624	audit/seal artifact
specs/_audits/2026-05-26-w32-phaseC-cf-provision.md	9827	audit/seal artifact
specs/_audits/2026-05-26-w32-phaseD-apply-HALT.md	14062	audit/seal artifact
specs/_audits/2026-05-26-w32-phaseD-apply.md	11200	audit/seal artifact
specs/_audits/2026-05-26-w32-phaseD-prep.md	19516	audit/seal artifact
specs/_audits/2026-05-26-w32-phaseE-apply.md	21640	audit/seal artifact
specs/_audits/2026-05-26-w32-phaseE-prep.md	15124	audit/seal artifact
specs/_audits/2026-05-26-w32-phaseF-apply-admin-ui-closure.md	13008	audit/seal artifact
specs/_audits/2026-05-26-w32-phaseF-apply.md	12494	audit/seal artifact
specs/_audits/2026-05-26-w32-phaseF-prep.md	16981	audit/seal artifact
specs/_audits/2026-05-26-w32-phaseG-apply.md	12300	audit/seal artifact
specs/_audits/2026-05-26-w32-phaseG-prep.md	14820	audit/seal artifact
specs/_audits/2026-05-26-w32-phaseH-apply.md	15311	audit/seal artifact
specs/_audits/2026-05-26-w32-phaseH-cutover-checklist-20260526T235900Z.md	3286	audit/seal artifact
specs/_audits/2026-05-26-w32-phaseH-prep.md	19888	audit/seal artifact
specs/_audits/2026-05-26-w32-phaseI-signoff.md	10123	audit/seal artifact
specs/_audits/2026-05-26-w33-stage2-a-v2-additive-aggregator.md	6403	audit/seal artifact
specs/_audits/2026-05-26-w33-stage2-b-container.md	14938	audit/seal artifact
specs/_audits/2026-05-26-w33-stage2-e-consumer-migration.md	17027	audit/seal artifact
specs/_audits/2026-05-26-w34-adapter-brew.md	9349	audit/seal artifact
specs/_audits/2026-05-26-w34-adapter-cargo-halt.md	13589	audit/seal artifact
specs/_audits/2026-05-26-w34-adapter-cargo-v2.md	9675	audit/seal artifact
specs/_audits/2026-05-26-w34-adapter-npm-v2.md	8273	audit/seal artifact
specs/_audits/2026-05-26-w34-adapter-oci.md	14862	audit/seal artifact
specs/_audits/2026-05-26-w34-adapter-pip.md	7935	audit/seal artifact
specs/_audits/2026-05-26-w35-adapter-host-prep.md	5556	audit/seal artifact
specs/_audits/2026-05-26-w35-p2-ac-absorption.md	10916	audit/seal artifact
specs/_audits/2026-05-26-w35-p2-adapter-host-absorption.md	12715	audit/seal artifact
specs/_audits/2026-05-26-w35-p2-billing-absorption.md	11313	audit/seal artifact
specs/_audits/2026-05-26-w35-p2-byok-absorption.md	21525	audit/seal artifact
specs/_audits/2026-05-26-w35-p2-cas-absorption.md	6235	audit/seal artifact
specs/_audits/2026-05-26-w35-p2-ops-absorption.md	23764	audit/seal artifact
specs/_audits/2026-05-26-w35-p2-privacy-absorption.md	12049	audit/seal artifact
specs/_audits/2026-05-26-w35-p2-replication-absorption.md	10883	audit/seal artifact
specs/_audits/2026-05-26-w35-p2-telemetry-absorption.md	9571	audit/seal artifact
specs/_audits/2026-05-26-w36-proptest-fu-001-seal.md	11460	audit/seal artifact
specs/_audits/2026-05-26-w36-proptest-fu-002-seal.md	7611	audit/seal artifact
specs/_audits/2026-05-26-w36-proptest-wasm-seal.md	3454	audit/seal artifact
specs/_audits/2026-05-26-w36-stage2c-closure.md	17518	audit/seal artifact
specs/_audits/2026-05-26-w36-trigger-b-seal.md	5481	audit/seal artifact
specs/_audits/2026-05-26-wave-33-34-closure-followups.md	15577	audit/seal artifact
specs/_audits/2026-05-27-w36-stage-3-seal.md	13483	audit/seal artifact
specs/_audits/2026-05-27-w36-trigger-a-seal.md	7914	audit/seal artifact
specs/_audits/2026-XX-XX-legal-externo-review-s14.md	3126	audit/seal artifact
specs/_audits/2026-XX-XX-lighthouse-customer-dpa-signed.md	2030	audit/seal artifact
specs/_audits/HARDENING_SPRINT_2026-05-07_inv_audit.md	14856	audit/seal artifact
specs/_audits/STATE-2026-04-25-pre-compact-v2.md	8046	audit/seal artifact
specs/_audits/STATE-2026-04-25-pre-compact.md	5425	audit/seal artifact
specs/_audits/ci-optimization-followup-tickets.md	19591	audit/seal artifact
specs/_audits/pentest-vendor-shortlist.md	23111	audit/seal artifact
specs/_audits/perf-optimization-followup-tickets.md	24067	audit/seal artifact
specs/_audits/proptest-followup-tickets.md	17929	audit/seal artifact
specs/_audits/replication-followup-tickets.md	22028	audit/seal artifact
specs/_audits/stride-per-crate/INDEX.md	3983	audit/seal artifact
specs/_audits/stride-per-crate/STRIDE-corelink-audit-chain.md	8472	audit/seal artifact
specs/_audits/stride-per-crate/STRIDE-corelink-byok.md	10743	audit/seal artifact
specs/_audits/stride-per-crate/STRIDE-corelink-clerk.md	7423	audit/seal artifact
specs/_audits/stride-per-crate/STRIDE-corelink-dsr.md	7856	audit/seal artifact
specs/_audits/stride-per-crate/STRIDE-corelink-dual-approval.md	7426	audit/seal artifact
specs/_audits/stride-per-crate/STRIDE-corelink-failover-router.md	7544	audit/seal artifact
specs/_audits/stride-per-crate/STRIDE-corelink-pat.md	8135	audit/seal artifact
specs/_audits/stride-per-crate/STRIDE-corelink-privacy-consent-ledger.md	7964	audit/seal artifact
specs/_audits/stride-per-crate/STRIDE-corelink-rate-limit.md	7451	audit/seal artifact
specs/_audits/stride-per-crate/STRIDE-corelink-stripe-real.md	7869	audit/seal artifact
specs/_audits/stride-per-crate/STRIDE-corelink-tenant-path.md	8205	audit/seal artifact
specs/_audits/stride-per-crate/STRIDE-tenant-path.md	7750	audit/seal artifact
specs/_audits/tla-followup-tickets.md	26069	audit/seal artifact
specs/_audits/sealed/pentest/PENTEST-EVIDENCE-PACKAGE.md	77530	pentest evidence
specs/_audits/sealed/pentest/SOW-S20-EXTERNAL-PENTEST.md	16262	pentest evidence
specs/_audits/sealed/pentest/VENDOR-ONBOARDING.md	12309	pentest evidence
specs/_audits/sealed/pentest/access-provisioning.md	7406	pentest evidence
specs/_audits/sealed/pentest/vendor-shortlist.md	7003	pentest evidence
specs/_audits/sealed/postmortems/PM-2026-05-14-S17-CHAOS-PARTITION-DRILL.md	12324	postmortem
```
</details>


<details>
<summary><b>§4.3 SPRINT-WI-SEALED — 93 files, 3343.8 KB</b></summary>

```
specs/04_sprints/S01/work_items/WI-S01-001-tenant-path-hmac.md	25900	WI in sealed sprint S01
specs/04_sprints/S01/work_items/WI-S01-002-blake3-verify-at-write.md	31494	WI in sealed sprint S01
specs/04_sprints/S01/work_items/WI-S01-003-r2-adapter-single-blob.md	32695	WI in sealed sprint S01
specs/04_sprints/S01/work_items/WI-S01-004-d1-schema-blob-meta.md	24459	WI in sealed sprint S01
specs/04_sprints/S01/work_items/WI-S01-005-reapi-batchupdateblobs.md	37062	WI in sealed sprint S01
specs/04_sprints/S01/work_items/WI-S01-006-property-tests-10k.md	33037	WI in sealed sprint S01
specs/04_sprints/S01/work_items/WI-S01-007-ci-tlc-gate-sbom.md	29143	WI in sealed sprint S01
specs/04_sprints/S03/work_items/WI-S03-001-clerk-adapter.md	34548	WI in sealed sprint S03
specs/04_sprints/S03/work_items/WI-S03-002-corelink-pat-argon2id.md	49212	WI in sealed sprint S03
specs/04_sprints/S03/work_items/WI-S03-003-tower-middleware-tenantctx.md	51061	WI in sealed sprint S03
specs/04_sprints/S03/work_items/WI-S03-004-revocation-do-kv-invalidation.md	47814	WI in sealed sprint S03
specs/04_sprints/S03/work_items/WI-S03-005-neon-schema-auth-tables.md	55190	WI in sealed sprint S03
specs/04_sprints/S03/work_items/WI-S03-006-webauthn-level3-admin.md	52900	WI in sealed sprint S03
specs/04_sprints/S03/work_items/WI-S03-007-audit-events-evt047-chain.md	45282	WI in sealed sprint S03
specs/04_sprints/S03/work_items/WI-S03-008-property-pentest-prr-ship-gate.md	42801	WI in sealed sprint S03
specs/04_sprints/S04/work_items/WI-S04-001-reapi-actioncache-handlers.md	65389	WI in sealed sprint S04
specs/04_sprints/S04/work_items/WI-S04-002-d1-ac-meta-r2-bucket.md	56669	WI in sealed sprint S04
specs/04_sprints/S04/work_items/WI-S04-003-corelink-ac-merkle-dual-side.md	59781	WI in sealed sprint S04
specs/04_sprints/S04/work_items/WI-S04-004-hkdf-digest-signing-adr-0021.md	66334	WI in sealed sprint S04
specs/04_sprints/S04/work_items/WI-S04-005-ttl-worker-cron-do-adr-0019.md	53212	WI in sealed sprint S04
specs/04_sprints/S04/work_items/WI-S04-006-reapi-conformance-prr-ship-gate.md	53790	WI in sealed sprint S04
specs/04_sprints/S05/work_items/WI-S05-001-reapi-splitblob-spliceblob-handlers.md	62288	WI in sealed sprint S05
specs/04_sprints/S05/work_items/WI-S05-002-corelink-chunker-fastcdc-adr-0022.md	47930	WI in sealed sprint S05
specs/04_sprints/S05/work_items/WI-S05-003-r2-multipart-adapter.md	30430	WI in sealed sprint S05
specs/04_sprints/S05/work_items/WI-S05-004-d1-schema-chunks-manifest-multipart-sessions.md	35051	WI in sealed sprint S05
specs/04_sprints/S05/work_items/WI-S05-005-merkle-manifest-builder-verifier.md	37760	WI in sealed sprint S05
specs/04_sprints/S05/work_items/WI-S05-006-sweeper-rb-fm-060-prr-ship-gate.md	36610	WI in sealed sprint S05
specs/04_sprints/S06/work_items/WI-S06-001-worker-gc-binary-scheduler-degrade-mode.md	37291	WI in sealed sprint S06
specs/04_sprints/S06/work_items/WI-S06-002-mark-phase-multi-pass-scan-mark-started-at.md	40549	WI in sealed sprint S06
specs/04_sprints/S06/work_items/WI-S06-003-sweep-phase-soft-delete-inv-gc-004.md	43040	WI in sealed sprint S06
specs/04_sprints/S06/work_items/WI-S06-004-physical-delete-post-grace-r2-idempotent.md	37755	WI in sealed sprint S06
specs/04_sprints/S06/work_items/WI-S06-005-refcount-reconciliation-auto-fix.md	36126	WI in sealed sprint S06
specs/04_sprints/S06/work_items/WI-S06-006-tla-ci-gate-property-test-100k-race.md	44148	WI in sealed sprint S06
specs/04_sprints/S06/work_items/WI-S06-007-dash-gc-rb-dry-runs-prr-ship-gate.md	42552	WI in sealed sprint S06
specs/04_sprints/S07/work_items/WI-S07-001-dedup-index-findmissingblobs.md	35010	WI in sealed sprint S07
specs/04_sprints/S07/work_items/WI-S07-002-eviction-worker-lru-ttl-quota-trigger.md	39187	WI in sealed sprint S07
specs/04_sprints/S07/work_items/WI-S07-003-quota-enforcement-middleware.md	37413	WI in sealed sprint S07
specs/04_sprints/S07/work_items/WI-S07-004-last-accessed-at-hot-path.md	27243	WI in sealed sprint S07
specs/04_sprints/S07/work_items/WI-S07-005-dash-dedup-alerts-prr-ship-gate.md	31503	WI in sealed sprint S07
specs/04_sprints/S09/work_items/WI-S09-001-worker-analytics-engine-red-metrics-cardinality-validator.md	49526	WI in sealed sprint S09
specs/04_sprints/S09/work_items/WI-S09-002-logpush-r2-loki-log-schema-pii-redaction.md	56480	WI in sealed sprint S09
specs/04_sprints/S09/work_items/WI-S09-003-otlp-tracing-w3c-sampling-exemplars.md	42674	WI in sealed sprint S09
specs/04_sprints/S09/work_items/WI-S09-004-cloudevents-audit-r2-hash-chain-daily-verify.md	56553	WI in sealed sprint S09
specs/04_sprints/S09/work_items/WI-S09-005-12-grafana-dashboards-as-code.md	47658	WI in sealed sprint S09
specs/04_sprints/S09/work_items/WI-S09-006-multi-burn-rate-slo-alerts-pagerduty.md	48851	WI in sealed sprint S09
specs/04_sprints/S09/work_items/WI-S09-007-synthetic-canary-3-regions-runbook-dry-run.md	52174	WI in sealed sprint S09
specs/04_sprints/S09/work_items/WI-S09-008-customer-audit-export.md	38285	WI in sealed sprint S09
specs/04_sprints/S12/work_items/WI-S12-001-slsa-l3-github-actions-rekor.md	43496	WI in sealed sprint S12
specs/04_sprints/S12/work_items/WI-S12-002-sbom-cyclonedx-dependency-track.md	38592	WI in sealed sprint S12
specs/04_sprints/S12/work_items/WI-S12-003-cosign-cf-deploy-verify-gate.md	40901	WI in sealed sprint S12
specs/04_sprints/S12/work_items/WI-S12-004-cargo-audit-cargo-deny-dependabot.md	38060	WI in sealed sprint S12
specs/04_sprints/S12/work_items/WI-S12-005-dependency-track-self-host-cve-alerts.md	38113	WI in sealed sprint S12
specs/04_sprints/S12/work_items/WI-S12-006-reproducible-builds-2-runner-diff-adr.md	35534	WI in sealed sprint S12
specs/04_sprints/S12/work_items/WI-S12-007-rb-fm-156-rb-fm-157-prr-ship-gate.md	42571	WI in sealed sprint S12
specs/04_sprints/S15/work_items/WI-S15-001-corelink-cli-7-subcommands-cross-os-signed.md	31752	WI in sealed sprint S15
specs/04_sprints/S15/work_items/WI-S15-002-bazel-integration-starter-ci-test.md	18612	WI in sealed sprint S15
specs/04_sprints/S15/work_items/WI-S15-003-buck2-integration-starter-ci-test.md	15732	WI in sealed sprint S15
specs/04_sprints/S15/work_items/WI-S15-004-ffi-wrappers-python-go-js-adr-0016.md	26542	WI in sealed sprint S15
specs/04_sprints/S15/work_items/WI-S15-005-ci-templates-3-providers-telemetry-opt-in.md	23581	WI in sealed sprint S15
specs/04_sprints/S15/work_items/WI-S15-006-fuzz-cargo-1m-apple-notarize-authenticode-2-oss-proof-conversion.md	47144	WI in sealed sprint S15
specs/04_sprints/S16/work_items/WI-S16-001-nextjs-skeleton-clerk-csp-i18n-base.md	30366	WI in sealed sprint S16
specs/04_sprints/S16/work_items/WI-S16-002-tenant-onboarding-flow-self-service-first-pat.md	26687	WI in sealed sprint S16
specs/04_sprints/S16/work_items/WI-S16-003-consent-ui-6-field-screenshot-evidence.md	29194	WI in sealed sprint S16
specs/04_sprints/S16/work_items/WI-S16-004-dsr-self-service-form-6-direitos-mfa-jwt-receipt.md	29376	WI in sealed sprint S16
specs/04_sprints/S16/work_items/WI-S16-005-admin-ops-ui-audit-viewer-dual-approval.md	27116	WI in sealed sprint S16
specs/04_sprints/S16/work_items/WI-S16-006-component-library-a11y-wcag-2-2-aa-i18n-privacy-pages.md	30086	WI in sealed sprint S16
specs/04_sprints/S16/work_items/WI-S16-007-e2e-playwright-lighthouse-ux-workshop-csp-enforce-prr.md	39847	WI in sealed sprint S16
specs/04_sprints/S17/work_items/WI-S17-001-chaos-scheduler-8-types-staging-weekly-catalog.md	27871	WI in sealed sprint S17
specs/04_sprints/S17/work_items/WI-S17-002-dr-drill-scheduler-semestral-1-cycle-cf-region-outage.md	20110	WI in sealed sprint S17
specs/04_sprints/S17/work_items/WI-S17-003-runbook-dry-run-tracker-3-p0-p1-monthly-cadence-fm-202.md	20729	WI in sealed sprint S17
specs/04_sprints/S17/work_items/WI-S17-004-incident-postmortem-templates-blameless-1-synthetic-test.md	21003	WI in sealed sprint S17
specs/04_sprints/S17/work_items/WI-S17-005-oncall-pagerduty-schedule-fadigue-tracking-dashboard.md	21226	WI in sealed sprint S17
specs/04_sprints/S17/work_items/WI-S17-006-game-day-quarterly-1-tabletop-chaos-catalog-cleanup-prr.md	34388	WI in sealed sprint S17
specs/04_sprints/S17/work_items/WI-S17-008-active-failover-drill.md	33182	WI in sealed sprint S17
specs/04_sprints/S18/work_items/WI-S18-001-docusaurus-foundation-diataxis-i18n-custom-domain.md	23462	WI in sealed sprint S18
specs/04_sprints/S18/work_items/WI-S18-002-getting-started-reapi-auto-gen-4-language-examples.md	25476	WI in sealed sprint S18
specs/04_sprints/S18/work_items/WI-S18-003-sdk-guides-python-go-js-cli-per-command.md	25587	WI in sealed sprint S18
specs/04_sprints/S18/work_items/WI-S18-004-compliance-security-pricing-pages-cross-functional-gate.md	39689	WI in sealed sprint S18
specs/04_sprints/S18/work_items/WI-S18-005-i18n-wcag-lighthouse-vale-lychee-ux-research-prr-closing.md	43040	WI in sealed sprint S18
specs/04_sprints/S19/work_items/WI-S19-001-signup-orchestration-atomic-provisioning-clerk-d1-tx-chaos-stripe-outage.md	37638	WI in sealed sprint S19
specs/04_sprints/S19/work_items/WI-S19-002-dpa-click-through-6-field-consent-jwt-receipt-3-locales-legal-review.md	38144	WI in sealed sprint S19
specs/04_sprints/S19/work_items/WI-S19-003-dpa-versioning-re-acceptance-30d-grace-degrade-read-only.md	29506	WI in sealed sprint S19
specs/04_sprints/S19/work_items/WI-S19-004-tier-selection-stripe-checkout-inv-onboard-dpa-first-d1-lock.md	31714	WI in sealed sprint S19
specs/04_sprints/S19/work_items/WI-S19-005-enterprise-inquiry-form-slack-crm-atomic-24h-auto-reply-sla.md	27614	WI in sealed sprint S19
specs/04_sprints/S19/work_items/WI-S19-006-conversion-funnel-property-tests-rb-fm-signup-failed-prr.md	39684	WI in sealed sprint S19
specs/04_sprints/S20/work_items/WI-S20-001-prr-global-orchestration-14-canonical-sources-13-signoffs.md	27504	WI in sealed sprint S20
specs/04_sprints/S20/work_items/WI-S20-002-external-pentest-2week-1week-retest-zero-high-critical.md	25260	WI in sealed sprint S20
specs/04_sprints/S20/work_items/WI-S20-003-soc2-drata-vanta-gap-analysis-roadmap-type1-6m.md	20053	WI in sealed sprint S20
specs/04_sprints/S20/work_items/WI-S20-004-3-lighthouse-customers-2-team-1-enterprise-byok-30d-sla-attestations.md	24375	WI in sealed sprint S20
specs/04_sprints/S20/work_items/WI-S20-005-sla-v1-published-dpa-v1-signed-3-lighthouse-legal-review.md	21683	WI in sealed sprint S20
specs/04_sprints/S20/work_items/WI-S20-006-incident-response-24-7-pagerduty-3-regions-synthetic-page-weekly.md	19794	WI in sealed sprint S20
specs/04_sprints/S20/work_items/WI-S20-007-30d-staging-tla-4-runbooks-90d-sbom-closing-prr.md	25813	WI in sealed sprint S20
specs/04_sprints/S20/work_items/WI-S20-008-launch-orchestration-prep-press-release-blogs-case-studies-product-hunt.md	24362	WI in sealed sprint S20
```
</details>


<details>
<summary><b>§4.4 SPRINT-CONTRACT-SEALED — 56 files, 1343.3 KB</b></summary>

```
specs/04_sprints/S01/_spec_contract.md	21222	sprint artifact in sealed S01 (_spec_contract.md)
specs/04_sprints/S01/sprint.md	15324	sprint artifact in sealed S01 (sprint.md)
specs/04_sprints/S03/PRR-S03.md	22051	sprint artifact in sealed S03 (PRR-S03.md)
specs/04_sprints/S03/_spec_contract.md	25154	sprint artifact in sealed S03 (_spec_contract.md)
specs/04_sprints/S03/asvs-v2-v3-v4-v6-v8-checklist.md	11875	sprint artifact in sealed S03 (asvs-v2-v3-v4-v6-v8-checklist.md)
specs/04_sprints/S03/sprint.md	11201	sprint artifact in sealed S03 (sprint.md)
specs/04_sprints/S04/PRR-S04.md	29378	sprint artifact in sealed S04 (PRR-S04.md)
specs/04_sprints/S04/_spec_contract.md	40668	sprint artifact in sealed S04 (_spec_contract.md)
specs/04_sprints/S04/asvs-v5-v6-v8-v10-v14-checklist.md	16602	sprint artifact in sealed S04 (asvs-v5-v6-v8-v10-v14-checklist.md)
specs/04_sprints/S04/sprint.md	7403	sprint artifact in sealed S04 (sprint.md)
specs/04_sprints/S05/PRR-S05.md	35238	sprint artifact in sealed S05 (PRR-S05.md)
specs/04_sprints/S05/_spec_contract.md	37235	sprint artifact in sealed S05 (_spec_contract.md)
specs/04_sprints/S05/asvs-v5-v6-v8-v10-v14-checklist.md	15329	sprint artifact in sealed S05 (asvs-v5-v6-v8-v10-v14-checklist.md)
specs/04_sprints/S05/sprint.md	7343	sprint artifact in sealed S05 (sprint.md)
specs/04_sprints/S06/PRR-S06.md	54029	sprint artifact in sealed S06 (PRR-S06.md)
specs/04_sprints/S06/_review_R4_opus_part1.md	18875	sprint artifact in sealed S06 (_review_R4_opus_part1.md)
specs/04_sprints/S06/_review_R4_opus_part2.md	18572	sprint artifact in sealed S06 (_review_R4_opus_part2.md)
specs/04_sprints/S06/_review_R5_sonnet_part1.md	17298	sprint artifact in sealed S06 (_review_R5_sonnet_part1.md)
specs/04_sprints/S06/_review_R5_sonnet_part2.md	16926	sprint artifact in sealed S06 (_review_R5_sonnet_part2.md)
specs/04_sprints/S06/_spec_contract.md	57806	sprint artifact in sealed S06 (_spec_contract.md)
specs/04_sprints/S06/asvs-v5-v6-v8-v10-v14-checklist.md	13661	sprint artifact in sealed S06 (asvs-v5-v6-v8-v10-v14-checklist.md)
specs/04_sprints/S06/sprint.md	8603	sprint artifact in sealed S06 (sprint.md)
specs/04_sprints/S07/PRR-S07.md	29639	sprint artifact in sealed S07 (PRR-S07.md)
specs/04_sprints/S07/_spec_contract.md	36026	sprint artifact in sealed S07 (_spec_contract.md)
specs/04_sprints/S09/PRR-S09.md	47415	sprint artifact in sealed S09 (PRR-S09.md)
specs/04_sprints/S09/_review_R4_opus_part1.md	22970	sprint artifact in sealed S09 (_review_R4_opus_part1.md)
specs/04_sprints/S09/_review_R4_opus_part2.md	21210	sprint artifact in sealed S09 (_review_R4_opus_part2.md)
specs/04_sprints/S09/_review_R5_sonnet_part1.md	19289	sprint artifact in sealed S09 (_review_R5_sonnet_part1.md)
specs/04_sprints/S09/_review_R5_sonnet_part2.md	19126	sprint artifact in sealed S09 (_review_R5_sonnet_part2.md)
specs/04_sprints/S09/_review_R5_sonnet_round_2.md	17499	sprint artifact in sealed S09 (_review_R5_sonnet_round_2.md)
specs/04_sprints/S09/_spec_contract.md	70352	sprint artifact in sealed S09 (_spec_contract.md)
specs/04_sprints/S12/PRR-S12.md	30286	sprint artifact in sealed S12 (PRR-S12.md)
specs/04_sprints/S12/_spec_contract.md	23254	sprint artifact in sealed S12 (_spec_contract.md)
specs/04_sprints/S12/asvs-v14-v11.1-ssdf-eo14028-checklist.md	8775	sprint artifact in sealed S12 (asvs-v14-v11.1-ssdf-eo14028-checklist.md)
specs/04_sprints/S12/sprint.md	21939	sprint artifact in sealed S12 (sprint.md)
specs/04_sprints/S15/PRR-S15.md	13669	sprint artifact in sealed S15 (PRR-S15.md)
specs/04_sprints/S15/_spec_contract.md	19987	sprint artifact in sealed S15 (_spec_contract.md)
specs/04_sprints/S15/sprint.md	32283	sprint artifact in sealed S15 (sprint.md)
specs/04_sprints/S16/PRR-S16.md	10620	sprint artifact in sealed S16 (PRR-S16.md)
specs/04_sprints/S16/_spec_contract.md	21938	sprint artifact in sealed S16 (_spec_contract.md)
specs/04_sprints/S16/sprint.md	43009	sprint artifact in sealed S16 (sprint.md)
specs/04_sprints/S17/PRR-S17.md	14346	sprint artifact in sealed S17 (PRR-S17.md)
specs/04_sprints/S17/_spec_contract.md	22143	sprint artifact in sealed S17 (_spec_contract.md)
specs/04_sprints/S17/dr_drill_cadence.md	3832	sprint artifact in sealed S17 (dr_drill_cadence.md)
specs/04_sprints/S17/sprint.md	43406	sprint artifact in sealed S17 (sprint.md)
specs/04_sprints/S18/PRR-S18.md	12029	sprint artifact in sealed S18 (PRR-S18.md)
specs/04_sprints/S18/_spec_contract.md	20984	sprint artifact in sealed S18 (_spec_contract.md)
specs/04_sprints/S18/sprint.md	36418	sprint artifact in sealed S18 (sprint.md)
specs/04_sprints/S19/PRR-S19.md	17595	sprint artifact in sealed S19 (PRR-S19.md)
specs/04_sprints/S19/_spec_contract.md	22375	sprint artifact in sealed S19 (_spec_contract.md)
specs/04_sprints/S19/sprint.md	38442	sprint artifact in sealed S19 (sprint.md)
specs/04_sprints/S20/PRR-S20-CLOSING.md	13334	sprint artifact in sealed S20 (PRR-S20-CLOSING.md)
specs/04_sprints/S20/PRR-S20-GA.md	31773	sprint artifact in sealed S20 (PRR-S20-GA.md)
specs/04_sprints/S20/_spec_contract.md	29987	sprint artifact in sealed S20 (_spec_contract.md)
specs/04_sprints/S20/pentest_gate.md	5358	sprint artifact in sealed S20 (pentest_gate.md)
specs/04_sprints/S20/sprint.md	52485	sprint artifact in sealed S20 (sprint.md)
```
</details>


### §4.5 SUPERSEDED (empty)


<details>
<summary><b>§4.6 DEFASADO-CODE-DRIFT — 27 files, 712.5 KB</b></summary>

```
specs/03_architecture/d1-schema-evolution.md	9831	references absorbed crate without historical marker (was CHARTER)
specs/03_architecture/tenant-offboarding-spec.md	16627	references absorbed crate without historical marker (was CHARTER)
specs/04_sprints/S08/work_items/WI-S08-003-quota-checker-middleware-atomic-cas.md	69990	references absorbed crate without historical marker (was REFERENCE)
specs/04_sprints/S08/work_items/WI-S08-004-abuse-detection-heuristica-scoring.md	62443	references absorbed crate without historical marker (was REFERENCE)
specs/04_sprints/S10/work_items/WI-S10-006-replay-forensic-endpoint-role-audit-trail.md	74869	references absorbed crate without historical marker (was REFERENCE)
specs/04_sprints/S11/work_items/WI-S11-003-consent-ledger-d1-proof-of-informed-symmetric-revoke.md	57395	references absorbed crate without historical marker (was REFERENCE)
specs/04_sprints/S11/work_items/WI-S11-007-residency-e2e-custom-domain-routing-property-test-20k.md	51452	references absorbed crate without historical marker (was REFERENCE)
specs/04_sprints/S13/PRR-S13.md	25756	references absorbed crate without historical marker (was REFERENCE)
specs/04_sprints/S13/work_items/WI-S13-003-secret-rotation-tdk-pat-audit-chain-byok.md	52238	references absorbed crate without historical marker (was REFERENCE)
specs/04_sprints/S13/work_items/WI-S13-005-progressive-rollout-error-budget-auto-rollback.md	44866	references absorbed crate without historical marker (was REFERENCE)
specs/04_sprints/S14/work_items/WI-S14-005-byok-gcp-azure-vault-16-combination-matrix-test.md	38946	references absorbed crate without historical marker (was REFERENCE)
specs/04_sprints/S14/work_items/WI-S14-006-cmk-revocation-kill-switch-5min-chaos-drill.md	45255	references absorbed crate without historical marker (was REFERENCE)
specs/05_quality/runbooks/RB-FM-AC-MIGRATION-BUG.md	5495	references absorbed crate without historical marker (was REFERENCE)
specs/_compliance/BCP-DR-DRILL-CADENCE.md	27467	references absorbed crate without historical marker (was REFERENCE)
specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md	9351	references absorbed crate without historical marker (was REFERENCE)
specs/_compliance/DPO-RESPONSIBILITIES-MATRIX.md	15658	references absorbed crate without historical marker (was REFERENCE)
specs/_compliance/DRATA-INTEGRATION-COVERAGE.md	11183	references absorbed crate without historical marker (was REFERENCE)
specs/_compliance/LGPD-RESIDENCY-ATTESTATION-2026-05-15.md	23152	references absorbed crate without historical marker (was REFERENCE)
specs/_compliance/fips-attestation-letters/LETTER-AWS-KMS.md	4795	references absorbed crate without historical marker (was REFERENCE)
specs/_compliance/fips-attestation-letters/LETTER-AZURE-KV.md	5211	references absorbed crate without historical marker (was REFERENCE)
specs/_compliance/fips-attestation-letters/LETTER-GCP-KMS.md	4845	references absorbed crate without historical marker (was REFERENCE)
specs/_compliance/fips-attestation-letters/LETTER-VAULT.md	5515	references absorbed crate without historical marker (was REFERENCE)
specs/_lighthouse/lighthouse-customer-program.md	9204	references absorbed crate without historical marker (was REFERENCE)
specs/_runbooks/ONCALL-ESCALATION-MATRIX.md	14042	references absorbed crate without historical marker (was REFERENCE)
specs/_runbooks/RB-LIGHTHOUSE-PHASE-MANAGEMENT.md	20305	references absorbed crate without historical marker (was REFERENCE)
specs/_runbooks/RB-ONCALL-POLICY.md	12738	references absorbed crate without historical marker (was REFERENCE)
specs/_runbooks/RB-TENANT-OFFBOARDING.md	11000	references absorbed crate without historical marker (was REFERENCE)
```
</details>


<details>
<summary><b>§4.7 DEFASADO-NOT-LANDED — 8 files, 133.8 KB</b></summary>

```
specs/_proposals/2026-05-16-framework-reviewer-roles-addendum.md	45094	proposal doc (pre-implementation)
specs/_proposals/2026-05-16-framework-reviewer-roles.md	24365	proposal doc (pre-implementation)
specs/_proposals/adapters/README.md	7885	proposal doc (pre-implementation)
specs/_proposals/adapters/brew.md	8532	proposal doc (pre-implementation)
specs/_proposals/adapters/cargo.md	13952	proposal doc (pre-implementation)
specs/_proposals/adapters/npm.md	14217	proposal doc (pre-implementation)
specs/_proposals/adapters/oci.md	14165	proposal doc (pre-implementation)
specs/_proposals/adapters/pip.md	8764	proposal doc (pre-implementation)
```
</details>


<details>
<summary><b>§4.8 DUPLICATE — 4 files, 36.9 KB</b></summary>

```
specs/05_quality/runbooks/RB-BYOK-REVOKE.md	14584	same basename, different content (cross-dir copy)
specs/05_quality/runbooks/RB-FM-SIGNUP-FAILED.md	6481	same basename, different content (cross-dir copy)
specs/05_runbooks/RB-BYOK-REVOKE.md	6680	same basename, different content (cross-dir copy)
specs/_runbooks/RB-FM-SIGNUP-FAILED.md	10090	same basename, different content (cross-dir copy)
```
</details>


<details>
<summary><b>§4.9 SCAFFOLD — 15 files, 221.4 KB</b></summary>

```
specs/_audits/lia-template.md	3911	template file
specs/_audits/templates/byok-quarterly-review.md	4012	in templates/ subdir
specs/_compliance/templates/DR-DRILL-EVIDENCE.md	10019	in templates/ subdir
specs/_compliance/templates/IR-TABLETOP-EVIDENCE.md	8126	in templates/ subdir
specs/_lighthouse/sla-attestation-template.md	6043	template file
specs/_pentest/findings-template.md	4262	template file
specs/_templates/adr.md	16533	in _templates/
specs/_templates/dpia.md	8260	in _templates/
specs/_templates/lia.md	7408	in _templates/
specs/_templates/oncall_handoff.md	2648	in _templates/
specs/_templates/production_readiness_review.md	27896	in _templates/
specs/_templates/sprint_contract.md	40349	in _templates/
specs/_templates/subtask.md	16694	in _templates/
specs/_templates/waiver.md	12289	in _templates/
specs/_templates/work_item.md	58255	in _templates/
```
</details>


<details>
<summary><b>§4.10 REFERENCE — 244 files, 4224.9 KB</b></summary>

```
specs/04_sprints/S00/_spec_contract.md	8158	sprint artifact in active S00 (_spec_contract.md)
specs/04_sprints/S00/sprint.md	7230	sprint artifact in active S00 (sprint.md)
specs/04_sprints/S02/PRR-S02.md	16892	sprint artifact in active S02 (PRR-S02.md)
specs/04_sprints/S02/_spec_contract.md	49700	sprint artifact in active S02 (_spec_contract.md)
specs/04_sprints/S02/sprint.md	23617	sprint artifact in active S02 (sprint.md)
specs/04_sprints/S02/work_items/WI-S02-001-bytestream-read.md	35655	WI in active sprint S02
specs/04_sprints/S02/work_items/WI-S02-002-getblob-findmissing.md	35510	WI in active sprint S02
specs/04_sprints/S02/work_items/WI-S02-003-corelink-client-verify-crate.md	31926	WI in active sprint S02
specs/04_sprints/S02/work_items/WI-S02-004-constant-time-middleware.md	41009	WI in active sprint S02
specs/04_sprints/S02/work_items/WI-S02-005-negative-cache-kv.md	32295	WI in active sprint S02
specs/04_sprints/S02/work_items/WI-S02-006-property-tests-rb-prr.md	25116	WI in active sprint S02
specs/04_sprints/S08/PRR-S08.md	42896	sprint artifact in active S08 (PRR-S08.md)
specs/04_sprints/S08/_spec_contract.md	36472	sprint artifact in active S08 (_spec_contract.md)
specs/04_sprints/S08/work_items/WI-S08-001-do-ratelimiter-token-bucket.md	38062	WI in active sprint S08
specs/04_sprints/S08/work_items/WI-S08-002-cf-edge-per-ip-rules-cidr-blocklist.md	51683	WI in active sprint S08
specs/04_sprints/S08/work_items/WI-S08-005-rfc9331-headers-global-circuit-breaker.md	53550	WI in active sprint S08
specs/04_sprints/S08/work_items/WI-S08-006-dash-rate-alerts-prr-ship-gate.md	56525	WI in active sprint S08
specs/04_sprints/S10/PRR-S10.md	60849	sprint artifact in active S10 (PRR-S10.md)
specs/04_sprints/S10/_spec_contract.md	56669	sprint artifact in active S10 (_spec_contract.md)
specs/04_sprints/S10/finance-walkthrough.md	19417	sprint artifact in active S10 (finance-walkthrough.md)
specs/04_sprints/S10/work_items/WI-S10-001-usage-event-emitter-r2-append-only-idempotency.md	61133	WI in active sprint S10
specs/04_sprints/S10/work_items/WI-S10-002-counter-aggregator-cron-do-hash-chain.md	73133	WI in active sprint S10
specs/04_sprints/S10/work_items/WI-S10-003-corelink-billing-stripe-adapter-idempotency-webhook.md	82071	WI in active sprint S10
specs/04_sprints/S10/work_items/WI-S10-004-reconciliation-worker-3-layer-drift-alerts.md	73695	WI in active sprint S10
specs/04_sprints/S10/work_items/WI-S10-005-quota-state-machine-overage-email.md	77771	WI in active sprint S10
specs/04_sprints/S10/work_items/WI-S10-007-tla-billing-atomicity-runbooks-finance-walkthrough.md	74801	WI in active sprint S10
specs/04_sprints/S11/_review_R5_sonnet_round_2.md	37745	sprint artifact in active S11 (_review_R5_sonnet_round_2.md)
specs/04_sprints/S11/_spec_contract.md	83923	sprint artifact in active S11 (_spec_contract.md)
specs/04_sprints/S11/work_items/WI-S11-001-dsr-api-7-endpoints-jwt-receipt-mfa-step-up.md	60836	WI in active sprint S11
specs/04_sprints/S11/work_items/WI-S11-002-erasure-worker-10-backends-verification-24h-reports.md	82051	WI in active sprint S11
specs/04_sprints/S11/work_items/WI-S11-004-privacy-notice-versioning-3-locales-diff-publication.md	44049	WI in active sprint S11
specs/04_sprints/S11/work_items/WI-S11-005-sub-processor-register-30d-broadcast-objection-flow.md	50473	WI in active sprint S11
specs/04_sprints/S11/work_items/WI-S11-006-breach-notification-runbook-3-jurisdictional-templates-dry-run.md	48679	WI in active sprint S11
specs/04_sprints/S11/work_items/WI-S11-008-dpia-lia-3-filled-tla-dsr-erasure-atomicity.md	74162	WI in active sprint S11
specs/04_sprints/S13/RELEASE_NOTES.md	6365	sprint artifact in active S13 (RELEASE_NOTES.md)
specs/04_sprints/S13/_spec_contract.md	21396	sprint artifact in active S13 (_spec_contract.md)
specs/04_sprints/S13/asvs-v4-v5-v6-v7-v14-ssdf-nist-checklist.md	11545	sprint artifact in active S13 (asvs-v4-v5-v6-v7-v14-ssdf-nist-checklist.md)
specs/04_sprints/S13/sprint.md	28449	sprint artifact in active S13 (sprint.md)
specs/04_sprints/S13/work_items/WI-S13-001-do-config-singleton-flag-crud-propagation-rollback.md	42802	WI in active sprint S13
specs/04_sprints/S13/work_items/WI-S13-002-admin-api-dual-approval-collusion-rotation.md	49456	WI in active sprint S13
specs/04_sprints/S13/work_items/WI-S13-004-terraform-drift-detection-daily-rb-fm-206.md	35402	WI in active sprint S13
specs/04_sprints/S13/work_items/WI-S13-006-property-tests-mfa-freshness-rotation-overlap-rb-fm-205-prr.md	53454	WI in active sprint S13
specs/04_sprints/S14/PRR-S14.md	39404	sprint artifact in active S14 (PRR-S14.md)
specs/04_sprints/S14/_spec_contract.md	24483	sprint artifact in active S14 (_spec_contract.md)
specs/04_sprints/S14/sprint.md	43544	sprint artifact in active S14 (sprint.md)
specs/04_sprints/S14/work_items/WI-S14-001-r2-d1-do-provisioning-4-regions-terraform.md	41055	WI in active sprint S14
specs/04_sprints/S14/work_items/WI-S14-002-tenant-region-pinning-30k-property-test.md	39440	WI in active sprint S14
specs/04_sprints/S14/work_items/WI-S14-003-hot-blob-replica-pat-region-failover-001.md	44459	WI in active sprint S14
specs/04_sprints/S14/work_items/WI-S14-004-byok-trait-aws-kms-envelope-encryption-matrix-framework.md	55857	WI in active sprint S14
specs/04_sprints/S14/work_items/WI-S14-007-erasure-attestation-ed25519-7y-retention-verify.md	49789	WI in active sprint S14
specs/04_sprints/S14/work_items/WI-S14-008-dpa-amendment-schrems-ii-tia-legal-review.md	44087	WI in active sprint S14
specs/04_sprints/S14/work_items/WI-S14-009-tla-region-residency-rb-byok-revoke-pentest-prr.md	39662	WI in active sprint S14
specs/04_sprints/_sprint_creation_contract.md	14442	uncategorized fallback
specs/04_sprints/timing-gates-cumulative.md	7948	uncategorized fallback
specs/05_quality/runbooks/INDEX.md	10058	runbook / quality doc
specs/05_quality/runbooks/RB-ADMIN-CONFIG-STALE.md	7127	runbook / quality doc
specs/05_quality/runbooks/RB-ADMIN-DUAL-APPROVAL-BREACH.md	6947	runbook / quality doc
specs/05_quality/runbooks/RB-ADMIN-ROTATION-GAP.md	7515	runbook / quality doc
specs/05_quality/runbooks/RB-BILLING-001.md	5128	runbook / quality doc
specs/05_quality/runbooks/RB-BILLING-002.md	4890	runbook / quality doc
specs/05_quality/runbooks/RB-BREACH-NOTIF.md	19830	runbook / quality doc
specs/05_quality/runbooks/RB-CONSENT-TAMPERING.md	3343	runbook / quality doc
specs/05_quality/runbooks/RB-DATA-RESIDENCY-LEAK.md	8573	runbook / quality doc
specs/05_quality/runbooks/RB-DR-DRILL.md	7621	runbook / quality doc
specs/05_quality/runbooks/RB-DSR-ERASURE-INCOMPLETE.md	9021	runbook / quality doc
specs/05_quality/runbooks/RB-DSR-INTAKE-FAILURE.md	2620	runbook / quality doc
specs/05_quality/runbooks/RB-FM-007-deserialize-rce.md	2444	runbook / quality doc
specs/05_quality/runbooks/RB-FM-051-r2-bit-rot.md	1841	runbook / quality doc
specs/05_quality/runbooks/RB-FM-054-kv-stale.md	1228	runbook / quality doc
specs/05_quality/runbooks/RB-FM-057-neon-failover.md	1874	runbook / quality doc
specs/05_quality/runbooks/RB-FM-059-do-quota-exceeded.md	3155	runbook / quality doc
specs/05_quality/runbooks/RB-FM-060-multipart-orphan.md	1578	runbook / quality doc
specs/05_quality/runbooks/RB-FM-062-hash-collision.md	3232	runbook / quality doc
specs/05_quality/runbooks/RB-FM-100-dns-outage.md	2323	runbook / quality doc
specs/05_quality/runbooks/RB-FM-101-cf-edge-outage.md	2764	runbook / quality doc
specs/05_quality/runbooks/RB-FM-105-region-replication-diverge.md	5900	runbook / quality doc
specs/05_quality/runbooks/RB-FM-151-stripe-outage.md	1580	runbook / quality doc
specs/05_quality/runbooks/RB-FM-153-grafana-cloud-outage.md	3116	runbook / quality doc
specs/05_quality/runbooks/RB-FM-156-dep-maintainer-malicioso.md	12649	runbook / quality doc
specs/05_quality/runbooks/RB-FM-156-dep-malicious.md	2957	runbook / quality doc
specs/05_quality/runbooks/RB-FM-157-typosquat.md	15655	runbook / quality doc
specs/05_quality/runbooks/RB-FM-160-auth-invalid-storm.md	3050	runbook / quality doc
specs/05_quality/runbooks/RB-FM-201-config-change-ratelimit-drop.md	2533	runbook / quality doc
specs/05_quality/runbooks/RB-FM-202-runbook-stale.md	3389	runbook / quality doc
specs/05_quality/runbooks/RB-FM-205-admin-mistake.md	2096	runbook / quality doc
specs/05_quality/runbooks/RB-FM-206-terraform-drift.md	12414	runbook / quality doc
specs/05_quality/runbooks/RB-FM-250-ddos-volumetric.md	3447	runbook / quality doc
specs/05_quality/runbooks/RB-FM-253-cross-tenant-read.md	3520	runbook / quality doc
specs/05_quality/runbooks/RB-FM-254-cache-poisoning.md	3038	runbook / quality doc
specs/05_quality/runbooks/RB-FM-258-insider-exfil.md	3160	runbook / quality doc
specs/05_quality/runbooks/RB-FM-300-gc-refcount-bug.md	3575	runbook / quality doc
specs/05_quality/runbooks/RB-FM-302-billing-leak.md	3093	runbook / quality doc
specs/05_quality/runbooks/RB-FM-303-ac-cross-tenant.md	4893	runbook / quality doc
specs/05_quality/runbooks/RB-FM-305-tombstone-lost.md	3604	runbook / quality doc
specs/05_quality/runbooks/RB-FM-400-retry-storm.md	1503	runbook / quality doc
specs/05_quality/runbooks/RB-FM-403-container-leak.md	1282	runbook / quality doc
specs/05_quality/runbooks/RB-FM-404-gc-write-race.md	2476	runbook / quality doc
specs/05_quality/runbooks/RB-FM-AC-BUCKET-LEAK.md	5732	runbook / quality doc
specs/05_quality/runbooks/RB-FM-AC-TTL-DRIFT.md	4670	runbook / quality doc
specs/05_quality/runbooks/RB-FM-AC-TTL-STORM.md	3766	runbook / quality doc
specs/05_quality/runbooks/RB-GDPR-ERASURE-HOLD.md	3445	runbook / quality doc
specs/05_quality/runbooks/RB-HSM-UNAVAILABLE.md	2286	runbook / quality doc
specs/05_quality/runbooks/RB-KEY-COMPROMISE.md	3455	runbook / quality doc
specs/05_quality/runbooks/RB-OBS-CARDINALITY-001.md	3544	runbook / quality doc
specs/05_quality/runbooks/RB-PRIVACY-NOTICE-LATE-PUBLICATION.md	2756	runbook / quality doc
specs/05_quality/runbooks/RB-REGION-FAILOVER.md	7795	runbook / quality doc
specs/05_quality/runbooks/RB-ROLLOUT-STUCK.md	6339	runbook / quality doc
specs/05_quality/runbooks/RB-SLO-AVAIL-CP.md	2494	runbook / quality doc
specs/05_quality/runbooks/RB-SLO-AVAIL-DATA-PLANE.md	8323	runbook / quality doc
specs/05_quality/runbooks/RB-SLO-CORRECT-VIOLATION.md	9266	runbook / quality doc
specs/05_quality/runbooks/RB-SLO-DEDUP-DEGRADATION.md	7114	runbook / quality doc
specs/05_quality/runbooks/RB-SLO-LATENCY-INVESTIGATION.md	6610	runbook / quality doc
specs/05_quality/runbooks/RB-SUB-PROCESSOR-BROADCAST-MISS.md	5116	runbook / quality doc
specs/05_quality/runbooks/RB-SUPPLY-REKOR-OUTAGE.md	5223	runbook / quality doc
specs/05_quality/runbooks/RB-TLA-COUNTEREXAMPLE.md	2860	runbook / quality doc
specs/05_quality/runbooks/RB-region-leak.md	7133	runbook / quality doc
specs/05_runbooks/RB-RUNBOOK-DRILL-INDEX.md	8805	runbook / quality doc
specs/05_runbooks/RB-region.md	10672	runbook / quality doc
specs/_compliance/ACTIVE-FAILOVER-DRILL-SPEC.md	17398	compliance artifact
specs/_compliance/AUDITOR-WALKTHROUGH-SCRIPT.md	18122	compliance artifact
specs/_compliance/COLD-RESTORE-DRILL-SPEC.md	17239	compliance artifact
specs/_compliance/DPO-APPOINTMENT-2026-05-15.md	18226	compliance artifact
specs/_compliance/DPO-HANDOFF-PLAN.md	16928	compliance artifact
specs/_compliance/FEDRAMP-MODERATE-CROSSWALK-2026-05-15.md	28934	compliance artifact
specs/_compliance/FEDRAMP-NOT-IN-SCOPE-RATIONALE.md	14933	compliance artifact
specs/_compliance/FIPS-RFI-QUESTIONNAIRE.md	7279	compliance artifact
specs/_compliance/GA-GATE-CRITERIA.md	29048	compliance artifact
specs/_compliance/GA-GATE-GO-NOGO-TEMPLATE.md	15879	compliance artifact
specs/_compliance/GDPR-DPIA-LIBRARY.md	14538	compliance artifact
specs/_compliance/GDPR-FULL-AUDIT-2026-05-15.md	44395	compliance artifact
specs/_compliance/GDPR-SCC-EXECUTION-2026-05-15.md	18455	compliance artifact
specs/_compliance/IR-TABLETOP-PLAYBOOK.md	13949	compliance artifact
specs/_compliance/IR-TABLETOP-SCHEDULE-2026.md	7490	compliance artifact
specs/_compliance/ISO27001-CROSSWALK-2026-05-15.md	29164	compliance artifact
specs/_compliance/ISO27001-GAP-ANALYSIS.md	15126	compliance artifact
specs/_compliance/ISO27001-INTERNAL-AUDIT-PROGRAM.md	13070	compliance artifact
specs/_compliance/ISO27001-MANAGEMENT-REVIEW-TEMPLATE.md	11307	compliance artifact
specs/_compliance/ISO27001-ROADMAP.md	16864	compliance artifact
specs/_compliance/ISO27001-STATEMENT-OF-APPLICABILITY-2026-05-15.md	28342	compliance artifact
specs/_compliance/LGPD-DPO-MONTHLY-CHECKLIST.md	7071	compliance artifact
specs/_compliance/LGPD-FULL-AUDIT-2026-05-15.md	37324	compliance artifact
specs/_compliance/LGPD-ROPA-2026-05-15.md	18472	compliance artifact
specs/_compliance/PCI-DSS-ANNUAL-RECERTIFY.md	10579	compliance artifact
specs/_compliance/PCI-DSS-BOUNDARY-DIAGRAM.md	18749	compliance artifact
specs/_compliance/PCI-DSS-SAQ-A-2026-05-15.md	26558	compliance artifact
specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md	35174	compliance artifact
specs/_compliance/SOC2-GAP-ANALYSIS.md	31787	compliance artifact
specs/_compliance/SOC2-ROADMAP.md	10955	compliance artifact
specs/_compliance/VENDOR-RISK-METHODOLOGY.md	9904	compliance artifact
specs/_compliance/VENDOR-RISK-REGISTER.md	14202	compliance artifact
specs/_compliance/aws-artifact-placeholder.md	13541	compliance artifact
specs/_compliance/drill-evidence/2026-Q3-cold-restore-dry-run.md	10356	compliance artifact
specs/_compliance/ir-scenarios/TT-01-data-breach.md	15556	compliance artifact
specs/_compliance/ir-scenarios/TT-02-cascading-failure.md	13975	compliance artifact
specs/_compliance/ir-scenarios/TT-03-webhook-compromise.md	13489	compliance artifact
specs/_compliance/ir-scenarios/TT-04-insider-threat.md	17424	compliance artifact
specs/_compliance/ir-scenarios/TT-05-supply-chain.md	16684	compliance artifact
specs/_compliance/ir-scenarios/TT-06-ddos-abuse.md	14367	compliance artifact
specs/_compliance/vendor-dd/DD-AWS-KMS.md	8035	compliance artifact
specs/_compliance/vendor-dd/DD-AZURE-KEYVAULT.md	9065	compliance artifact
specs/_compliance/vendor-dd/DD-CLERK.md	6404	compliance artifact
specs/_compliance/vendor-dd/DD-CLOUDFLARE.md	8959	compliance artifact
specs/_compliance/vendor-dd/DD-DRATA.md	7130	compliance artifact
specs/_compliance/vendor-dd/DD-GCP-KMS.md	8799	compliance artifact
specs/_compliance/vendor-dd/DD-HASHICORP-VAULT.md	10213	compliance artifact
specs/_compliance/vendor-dd/DD-HUBSPOT.md	8447	compliance artifact
specs/_compliance/vendor-dd/DD-PAGERDUTY.md	8964	compliance artifact
specs/_compliance/vendor-dd/DD-SLACK.md	10811	compliance artifact
specs/_compliance/vendor-dd/DD-STRIPE.md	7211	compliance artifact
specs/_compliance/vendor-shortlist-soc2.md	5503	compliance artifact
specs/_compliance/weekly-digests/2026-05-15.md	3035	compliance artifact
specs/_compliance/weekly-digests/README.md	7499	compliance artifact
specs/_dashboards/DASH-AUDIT-CHAIN.md	5006	dashboard spec
specs/_dashboards/DASH-BILLING.md	5353	dashboard spec
specs/_dashboards/DASH-BYOK-HEALTH.md	5289	dashboard spec
specs/_dashboards/DASH-CAPACITY-PLANNING.md	6267	dashboard spec
specs/_dashboards/DASH-COMPLIANCE-HEALTH.md	6208	dashboard spec
specs/_dashboards/DASH-CUSTOMER-TRAFFIC.md	5607	dashboard spec
specs/_dashboards/DASH-DR-STATUS.md	6051	dashboard spec
specs/_dashboards/DASH-DSR-PIPELINE.md	5557	dashboard spec
specs/_dashboards/DASH-INCIDENT-TRIAGE.md	6025	dashboard spec
specs/_dashboards/DASH-RATELIMIT-ABUSE.md	5209	dashboard spec
specs/_dashboards/DASH-RELIABILITY.md	5153	dashboard spec
specs/_dashboards/DASH-SLO-BURNDOWN.md	11840	dashboard spec
specs/_dashboards/INDEX.md	8162	dashboard spec
specs/_governance/reviewer_staffing_strategy.md	9545	governance/legal/security doc
specs/_legal/lighthouse-legal-review-tracker.md	7690	governance/legal/security doc
specs/_lighthouse/case-study-templates/enterprise-byok.md	7160	governance/legal/security doc
specs/_lighthouse/case-study-templates/team-tier-1-forge.md	4406	governance/legal/security doc
specs/_lighthouse/case-study-templates/team-tier-2-oss.md	4872	governance/legal/security doc
specs/_lighthouse/recruitment-shortlist.md	7061	governance/legal/security doc
specs/_runbooks/RB-24H-ENDURANCE-LOAD.md	11414	runbook / quality doc
specs/_runbooks/RB-ACTIVE-FAILOVER.md	19048	runbook / quality doc
specs/_runbooks/RB-AUDIT-EXPORT-CROSS-TENANT-ATTEMPT.md	13962	runbook / quality doc
specs/_runbooks/RB-AUDIT-EXPORT-INTEGRITY.md	9223	runbook / quality doc
specs/_runbooks/RB-AUDIT-EXPORT-VERIFY-FAILED.md	18176	runbook / quality doc
specs/_runbooks/RB-BACKUP-VERIFICATION-FAILURE.md	7747	runbook / quality doc
specs/_runbooks/RB-BACKUP-VERIFICATION.md	4754	runbook / quality doc
specs/_runbooks/RB-CANONICAL-DRIFT.md	7199	runbook / quality doc
specs/_runbooks/RB-CHAOS-CAMPAIGN.md	9954	runbook / quality doc
specs/_runbooks/RB-CHAOS-CATALOG.md	17244	runbook / quality doc
specs/_runbooks/RB-CHURN-RISK-RESPONSE.md	13836	runbook / quality doc
specs/_runbooks/RB-COLD-RESTORE-FROM-ZERO.md	22080	runbook / quality doc
specs/_runbooks/RB-COMPLIANCE-WEEKLY-REVIEW.md	14545	runbook / quality doc
specs/_runbooks/RB-CUSTOMER-SUPPORT-T-90.md	22874	runbook / quality doc
specs/_runbooks/RB-D1-MIGRATION-APPLY.md	7712	runbook / quality doc
specs/_runbooks/RB-DEPENDABOT-INCIDENT.md	12091	runbook / quality doc
specs/_runbooks/RB-DPA-CHANGE.md	4746	runbook / quality doc
specs/_runbooks/RB-DPO-ESCALATION.md	19097	runbook / quality doc
specs/_runbooks/RB-DRATA-SYNC-FAILURE.md	9458	runbook / quality doc
specs/_runbooks/RB-DSR-GDPR.md	21634	runbook / quality doc
specs/_runbooks/RB-DSR-LGPD-FULL.md	20185	runbook / quality doc
specs/_runbooks/RB-DSR-STATUSPAGE-PUBLISH-FAILED.md	16626	runbook / quality doc
specs/_runbooks/RB-DSR-TICKET-TRIAGE.md	17262	runbook / quality doc
specs/_runbooks/RB-ENDURANCE-24H-DRILL.md	11104	runbook / quality doc
specs/_runbooks/RB-FIPS-ATTESTATION-RENEWAL.md	8655	runbook / quality doc
specs/_runbooks/RB-GA-CUTOVER.md	37838	runbook / quality doc
specs/_runbooks/RB-GA-LAUNCH-ROLLBACK.md	20925	runbook / quality doc
specs/_runbooks/RB-LAUNCH-WAR-ROOM-COORDINATION.md	5076	runbook / quality doc
specs/_runbooks/RB-LIGHTHOUSE-CUSTOMER-INCIDENT.md	10294	runbook / quality doc
specs/_runbooks/RB-NEON-SHADOW-LAG.md	11154	runbook / quality doc
specs/_runbooks/RB-PENTEST-FINDING-ABSORPTION.md	19876	runbook / quality doc
specs/_runbooks/RB-PENTEST-FINDING-RESPONSE.md	8179	runbook / quality doc
specs/_runbooks/RB-PERF-REGRESSION.md	6655	runbook / quality doc
specs/_runbooks/RB-PILOT-ONBOARDING-E2E.md	7492	runbook / quality doc
specs/_runbooks/RB-POST-GA-CONTINUITY.md	29776	runbook / quality doc
specs/_runbooks/RB-POSTMORTEM-PROCESS.md	9557	runbook / quality doc
specs/_runbooks/RB-REPLICA-FAILOVER.md	27885	runbook / quality doc
specs/_runbooks/RB-SECRETS-DRIFT.md	8077	runbook / quality doc
specs/_runbooks/RB-SECURITY-VULNERABILITY-INTAKE.md	13177	runbook / quality doc
specs/_runbooks/RB-STATIC-ANALYSIS-TRIAGE.md	6733	runbook / quality doc
specs/_runbooks/RB-STRIPE-PORTAL-INCIDENT.md	10065	runbook / quality doc
specs/_runbooks/RB-SUBPROCESSOR-CHANGE.md	9561	runbook / quality doc
specs/_runbooks/RB-SURVEY-ABUSE.md	8718	runbook / quality doc
specs/_runbooks/RB-SYNTHETIC-PAGE-DRILL.md	8642	runbook / quality doc
specs/_runbooks/RB-SYSTEM-CMK-ROTATION.md	8508	runbook / quality doc
specs/_runbooks/RB-TABLETOP-TEMPLATE.md	6926	runbook / quality doc
specs/_runbooks/RB-TERRAFORM-DRIFT.md	6211	runbook / quality doc
specs/_runbooks/RB-VENDOR-RISK-QUARTERLY-REVIEW.md	9673	runbook / quality doc
specs/_runbooks/RB-WEBHOOK-DLQ-REPLAY.md	12004	runbook / quality doc
specs/_runbooks/STATUSPAGE-INIT.md	13614	runbook / quality doc
specs/_security/vulnerability-disclosure-policy.md	12474	governance/legal/security doc
specs/tla/PLANNED-specs.md	14727	schema / tla index
specs/tla/README.md	6183	schema / tla index
```
</details>


## §5 Validation

Post Wave A `git mv`s, the following must still resolve to existing paths:
- `scripts/validate_specs.py` allow-list entries (run `python3 scripts/validate_specs.py` after the move and expect 0 dangling refs).
- Cross-doc `references:` frontmatter entries (greppable: `grep -rE "specs/_audits/[0-9]" specs/ | grep -v "_audits/sealed"` should return 0).
- Memory references in `/Users/gustavoschneiter/.claude/projects/-Users-gustavoschneiter-Documents-HuGR/memory/MEMORY.md` and `CLAUDE.md` — both index files of audit paths; need path rewrite if Wave A is executed.

Wave B is content-additive (callouts + frontmatter bumps); no link rewrites needed. Wave C requires manual diff review per pair.

## §A Appendix — Absorbed-crate → umbrella mapping (proposed for Wave B callouts)

Derived from `specs/_audits/2026-05-26-w35-p2-*-absorption.md`. Verify against actual umbrella crates before mechanical edits.

| Absorbed crate | Umbrella crate | Module path |
|---|---|---|
| corelink-chunker, corelink-dedup, corelink-edge, corelink-lru-tracker, corelink-manifest, corelink-multipart-schema | corelink-cas | `corelink_cas::{chunker,dedup,edge,lru_tracker,manifest,multipart_schema}` |
| corelink-canary, corelink-lighthouse-tracker, corelink-logpush, corelink-otel-export, corelink-synthetic-pager | corelink-telemetry | `corelink_telemetry::{canary,lighthouse_tracker,logpush,otel_export,synthetic_pager}` |
| corelink-adapter-{brew,cargo,npm,oci,pip}, corelink-rollout-controller | corelink-adapter-host | `corelink_adapter_host::{brew,cargo,npm,oci,pip,rollout_controller}` |
| corelink-webauthn, corelink-auth-schema | corelink-ac | `corelink_ac::{webauthn,auth_schema}` |
| corelink-abuse, corelink-billing-replay, corelink-quota{,-cas,-fsm}, corelink-survey | corelink-billing | `corelink_billing::{abuse,replay,quota,quota_cas,quota_fsm,survey}` |
| corelink-dpa-versioning, corelink-privacy-* | corelink-privacy | `corelink_privacy::{dpa_versioning,breach_emit,consent_ledger,notice_emit,residency_enforcement,sub_processor_emit}` |
| corelink-admin-api, corelink-admin-dry-run, corelink-backup-verify, corelink-config-api, corelink-customer-alerts, corelink-d1-migrations, corelink-deploy-verifier, corelink-dr-drill, corelink-drata-sync, corelink-oncall, corelink-rotation-worker, corelink-supply-chain-policy, corelink-supply-verify, corelink-tenant-offboarding | corelink-ops | `corelink_ops::{admin_api,admin_dry_run,backup_verify,config_api,customer_alerts,d1_migrations,deploy_verifier,dr_drill,drata_sync,oncall,rotation_worker,supply_chain_policy,supply_verify,tenant_offboarding}` |
| corelink-ac-{core,schema} | corelink-ac | `corelink_ac::{core,schema}` |
| corelink-byok-{core,revocation,aws,gcp,azure,vault} | corelink-byok | `corelink_byok::{core,revocation,aws,gcp,azure,vault}` |

## §B DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.
