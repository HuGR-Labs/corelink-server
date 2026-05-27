---
id: "AUDIT-S19-ADVERSARIAL-SUMMARY"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "WI-S19-006"
tags:
  - "audit"
  - "s19"
  - "adversarial"
  - "summary"
  - "cross-wi"
  - "ship-gate"
  - "onboarding"
  - "dpa"
  - "atomic-provisioning"
  - "enterprise-handoff"
  - "high-risk"
---

# S-19 Adversarial Summary — Cross-WI roll-up (customer onboarding ship gate)

Cross-WI consolidation of S-19 adversarial scenarios + ship-gate
quantitative readout. **27 scenarios catalogued** (target ≥ 25);
**100 % named mitigation coverage**; **0 residual HIGH/CRITICAL**;
**3 residual MEDIUM** tracked (real Stripe/Slack/CRM prod keys gap,
real Legal sign-off D+10 calendar, 5-real-signups closed-beta D+45
GA gate). Per WI-S19-006 §15 + spec contract S-19 §10 anti-scope.

---

## 1. Quantitative readout (ship-gate snapshot)

| Dimension | Target | Measured | Verdict |
|---|---|---|---|
| Signup atomicity property test (WI-S19-001) | 10k iter green | 10k iter green PR + 100k nightly | OK |
| DPA click-through 6-field consent + JWT receipt (WI-S19-002) | 3 properties × 10k iter | 3 × 10k green | OK |
| DPA versioning semver bump detection (WI-S19-003) | 1 property × 10k iter | 10k green | OK |
| INV-ONBOARD-DPA-FIRST D1 lock concurrent (WI-S19-004) | 10k iter green | 10k green | OK |
| Enterprise inquiry saga atomicity (WI-S19-005) | 10k iter green | 10k green | OK |
| Cross-WI E2E composition stress (chaos + race + saga) | 1k iter green | 1k green | OK |
| INV-ONBOARD-ATOMIC-PROVISIONING | property green + chaos rollback consistent | green | OK |
| INV-ONBOARD-DPA-FIRST | property green + lock contention safe | green | OK |
| INV-CONSENT-PROOF-VERIFIABLE round-trip | ≥ 99.9 % | 99.97 % synthetic 10k receipts | OK |
| Funnel cardinality (105 séries) | ≤ INV-OBS-CARDINALITY-BUDGET 20k | 105 / 20k = 0.5 % | OK |
| 12+ S-19 métricas emitting em DASH-ONBOARDING | 12 minimum | 14 emitted in staging stub | OK |
| Adversarial scenarios | ≥ 25 | 27 | OK |
| Mitigation coverage | 100 % | 100 % | OK |
| Cross-functional sign-offs (12 canonical) | 12 slots populated | 7 signed at SEAL + 5 deferred D+10 | CONDITIONALLY_APPROVED |
| 5 real signups closed beta (GA gate) | 5 / 5 | pending D+45 | PENDING-GA-GATE |
| Funnel sustained 30d staging (GA gate) | clean 30d | pending D+45 | PENDING-GA-GATE |
| DPA re-acceptance v1→v2 cycle (GA gate) | simulated | pending D+45 | PENDING-GA-GATE |
| Enterprise atomicity weekly chaos drill (GA gate) | 4 weeks green | pending D+45 | PENDING-GA-GATE |

---

## 2. WI-S19-001 — Signup orchestration · atomic provisioning · Clerk + D1 TX · chaos Stripe outage (5 scenarios)

| # | Scenario | Vector | Mitigation | Residual |
|---|---|---|---|---|
| 1 | Clerk webhook replay (duplicate `user.created`) | Attacker captures + replays Clerk signed webhook | Webhook idempotency key + Clerk svix-id de-dup KV cache 7d; replay → 200 OK NO-OP | LOW |
| 2 | Race condition: 2 concurrent signups same email | Email normalization not deterministic; 2 tenant rows created | D1 unique index on `LOWER(email)` + tenant provisioning inside single TX with `SELECT FOR UPDATE` equivalent | LOW |
| 3 | Stripe outage chaos: customer ID created, subscription fails | Stripe 5xx during checkout activation | Saga pattern + reconciler worker `corelink-billing-stripe`; partial state safe-mode; idempotent retry on recovery; RB-FM-SIGNUP-FAILED sub-mode 4 | LOW |
| 4 | Region drift: signup region != Clerk verify region | Locale cookie tampering; user spoofs EU when signing up from US | Region pin per `corelink_locale` cookie (S-16 middleware) + Cloudflare CF-IPCountry cross-check; mismatch → audit emit + reject | LOW |
| 5 | First-PAT race: two PATs issued on retry | Network partition mid-PAT-issuance | PAT issuance inside same D1 TX as tenant row; on retry, idempotent on `tenant_id` (one PAT per signup) | LOW |

## 3. WI-S19-002 — DPA click-through · 6-field consent · JWT receipt · 3 locales · Legal review (5 scenarios)

| # | Scenario | Vector | Mitigation | Residual |
|---|---|---|---|---|
| 6 | Locale forge: pt-BR claimed but en text shown | Client sends `locale=pt-BR` while rendering `legal/dpa/v1.en.md` | `notice_text_hash` computed client-side AND verified server-side against `legal/dpa/v1.{locale}.md` SHA-256; mismatch → 400 | LOW |
| 7 | Hash tamper: legitimate locale, modified text | Browser dev-tools edits DPA text pre-submit | Server recomputes hash from canonical file by locale + version; client-supplied hash is informational only (defense-in-depth) | LOW |
| 8 | JWT forge: receipt JWT with attacker key | Attacker tries to mint own DPA receipt | HS256 per-region signing key in CF Secrets (rotated S-13 worker); receipt verify endpoint `/v1/dpa/receipt/verify` checks issuer + kid + alg | LOW |
| 9 | Scroll-gate bypass: submit without scrolling | Frontend JS bypassed; user POSTs DPA accept directly to API | Server requires `scroll_progress=1.0` field in 6-field consent payload + UI scroll-event-emitted nonce signed by short-lived JWT; tamper → 400 | LOW |
| 10 | Pre-check injection: "agree" pre-checked | Phishing or malicious browser extension | CTRL-PRIV-CONSENT-003: checkbox MUST start unchecked; UI test gate + server requires `consent.user_action=click` audit field | LOW |

## 4. WI-S19-003 — DPA versioning · re-acceptance · 30d grace · degrade read-only (5 scenarios)

| # | Scenario | Vector | Mitigation | Residual |
|---|---|---|---|---|
| 11 | Bump kind misidentification: material change as minor | Author marks `v1.1.0` for clause change requiring re-acceptance | Semver bump-kind property test + Legal review checklist + diff size guard (> 200 lines = MAJOR by policy); CI fail otherwise | LOW |
| 12 | Email broadcast bounce: tenant never notified | Tenant email invalid; no re-acceptance prompt; auto-degraded after 30d | Pre-broadcast email validation + in-app banner + admin-emit alert + DPO review for hard-bounce list (CTRL-PRIV-NOTIFY-001) | LOW |
| 13 | Grace-period drift: 30d → 45d via clock skew | Server clock skew or worker timer mishandling | `dpa_acceptance_grace_expiry` calculated at bump-time + stored in D1; worker reads stored timestamp NOT wall-clock delta | LOW |
| 14 | Degrade-mode aggressive: read-only too soon | Worker mis-calc + tenant degraded prematurely | Worker requires 2-of-2 confirmation: stored expiry AND admin-emit approval; CTRL-ADMIN-002 dual-control on degrade | LOW |
| 15 | Bypass attempt: tenant ignores DPA v2 by tampering with `accepted_version` in client | Browser sends fake accepted_version field | Server is source of truth (`dpa_acceptance` D1 row + receipt JWT); client value is hint only | LOW |

## 5. WI-S19-004 — Tier selection · Stripe checkout · INV-ONBOARD-DPA-FIRST · D1 lock (5 scenarios)

| # | Scenario | Vector | Mitigation | Residual |
|---|---|---|---|---|
| 16 | Concurrent flow: 2 tabs → 2 Stripe customers same tenant | User opens 2 tabs, completes Stripe twice | Idempotency key per tenant_id at `stripe_customer` creation + D1 unique index `(tenant_id) UNIQUE`; second attempt 409 | LOW |
| 17 | D1 lock bypass: tier selection without DPA | Direct API call to `/v1/onboard/tier` without `dpa_acceptance` row | Server enforces `SELECT dpa_acceptance ... FOR UPDATE` inside same TX as tier write; missing row → 412 PreconditionFailed; INV-ONBOARD-DPA-FIRST property test 10k green | LOW |
| 18 | Stripe webhook replay: subscription activated twice | Attacker replays `customer.subscription.created` | Stripe svix-id de-dup KV 7d + idempotency on `stripe_subscription_id` D1 unique | LOW |
| 19 | Free-tier bypass: claim free → upgrade without DPA re-accept | User signs up free, then upgrades to paid tier while skipping DPA re-check | Tier upgrade requires fresh DPA verify (receipt JWT iat < 30d OR re-prompt); CTRL-ONBOARD-005 | LOW |
| 20 | Enterprise route bypass: claim enterprise tier in checkout | User picks enterprise tier in Stripe Checkout to bypass sales review | Enterprise tier removed from Stripe Checkout product list; only reachable via Sales-issued Stripe coupon code AND WI-S19-005 inquiry-form route | LOW |

## 6. WI-S19-005 — Enterprise inquiry form · Slack + CRM atomic · 24h auto-reply SLA (5 scenarios)

| # | Scenario | Vector | Mitigation | Residual |
|---|---|---|---|---|
| 21 | Saga partial state: Slack post OK, CRM 5xx | HubSpot API outage mid-saga | Outbox pattern; Slack + CRM both produce idempotent ops keyed on `inquiry_id`; reconciler retries CRM until success; property test 10k saga atomicity green | LOW |
| 22 | Spam: 10k bot inquiries | Bot floods inquiry form | reCAPTCHA v3 score < 0.5 reject + per-IP rate limit (S-08) + email domain MX check | LOW |
| 23 | reCAPTCHA bypass: token re-use | Attacker captures one good token, replays | reCAPTCHA token single-use server-side; `action` field bound to form-id | LOW |
| 24 | Slack forge: attacker spoofs internal Slack hook | Attacker discovers `#sales-inquiries` webhook URL | Webhook URL in CF Secrets (not in code); Slack signing secret verified on all inbound; outbound posts use signed payload | LOW |
| 25 | CRM key exfiltration: HubSpot API key in logs | safeLog miss leaks key | safeLog redaction rule pattern `pat-*` + `hsk-*` (CTRL-PRIV-LOG-REDACT) + CI scan `gitleaks`; rotation every 90d (S-13 rotation worker) | LOW |

## 7. WI-S19-006 — Ship gate (2 scenarios)

| # | Scenario | Vector | Mitigation | Residual |
|---|---|---|---|---|
| 26 | Cross-WI integration drift: WI-S19-002 schema bump breaks WI-S19-003 re-acceptance | Schema_version mismatch | Cross-WI integration property test 1k iter green; CI gate on schema-version compat matrix; failure_modes.md FM-X-SCHEMA-DRIFT cross-ref | MEDIUM (real-prod-keys gap until D+10) |
| 27 | Funnel cardinality regression: per-tenant label accidentally added | Engineer adds `tenant_id` label to onboarding métrica | CI gate `cardinality-budget.py` (S-09); fails build if any S-19 funnel série emits more than 105 cardinality; INV-OBS-CARDINALITY-BUDGET enforced | LOW |

---

## 8. Residual MEDIUM tracking

| ID | Residual | Reason | Expiry / Resolution |
|---|---|---|---|
| RES-S19-01 | Real Stripe + Slack + HubSpot prod keys gap | Org-level secret provisioning out of WI scope; staging uses sandbox keys | D+10 (first post-PRR staging→prod cutover) |
| RES-S19-02 | Real Legal sign-off (vs synthetic Legal SME review) per locale | External Legal counsel scheduling lag; synthetic SME review captured by Architect dual-hat per ADR-0034 | D+10 (external Legal counsel engagement scheduled) |
| RES-S19-03 | 5 real signups closed beta + 30d sustained funnel + v1→v2 DPA cycle + weekly chaos drill | Observation-window items; canonically GA Evidence Gate at D+45 | D+45 GA Evidence Gate (post-Implementation SEAL) |

All 3 tracked in WI-S19-006 §28 risk register (R-001..R-009) + PRR-S19 §7 waiver table.

---

## 9. Coverage cross-reference

| INV | Property test | Adversarial scenario | RB cross-ref |
|---|---|---|---|
| INV-ONBOARD-ATOMIC-PROVISIONING | WI-S19-001 prop 1 (10k) + cross-WI prop 6 (1k) | 1, 2, 3, 5, 26 | RB-FM-SIGNUP-FAILED sub-modes 1,3,4,6 |
| INV-ONBOARD-DPA-FIRST | WI-S19-004 prop 1 (10k) + cross-WI prop 6 (1k) | 17, 19, 20 | RB-FM-SIGNUP-FAILED sub-mode 5 |
| INV-CONSENT-PROOF-VERIFIABLE | WI-S19-002 prop 3 (10k JWT round-trip) | 8 | (S-11 DSR runbook) |
| INV-OBS-CARDINALITY-BUDGET | CI gate cardinality-budget.py | 27 | (S-09) |
| INV-AUDIT-APPEND-ONLY | WI-S09 audit chain processor (inherited) | 6 (cross-mode in RB) | RB-FM-SIGNUP-FAILED sub-mode 6 |

---

## 10. Verdict

- **27 scenarios catalogued** (target ≥ 25); **100 % named mitigation**.
- **0 residual HIGH / CRITICAL**; **3 residual MEDIUM** all with named expiry triggers (RES-S19-01..03).
- **24/27 LOW** + **3/27 MEDIUM** (all GA-gate observation items, not blockers for Implementation SEAL D+15).
- PRR-S19 promotes to **`CONDITIONALLY_APPROVED`** with 5 waiver rows (W1..W5; see PRR §7).
- **D+45 GA Evidence Gate** validates RES-S19-03 + 4 observation items (signup ≤ 3 min sustained, funnel 30d, v1→v2 cycle, enterprise weekly chaos drill); these are **HARD for GA** independent of Implementation SEAL D+15 unblock for S-20 development.

---

**Fim S-19 adversarial summary cross-WI roll-up.**
