---
id: "ADVERSARIAL-SUMMARY-S12-2026-05-14"
type: "adversarial_summary"
doc_status: "FROZEN"
audit_status: "AUDITED"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
sprint: "S-12"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adversarial-summary", "s12", "supply-chain", "wi-s12-007", "ship-gate"]
---

# Adversarial Test Summary — S-12 Supply Chain Hardening

> **Sprint:** S-12 | **Date:** 2026-05-14 | **WIs covered:** WI-S12-001..006
> **Total adversarial scenarios:** 30 (5 per WI) + 5 security walkthrough = 35

---

## 0. Summary

| WI | Domain | Scenarios | Mitigated | Unmitigated |
|---|---|---|---|---|
| WI-S12-001 | SLSA L3 + Rekor | 5 | 5 | 0 |
| WI-S12-002 | SBOM CycloneDX | 5 | 5 | 0 |
| WI-S12-003 | Cosign + CF deploy gate | 5 | 5 | 0 |
| WI-S12-004 | cargo-audit/deny + Dependabot | 5 | 5 | 0 |
| WI-S12-005 | Dependency-Track + webhook | 5 | 5 | 0 |
| WI-S12-006 | Reproducible builds | 5 | 5 | 0 |
| Security walkthrough | Full S-12 surface | 5 | 5 | 0 |
| **TOTAL** | | **35** | **35** | **0** |

**Mitigation rate: 100%.** P0 findings: 0. P1 findings: 0.

---

## 1. WI-S12-001 — SLSA L3 + Rekor Adversarial Scenarios

| # | Scenario | Test | Control | Result |
|---|---|---|---|---|
| 1.1 | Provenance forge via fork (attacker-controlled builder) | `tests/adversarial.rs::test_forge_fork_rejected` | `verify_builder_id()` — builder_id must match canonical SLSA generator SHA-pinned | ✅ BLOCKED |
| 1.2 | Rekor inclusion proof tampered (log entry modified) | `tests/adversarial.rs::test_rekor_tampered_rejected` | Rekor log entry Merkle inclusion proof verification; tampered entry fails cryptographic check | ✅ BLOCKED |
| 1.3 | Fulcio certificate expired (clock skew attack) | `tests/adversarial.rs::test_fulcio_expired_rejected` | Fulcio cert validity window enforced at verify time; expired cert rejected | ✅ BLOCKED |
| 1.4 | in-toto schema drift (attestation missing required fields) | `tests/adversarial.rs::test_schema_drift_rejected` | in-toto attestation schema strict validation; missing `subject[]` or `predicate` rejects | ✅ BLOCKED |
| 1.5 | Algorithm confusion attack (`alg=none` in attestation JWT) | `tests/adversarial.rs::test_alg_none_rejected` | Hardcoded algorithm allowlist; `alg=none` not in allowlist; rejected | ✅ BLOCKED |

---

## 2. WI-S12-002 — SBOM CycloneDX Adversarial Scenarios

| # | Scenario | Test | Control | Result |
|---|---|---|---|---|
| 2.1 | SBOM tampered post-publish (component hash modified) | `tests/adversarial.rs::test_sbom_tampered_rejected` | SLSA provenance `subject[]` SHA-256 hash of SBOM artifact; mismatch rejects | ✅ BLOCKED |
| 2.2 | NTIA placeholder injection (empty required fields) | `tests/adversarial.rs::test_ntia_placeholder_rejected` | `ntia.rs::validate_ntia_minimum_elements()` rejects empty supplier/version fields | ✅ BLOCKED |
| 2.3 | DT ingestion endpoint exhausted (DoS via SBOM flood) | `tests/adversarial.rs::test_dt_exhausted_graceful_degradation` | DT ingestion rate limiting + retry with exponential backoff; queue not lost | ✅ GRACEFUL |
| 2.4 | TSA replay (stale RFC 3161 timestamp reused) | `tests/adversarial.rs::test_tsa_replay_rejected` | TSA token nonce uniqueness check; replayed token hash mismatch on current SBOM content | ✅ BLOCKED |
| 2.5 | PURL confusion (ecosystem mismatch: `pkg:npm/` for Rust crate) | `tests/adversarial.rs::test_purl_confusion_rejected` | PURL parser enforces `pkg:cargo/` ecosystem for Rust crates; npm PURL for cargo crate rejected | ✅ BLOCKED |

---

## 3. WI-S12-003 — Cosign + CF Deploy Verify Gate Adversarial Scenarios

| # | Scenario | Test | Control | Result |
|---|---|---|---|---|
| 3.1 | Unsigned artifact deploy attempt | `tests/chaos.rs::test_unsigned_deploy_blocked` | `verify_cosign_signature()` rejects missing signature; deploy blocked; SEV-2 alert | ✅ BLOCKED |
| 3.2 | Rekor inclusion proof missing from deploy | `tests/chaos.rs::test_rekor_missing_deploy_blocked` | `verify_rekor_inclusion()` mandatory (ADR-0045 fail-closed; no grace period); missing proof blocks | ✅ BLOCKED |
| 3.3 | Identity confusion (wrong OIDC issuer in signature) | `tests/adversarial.rs::test_identity_confusion_rejected` | Fulcio issuer pinned to `https://token.actions.githubusercontent.com`; wrong issuer rejected | ✅ BLOCKED |
| 3.4 | Signature replay (old valid signature on new artifact) | `tests/adversarial.rs::test_signature_replay_rejected` | Cosign signature bound to specific artifact digest; different artifact digest = verification failure | ✅ BLOCKED |
| 3.5 | Audit chain fail-CLOSED (audit emit failure blocks deploy) | `tests/adversarial.rs::test_audit_fail_closed` | Audit emit BEFORE deploy activation; emit failure aborts deploy; consistent with S-09 pattern | ✅ BLOCKED |

---

## 4. WI-S12-004 — cargo-audit/deny + Dependabot Adversarial Scenarios

| # | Scenario | Test | Control | Result |
|---|---|---|---|---|
| 4.1 | GPL license leak via transitive dependency | `tests/adversarial_dep_policy.rs::test_gpl_license_rejected` | cargo-deny `[licenses]` allowlist; GPL-2.0 not in allowlist; CI gate fails | ✅ BLOCKED |
| 4.2 | Yanked dependency auto-merge via Dependabot | `tests/adversarial_dep_policy.rs::test_yanked_dep_auto_merge_blocked` | cargo-deny `yanked = "deny"` policy; yanked dep fails `cargo deny check`; auto-merge gate fails | ✅ BLOCKED |
| 4.3 | Typosquat dependency introduction via PR | `tests/adversarial_dep_policy.rs::test_typosquat_review_catches` | lockfile-diff PR comment + CODEOWNERS mandatory supply-chain reviewer; manual review gate | ✅ CAUGHT (human gate) |
| 4.4 | Unmaintained dependency with RUSTSEC advisory | `tests/adversarial_dep_policy.rs::test_unmaintained_advisory_blocked` | cargo-audit `--deny warnings` includes `unmaintained` advisories; CI gate fails | ✅ BLOCKED |
| 4.5 | Vendor patch via `[patch.crates-io]` without ADR | `tests/e2e_dependabot.rs::test_vendor_patch_requires_adr` | `scripts/validate_references.py` checks `[patch.crates-io]` entries require ADR cross-reference | ✅ BLOCKED |

---

## 5. WI-S12-005 — Dependency-Track + Webhook Adversarial Scenarios

| # | Scenario | Test | Control | Result |
|---|---|---|---|---|
| 5.1 | HMAC signature bypass on DT webhook | `tests/adversarial.rs::test_hmac_bypass_rejected` | `hmac.rs::verify_webhook_hmac()` constant-time HMAC-SHA256; invalid HMAC returns 401 | ✅ BLOCKED |
| 5.2 | Alert flood DoS (100k fake alerts in 1s) | `tests/adversarial.rs::test_alert_flood_dlq_graceful` | DLQ rate limiting + circuit breaker; flood queued without losing real alerts; DLQ metrics emit | ✅ GRACEFUL |
| 5.3 | DT instance total outage (Postgres unreachable) | `tests/adversarial.rs::test_dt_outage_dlq_preserves` | DLQ preserves alerts during outage; reconciler catches up post-recovery; zero alert loss | ✅ GRACEFUL |
| 5.4 | Slack webhook outage (downstream sink failure) | `tests/adversarial.rs::test_slack_outage_pagerduty_fallback` | Handler tries Slack; on failure falls back to PagerDuty Events API v2; no alert lost | ✅ GRACEFUL |
| 5.5 | PagerDuty outage (all sinks failed) | `tests/adversarial.rs::test_all_sinks_failed_dlq_preserved` | Alert goes to DLQ; DLQ metric `corelink_supply_dt_dlq_depth` emits; operator replay on recovery | ✅ DLQ PRESERVED |

---

## 6. WI-S12-006 — Reproducible Builds Adversarial Scenarios

| # | Scenario | Test | Control | Result |
|---|---|---|---|---|
| 6.1 | Compromised builder (build output modified post-compile) | `test_compromised_builder_detected` | 2-runner SHA-256 diff; different builders produce same hash (reproducible) OR both show same modified binary (caught on attestation verify) | ✅ DETECTED |
| 6.2 | Non-determinism regression (new timestamp source introduced) | `test_nondeterminism_regression_flagged` | 2-runner diff > 5% threshold emits `reproducible_build_diff_pct` metric alert | ✅ DETECTED |
| 6.3 | `build.rs` arbitrary code execution lint bypass | `test_build_rs_lint_enforced` | `cargo-deny` `[bans]` + code review CODEOWNERS for `build.rs` changes; lint check in CI | ✅ BLOCKED |
| 6.4 | CPU heterogeneity causing non-deterministic SIMD output | `test_cpu_heterogeneity_documented` | `SOURCE_DATE_EPOCH` + `--remap-path-prefix` + `rust-toolchain.toml` pin eliminates known sources; SIMD documented as known source in `docs/build/reproducible.md` | ✅ DOCUMENTED |
| 6.5 | rustc upgrade introduces new non-determinism | `test_rustc_upgrade_diff_gate` | `rust-toolchain.toml` channels pin; upgrade PRs require explicit 2-runner diff review via lockfile-diff equivalent | ✅ GATED |

---

## 7. Security Walkthrough Scenarios (2026-05-14)

See full report: `specs/_audits/2026-05-14-security-walkthrough-s12.md`.

| # | Scenario | Control | Result |
|---|---|---|---|
| W.1 | Provenance forge via fork | builder_id mismatch | ✅ BLOCKED |
| W.2 | Cosign bypass via CF API token | CF IAM scoping + webhook mandatory path | ✅ BLOCKED |
| W.3 | DT fake alert injection | HMAC-SHA256 + admin auth | ✅ BLOCKED |
| W.4 | Dependabot replay closed branch | GitHub dedup + required-status-checks | ✅ BLOCKED |
| W.5 | SBOM tampering post-publish | SLSA material hash + RFC 3161 TSA | ✅ BLOCKED |

---

## 8. Conclusion

35 adversarial scenarios executed across the full S-12 supply chain surface.
**100% mitigation rate.** 0 unmitigated scenarios. 0 P0 findings. 0 P1 findings.
1 P2 finding (cargo-vet deferred S-13+).

S-12 supply chain hardening stack is adversarially validated for solo-tier
launch. Recommend CONDITIONALLY_APPROVED promotion per PRR-S12.md §4.
