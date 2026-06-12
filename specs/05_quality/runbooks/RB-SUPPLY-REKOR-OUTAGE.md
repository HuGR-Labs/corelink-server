---
id: "RB-SUPPLY-REKOR-OUTAGE"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "supply-chain", "rekor", "sigstore", "outage", "s12"]
---

# RB-SUPPLY-REKOR-OUTAGE — Sigstore Rekor Transparency Log Outage

> **FM:** FM-156 (supply-chain) | **CTRL:** CTRL-SUPPLY-001..005, CTRL-SUPPLY-COSIGN-001 | **INV:** INV-SUPPLY-PROVENANCE-IN-REKOR (HIGH) | **SLA:** detect ≤ 5 min via health-check; mitigate ≤ 4 hours via fallback policy
>
> Production-grade SOP for handling Sigstore Rekor public transparency log
> unavailability during a Cosign-gated CF Worker deploy. Created S-12
> sprint-close P1-5 to close the dangling reference at
> `specs/03_architecture/adrs/ADR-0025-deploy-gate-hard-cosign-keyless.md:142`.

## 1. Trigger

The deploy verify gate (`corelink-deploy-verifier`) calls Rekor for an
inclusion proof and the call fails:

- `rekor.sigstore.dev` HTTP 5xx for ≥ 3 consecutive attempts.
- DNS resolution failure for `rekor.sigstore.dev`.
- TLS handshake failure (cert chain broken on Sigstore side).
- Inclusion proof returned but Merkle root verification fails (would
  indicate Sigstore-side compromise — escalate to SEV-1 immediately).

## 2. Severity

- **SEV-2 default**: Sigstore platform outage, no deploy in flight.
- **SEV-1 escalation**: deploy in flight + outage > 30 min (operator
  is forced to choose between deploy delay vs. fail-OPEN bypass —
  policy forbids the bypass).
- **SEV-1 immediate**: Merkle root mismatch (potential platform
  compromise; do NOT proceed with deploy under any circumstances).

## 3. Detection

- Synthetic check `rekor-health-probe.sh` runs every 5 min via GitHub
  Actions cron; alerts on consecutive failures.
- Metric `corelink_deploy_verify_rekor_lookup_failure_total` spikes;
  alert at ≥ 3 in 5 min.
- Customer-visible: CF Worker deploy stalls with `DeployVerifyError::
  RekorLookupFailed`.

## 4. Decision tree

| Symptom | Action |
|---|---|
| Rekor 5xx, no deploy in flight | Watch; alert SecLead; do nothing. |
| Rekor 5xx, deploy in flight, < 30 min outage | Hold deploy; alert oncall. |
| Rekor 5xx, deploy in flight, ≥ 30 min outage | SEV-1 — escalate to Architect + SRE. Deploy stays blocked per INV-SUPPLY-PROVENANCE-IN-REKOR fail-CLOSED. |
| Merkle root mismatch | **SEV-1 immediate** — abort deploy, notify Privacy + Security + Legal; treat as potential Sigstore compromise. |

## 5. Mitigation steps

### 5.1 Immediate (within 5 min of detection)

1. Confirm Sigstore status: check https://status.sigstore.dev.
2. Confirm OUR network path is healthy: test DNS + TLS to `rekor.sigstore.dev` from a non-CF host.
3. Pause any in-flight rollout (GitHub Actions `workflow_dispatch` cancel).
4. Post status to internal `#supply-chain-incidents` channel.

### 5.2 Short-term (within 4 hours)

1. If Sigstore status confirms platform outage, no action — wait for upstream recovery; INV-SUPPLY-PROVENANCE-IN-REKOR fail-CLOSED holds.
2. If our network path is degraded, route via alternate egress.
3. If Merkle root mismatch detected, **stop all deploys** — engage Sigstore community via security@sigstore.dev + post to CNCF #sigstore Slack.

### 5.3 Recovery

1. Confirm Rekor 200 responses for 5 consecutive checks.
2. Manually retry the held deploy via GitHub Actions `workflow_dispatch`.
3. Verify `corelink_deploy_verify_total{outcome="success"}` increments.
4. Close incident in audit log + post-mortem within 7 days.

## 6. Fail-OPEN policy: FORBIDDEN

**Do NOT bypass the Rekor check under any circumstance.** ADR-0025 + ADR-0045 establish Cosign + Rekor as **hard non-bypassable** gates. The acceptable outcome of a Rekor outage is **deploy delay**, never **unsigned deploy**. Any operator who manually disables the Rekor check during an outage commits an audit violation tracked under FM-156.

## 7. Communication template (customer-facing)

> Subject: Planned deploy delay — Sigstore transparency log degraded
>
> Body: Our supply-chain integrity gate requires confirmation from the
> public Sigstore Rekor transparency log before deploying new code. The
> Rekor service is experiencing degraded availability (status:
> https://status.sigstore.dev). We are holding the in-flight deploy
> until Rekor recovers. No customer-facing impact; existing service
> continues to run. Expected recovery: tracking upstream.

## 8. Post-mortem hooks

| Trigger | Owner | SLA |
|---|---|---|
| Rekor outage > 4 hours | SecLead + Architect | 7d post-mortem |
| Merkle root mismatch (any duration) | Security + Privacy + Legal | 24h incident report |
| Operator bypass attempt | Privacy Officer + Compliance | Immediate audit |

## 9. References

- ADR-0025 — Deploy gate hard Cosign keyless (canonical hard gate policy).
- ADR-0045 — SLSA L3 Rekor mandatory (canonical inclusion proof requirement).
- `corelink-deploy-verifier::verifier::verify_rekor_inclusion`
- INV-SUPPLY-PROVENANCE-IN-REKOR (HIGH, `specs/03_architecture/invariant_registry.md §3.10`).
- Sigstore community: https://sigstore.dev + https://status.sigstore.dev.
