# API stability baseline — 2026-05-15

**Scope:** Every endpoint and gRPC RPC reachable on `api.corelink.humangr.com`,
`cas.corelink.humangr.com`, `bytestream.corelink.humangr.com` at the GA gate. Tier
assignment is derived from `openapi/corelink-v1.yaml` (`x-stability`
extension, when present) plus a manual inference pass for endpoints that
do not yet carry the extension, using these rules:

- **GA** — endpoint backs the documented happy-path of a SAM-tier paying
  customer; has SLO instrumentation; has SDK coverage in all four first-party
  SDKs; has been in production ≥ 90d without shape change.
- **Preview** — endpoint exists in production but either lacks SLO,
  lacks one or more SDKs, or has had a shape change in the last 90d.
- **Internal** — endpoint exists only for CoreLink staff or system-to-system
  use (webhook ingestors, health probes invoked by infra, admin ops endpoints
  gated by dual-control).

**Authority:** This baseline is the inputs to the
[API stability policy](../../apps/docs/docs/explanation/api-stability.mdx).
A subsequent commit will materialise `x-stability` on every operation in
`openapi/corelink-v1.yaml` so this matrix becomes mechanically derivable.

**Total surface:** 35 endpoints (27 REST + 8 gRPC RPCs).

---

## 1. REST surface — `/v1/*` and `/api/*`

### 1.1 Signup, DPA, tier, PAT (customer-facing onboarding)

| OperationId        | Method | Path                              | Tier      | Justification                                                 |
|--------------------|--------|-----------------------------------|-----------|---------------------------------------------------------------|
| `signup`           | POST   | `/v1/signup`                      | **GA**    | Top-of-funnel; SLO target 99.9%; all 4 SDKs                   |
| `dpaAccept`        | POST   | `/v1/dpa/accept`                  | **GA**    | LGPD Art. 7 §1º consent ledger entry; legally load-bearing    |
| `dpaReAccept`      | POST   | `/v1/dpa/re-accept`               | **GA**    | DPA version transitions; legally load-bearing                 |
| `tierSelect`       | POST   | `/v1/onboarding/tier-select`      | **GA**    | Stripe Checkout handoff; revenue-critical                     |
| `patIssue`         | POST   | `/v1/pats`                        | **GA**    | Service-account credential issuance; SDK foundation           |
| `patList`          | GET    | `/v1/pats`                        | **GA**    | Audit + rotation workflows depend on it                       |
| `patRevoke`        | DELETE | `/v1/pats/{pat_id}`               | **GA**    | Security-critical credential revocation                       |

### 1.2 Billing webhook (system-to-system)

| OperationId        | Method | Path                              | Tier         | Justification                                                 |
|--------------------|--------|-----------------------------------|--------------|---------------------------------------------------------------|
| `stripeWebhook`    | POST   | `/v1/billing/stripe-webhook`      | **Internal** | Stripe → CoreLink only; HMAC-gated; never called by customers |

### 1.3 Privacy — DSR

| OperationId        | Method | Path                                          | Tier      | Justification                                                                          |
|--------------------|--------|-----------------------------------------------|-----------|----------------------------------------------------------------------------------------|
| `dsrSubmit`        | POST   | `/v1/privacy/dsr/{action}`                    | **GA**    | LGPD Art. 18 + GDPR Art. 15-22 obligation; 30-day SLA; legally mandated                |
| `dsrStatus`        | GET    | `/v1/privacy/dsr/{request_id}/status`         | **GA**    | Same legal mandate; required for closing the loop with data subjects                   |

### 1.4 Privacy — consent ledger

| OperationId               | Method | Path                                  | Tier      | Justification                                                                 |
|---------------------------|--------|---------------------------------------|-----------|-------------------------------------------------------------------------------|
| `consentGrant`            | POST   | `/v1/consent/{purpose}`               | **GA**    | LGPD Art. 8 §1º consent capture; HMAC proof in audit chain                    |
| `consentRevoke`           | DELETE | `/v1/consent/{purpose}`               | **GA**    | LGPD Art. 8 §5º revocation; symmetric with grant                              |
| `consentHistory`          | GET    | `/v1/consent`                         | **GA**    | Data-subject self-service inspection                                          |
| `consentVerifyGrant`      | GET    | `/v1/consent/verify`                  | **GA**    | Third-party HMAC verification of grant proofs                                 |
| `consentVerifyRevocation` | GET    | `/v1/consent/revocation/verify`       | **GA**    | Third-party HMAC verification of revocation proofs                            |

### 1.5 Admin — dual-control ops

| OperationId         | Method | Path                                      | Tier         | Justification                                                                                |
|---------------------|--------|-------------------------------------------|--------------|----------------------------------------------------------------------------------------------|
| `adminOpsSubmit`    | POST   | `/v1/admin/ops`                           | **Internal** | CoreLink staff only; M-of-N approval workflow                                                 |
| `adminOpGet`        | GET    | `/v1/admin/ops/{op_id}`                   | **Internal** | Staff-only inspection                                                                         |
| `adminOpApprove`    | POST   | `/v1/admin/ops/{op_id}/approve`           | **Internal** | Staff-only; dual-control signing                                                              |
| `adminOpReject`     | POST   | `/v1/admin/ops/{op_id}/reject`            | **Internal** | Staff-only                                                                                    |
| `adminAuditEvents`  | GET    | `/v1/admin/audit/events`                  | **Preview**  | Customer-facing audit feed *planned*; today restricted; promote to GA after CAP-AUDIT-FEED SEAL |
| `adminTenantsList`  | GET    | `/v1/admin/tenants`                       | **Internal** | Staff-only; cross-tenant view                                                                  |

### 1.6 Enterprise

| OperationId          | Method | Path                          | Tier      | Justification                                                       |
|----------------------|--------|-------------------------------|-----------|---------------------------------------------------------------------|
| `enterpriseInquire`  | POST   | `/v1/enterprise/inquire`      | **GA**    | Public lead-form endpoint; non-authenticated; rate-limited          |

### 1.7 Users + reference data

| OperationId            | Method | Path                       | Tier        | Justification                                                                                       |
|------------------------|--------|----------------------------|-------------|-----------------------------------------------------------------------------------------------------|
| `usersMeGet`           | GET    | `/v1/users/me`             | **GA**      | Identity endpoint; foundation of every SDK auth flow                                                 |
| `usersMePatch`         | PATCH  | `/v1/users/me`             | **Preview** | Field set still expanding (notification prefs, locale, MFA pref); shape change risk through 2026-Q3 |
| `dataCategoriesList`   | GET    | `/v1/data-categories`      | **GA**      | Public reference data; required for DSR self-service                                                 |

### 1.8 Ops surface

| OperationId   | Method | Path                  | Tier         | Justification                                                                  |
|---------------|--------|-----------------------|--------------|--------------------------------------------------------------------------------|
| `apiHealth`   | GET    | `/api/health`         | **Internal** | LB/probe surface; not part of customer contract; format may change for ops     |
| `cspReport`   | POST   | `/api/csp-report`     | **Internal** | Browser-to-CoreLink only; not part of customer contract                        |

---

## 2. gRPC REAPI v2 surface

Hosted at `cas.corelink.humangr.com:443` (CAS + Capabilities) and
`bytestream.corelink.humangr.com:443` (ByteStream + Health).

| Service                            | RPC                  | Tier      | Justification                                                            |
|------------------------------------|----------------------|-----------|--------------------------------------------------------------------------|
| `Capabilities`                     | `GetCapabilities`    | **GA**    | REAPI v2 §2.1 mandatory; clients pin against this                        |
| `ContentAddressableStorage`        | `BatchUpdateBlobs`   | **GA**    | Core CAS write path; SLO p99 < 50ms                                      |
| `ContentAddressableStorage`        | `FindMissingBlobs`   | **GA**    | Core CAS probe; SLO p99 < 30ms                                           |
| `ContentAddressableStorage`        | `BatchReadBlobs`     | **GA**    | Core CAS read path; SLO p99 < 50ms                                       |
| `google.bytestream.ByteStream`     | `Read`               | **GA**    | Large-blob read path; SLO p99 < 200ms first-byte                         |
| `google.bytestream.ByteStream`     | `Write`              | **GA**    | Large-blob write path; SLO p99 < 200ms first-ack                         |
| `google.bytestream.ByteStream`     | `QueryWriteStatus`   | **GA**    | Resumable upload contract                                                 |
| `grpc.health.v1.Health`            | `Check`              | **Internal** | gRPC LB probe; not part of customer contract                          |

**Action Cache (`GetActionResult`, `UpdateActionResult`) is not yet exposed** —
ships in S20 alongside the rest of the AC surface. When it ships it will be
**Preview** for 90 days before GA promotion per §1.1 of the stability policy.

---

## 3. Tier rollup

| Tier      | Count | Share  |
|-----------|-------|--------|
| GA        | 23    | 65.7%  |
| Preview   | 2     | 5.7%   |
| Internal  | 10    | 28.6%  |
| **Total** | **35**| **100%** |

The two Preview endpoints (`adminAuditEvents`, `usersMePatch`) are on track
for GA promotion in **2026-Q4** pending the gates in stability policy §1.1.

---

## 4. Required follow-ups

1. **Materialise `x-stability` extension** on every operation in
   `openapi/corelink-v1.yaml` so this audit becomes mechanically derivable
   (`scripts/extract-api-deprecations.py` already reads the extension).
2. **Add `x-stability` to the REAPI proto** as a `google.api.field_behavior`
   annotation or a custom `corelink.stability` option.
3. **Generate a public version of this table** at
   `apps/docs/docs/reference/api/stability-matrix.mdx` once `x-stability`
   ships in the spec.
4. **Annotate SDK symbols** with the stability proc macros referenced in
   the policy doc §4.3.

---

## 5. Sign-off

- **Tech Lead:** _to be signed at R-7-2 cluster_
- **Security Lead:** _to be signed at R-7-2 cluster_
- **DPO:** _to be signed at R-7-2 cluster_

Date: 2026-05-15
Author: Orchestrator (R-prep API stability swarm)
