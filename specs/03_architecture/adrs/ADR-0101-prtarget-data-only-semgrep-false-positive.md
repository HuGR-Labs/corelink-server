---
id: ADR-0101
type: "adr"
doc_status: "DRAFT"
audit_status: "AUDIT_PENDING"
version: "0.1.0"
title: pull_request_target trusted-base checkout false-positive annotations
status: PROPOSED
created: "2026-09-14"
updated: "2026-09-14"
expires_at: "2026-10-14"
owner: TBD
final_approver: "Gustavo Schneiter"
reviewers:
  - role: security_lead
    name: "TBD — pending Security Lead review"
supersedes: null
superseded_by: null
tags: ["adr", "b139", "semgrep", "pull_request_target", "security"]
---

# ADR-0101 — `pull_request_target` trusted-base checkout annotations

The nine inline annotations in the five named workflows suppress only the
checkout rule at independently checked data-only sites. Immutable refs,
non-persisted credentials, BASE-owned checkers, and strict boundary guards
ensure candidate content is data; this does not waive real candidate
execution findings. The guard remains fail-closed: drift in its preconditions
requires removal or re-review of these annotations. No action, ref, workflow
behavior, or permission changes under this proposal.

Semgrep retains ignored findings in SARIF with `inSource`; B139 accounting
must explicitly account for the retained ERROR rather than infer absence from
CLI output. This proposal expires 2026-10-14 and needs fresh boundary evidence
and approval before renewal. Remove comments and this ADR if the guard is
absent or weakened; fix real candidate execution findings.

## Approval record — 2026-09-14

Gustavo Schneiter approved, as Final Approver, the **nine** `nosemgrep`
annotations for the data-only checkout boundary in the following workflows:

- `.github/workflows/backlog-verify.yml` — candidate data and BASE control tree (2).
- `.github/workflows/dependabot-policy-trust-boundary.yml` — BASE control tree and PR merge data (2).
- `.github/workflows/dependabot-policy.yml` — PR merge data and BASE control tree (2).
- `.github/workflows/file-size-ratchet.yml` — PR blobs inspected as data (1).
- `.github/workflows/secrets-drift.yml` — trusted tooling and pull-request data (2).

Approval evidence: the Final Approver's explicit message in the CoreLink
orchestration session, “Eu aprovo; informarei o Security Lead.” This is **not**
a Security Lead review or sign-off. The Security Lead will be informed by the
Final Approver; their independent review remains pending. Accordingly this ADR
remains `PROPOSED`, and B-139 must not be marked complete on this approval alone.
