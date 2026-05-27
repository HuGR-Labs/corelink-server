# Wave-27 — Pilot Signup Pipeline (engineering-side prep)

> **Doc kind:** wave-27 R-prep audit / pilot-signup engineering-side enablement audit (no canonical front matter required — `_audits/` excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Author:** wave-27 R-prep pilot-signup-pipeline agent (Claude Opus 4.7) — branch `wt/r-prep-pilot-signup-pipeline`.
> **Base:** `main` @ `a48bbec`.
> **Scope:** land the engineering-side artefacts (admin tooling + dashboard placeholder + DEBT register row) that make the GA-blocking "pilot signups ≥ 3" gate executable. The actual signups remain Owner-bound (announcement + outreach + closing); engineering's job is to remove every operator-side blocker on the path from "tenant clicks signup link" to "pilot ACTIVE with first blob committed inside 24h".
> **Cross-ref:** `specs/_audits/sealed/2026-05-16-pilot-onboarding-e2e.md` (wave-23 17-test journey + 5 binaries), `docs/internal/customer-success-playbook.md` (wave-23 CS playbook), `docs/internal/pilot-comms-templates.md` (wave-23 canned-email pack), `docs/internal/pilot-dashboard-checklist.md` (wave-23 10-metric tile spec), `specs/_audits/sealed/2026-05-15-debt-register.md` (DEBT-027 row added below).

---

## 1. Customer-facing signup link

The wave-27 contract for the customer-facing signup surface is:

```
https://signup.corelink.humangr.com/pilot/<token>
```

`<token>` is a single-use slot reservation issued out-of-band by the operator (typically pasted into the personalised outreach email from `docs/internal/pilot-comms-templates.md`). The token-based model is intentional:

- We do NOT yet have a public CTA on `corelink.humangr.com` (pilot cohort is capped at 5–10 per customer-success-playbook §1).
- The token carries: `target_tier=pilot`, `slot_id`, `valid_until_ms`, signature.
- When redeemed, the token mints a `tenant_id` + sets `pilot_state = NEW`.
- Tokens expire after 14 days unredeemed (matches sales-cycle median).

**Production wiring target:** `apps/server/src/signup/pilot_token.rs` (T-7d). Until then, the operator hand-issues `tenant_id` via the existing tenant-provisioning admin endpoint (cf. `docs/internal/admin-plane.md`).

This surface is intentionally minimal: no marketing site copy, no public webhook, no Stripe checkout coupling. The pilot tier is enacted SERVER-SIDE by the operator running `scripts/admin/grant-pilot-tier.sh` AFTER the human-validated NDA / DPA flow per customer-success-playbook §2.

### 1.1 Anti-spec — what this surface is NOT

- NOT a self-service "click to sign up for a free trial" landing page. Self-service onboarding is post-GA work (GA-Full, not GA-Limited).
- NOT integrated with Stripe checkout. Pilot tenants pay zero during the pilot window (pilot success criteria are conversion-to-paid, not revenue-during-pilot).
- NOT visible from the public docs site. The link is share-only.

---

## 2. Owner pre-flight — operator action sequence per signup

The operator (Gustavo / customer-success function) executes this 8-step sequence for every pilot signup:

| # | Step | Tool / artefact | Output |
|---|---|---|---|
| 1 | NDA + DPA executed | `docs/internal/legal-review-process.md` | Counter-signed PDFs filed |
| 2 | Token minted + emailed | `docs/internal/pilot-comms-templates.md §1` | Token URL in customer's inbox |
| 3 | Customer redeems token | (customer-side; no operator action) | `tenant_id` provisioned with `pilot_state = NEW` |
| 4 | List `NEW` queue | `./scripts/admin/list-pilot-tenants.sh --state NEW` | Operator sees newly-redeemed tenants |
| 5 | Grant pilot tier | `./scripts/admin/grant-pilot-tier.sh --tenant-id <uuid> --tier pilot --cap 100GB` | `pilot_state := ACTIVE`; `EVT-PILOT-TIER-GRANTED` audit row emitted |
| 6 | Send onboarding email | `docs/internal/pilot-comms-templates.md §2` | Customer has SDK + first-blob walkthrough |
| 7 | Schedule 24h check-in | `./scripts/admin/pilot-24h-checkin.sh --tenant-id <uuid> --dry-run` (preview alert payload) | Operator confirms cron will pick up tenant |
| 8 | Operator SLA timer | 4h response from token redeem → tier grant per customer-success-playbook §3.2 | If breached → escalate to Owner |

**Charter:** every step above MUST be doable by a single operator without engineer assistance. Steps 4, 5, 7 are wave-27 deliverables that close this loop.

### 2.1 Wave-27 deliverables — admin tooling

| Tool | Path | Mode |
|---|---|---|
| Grant pilot tier | `scripts/admin/grant-pilot-tier.sh` | Placeholder (emits canonical SQL + audit-event preview); production wires to `apps/server/src/admin/tier_grant.rs` at T-7d |
| List pilot tenants | `scripts/admin/list-pilot-tenants.sh` | Placeholder (emits canonical SQL + table preview); production wires to `apps/server/src/admin/tier_list.rs` at T-7d |
| 24h check-in poller | `scripts/admin/pilot-24h-checkin.sh` | Placeholder (emits canonical SQL + Slack alert payload); production wires to `apps/server/src/admin/pilot_checkin.rs` + `.github/workflows/pilot-checkin-cron.yml` at T-7d |

All three scripts:
- have shebang `#!/usr/bin/env bash`,
- are `chmod +x`,
- emit DCO sign-off + Co-Authored-By header in the script preamble,
- exit non-zero on invalid argument shapes (validated locally),
- print explicit `[PLACEHOLDER]` markers before any side-effect that is not yet wired.

### 2.2 Smoke-test evidence

```bash
$ ./scripts/admin/grant-pilot-tier.sh --tenant-id 4a2c0a7e-1b9f-4d3a-9c0e-7e1f2b3a4c5d --tier pilot --cap 100GB --dry-run
[INFO] grant-pilot-tier action plan:
  tenant_id:    4a2c0a7e-1b9f-4d3a-9c0e-7e1f2b3a4c5d
  tier:         pilot
  cap:          100GB (100000000000 bytes)
  granted_at:   1778923639556 (ms epoch UTC)
  audit_event:  EVT-PILOT-TIER-GRANTED
  state_after:  pilot_state := ACTIVE (was: NEW)
[DRY-RUN] No changes applied. Exit 0.

$ ./scripts/admin/list-pilot-tenants.sh --state NEW
[INFO] list-pilot-tenants filter: pilot_state = 'NEW'
[PLACEHOLDER] D1 admin-plane list-binding not yet wired.
…
[DONE] list-pilot-tenants exit 0

$ ./scripts/admin/pilot-24h-checkin.sh --all-active --dry-run
[INFO] pilot-24h-checkin
  scope:         all ACTIVE pilots
  slack_webhook: <unset; placeholder payload only>
…
[DRY-RUN] No Slack POST issued. Exit 0.
```

Invalid-argument paths also smoke-tested locally (UUID format guard, tier guard, cap guard, state enum guard, mutually-exclusive `--tenant-id` + `--all-active`).

---

## 3. First-24h auto-checkin

The strongest leading indicator of pilot churn is a tenant whose first 24h pass with zero blobs committed (`first_blob_at_ms IS NULL`). The 24h-checkin script captures this signal and fans out a Slack alert to `#pilot-onboarding` for human follow-up.

### 3.1 Alert rule

```
emit alert WHEN
    pilot_state = 'ACTIVE'
    AND tier_granted_at_ms IS NOT NULL
    AND (now_ms - tier_granted_at_ms) >= 86_400_000   -- 24h
    AND first_blob_at_ms IS NULL
```

### 3.2 Cron cadence

T-7d ship target: `.github/workflows/pilot-checkin-cron.yml` running every 4 hours. The cadence is intentionally aggressive: we want at most a 4h-late alert when a tenant slips past the 24h boundary.

### 3.3 Slack payload schema

The payload body is documented inline in `scripts/admin/pilot-24h-checkin.sh §2` and matches the canonical CS-rotation alert shape (`channel: #pilot-onboarding`, `username: pilot-24h-checkin`, `icon_emoji: :warning:`). Sensitive fields are redacted (the webhook URL itself is printed only by 8-char suffix).

### 3.4 De-duplication

Production wiring MUST de-dupe alerts by `(tenant_id, day_bucket_utc)` so the same tenant does not page CS every 4h. Placeholder script has no state; production binary maintains a small KV table.

---

## 4. Pilot dashboard tile placeholder

The wave-23 `docs/internal/pilot-dashboard-checklist.md` enumerated 10 canonical pilot-health metrics. Wave-27 ships the Grafana-side scaffolding for those metrics:

| Path | Role |
|---|---|
| `dashboards/grafana/dash-pilot-tenants.yml` | Source-of-truth YAML for the 10-panel dashboard (this wave) |
| `dashboards/grafana/DASH-PILOT-TENANTS.json` | (T-7d) Generated JSON consumed by Grafana |

The YAML is intentionally readable: panel IDs 1-10 each have a `title`, `description`, `targets` (PromQL), and where appropriate `thresholds` + `gate` annotations. Panel 1 ("Pilot signup count rolling 7d") carries the `gate: GA-readiness pilot-signups ≥ 3` annotation that ties this dashboard back to the wave-27 raison d'être.

The dashboard depends on the `corelink_pilot_*` Prometheus metric family being wired in `apps/server/src/admin/pilot_metrics.rs` at T-7d. None of those metrics exist yet; the YAML lists them by canonical name so the production wiring lands them in lock-step.

### 4.1 RBAC

`DASH-PILOT-TENANTS` is AdminCtx-only. `corelink-viewer` tenants MUST NOT see this dashboard (no per-tenant deep-dive on competitor pilots). The Grafana folder-permissions wiring is documented in `docs/internal/admin-plane.md §3` (RBAC).

---

## 5. Wave-23 audit cross-reference

`specs/_audits/sealed/2026-05-16-pilot-onboarding-e2e.md` documents the 17-test journey across signup → first CAS → audit export → DSR → offboarding (5 stages, 5 test binaries). Wave-27 extends that contract with the **operator-side** tooling that surrounds it — the wave-23 audit is *journey-shape*, wave-27 is *operator-action-shape*. Both close together to make GA cutover's "pilot signups ≥ 3" gate executable.

Section §pilot-signup-pipeline-wave-27-prep references this audit from wave-23's open-questions tail (§7).

---

## 6. DEBT register

A new row `DEBT-027 Pilot signups (≥3 to GA)` is added to `specs/_audits/sealed/2026-05-15-debt-register.md`. The row encodes:

- engineering-side artefacts COMPLETE this wave (the 3 admin scripts + dashboard placeholder + this audit),
- the actual signup count is Owner-bound (announcement + outreach + closing),
- target close = T-7d before GA cutover with ≥ 3 ACTIVE pilots whose `first_blob_at_ms` is non-NULL.

---

## 7. GA-readiness gate

The wave-27 deliverables clear the engineering side of the pilot-signups gate. The remaining work is Owner-bound:

1. Announce pilot programme (Slack / LinkedIn / 1:1 outreach to ICP).
2. Pre-screen candidates against customer-success-playbook §1 ICP filter.
3. Execute the 8-step pre-flight (§2 above) per signup.
4. Sustain ≥ 3 ACTIVE pilots with first-blob committed inside 24h.

This audit + the 3 admin scripts + the dashboard placeholder constitute the engineering-side handoff for that flow.

---

## 8. Charter compliance

- DCO sign-off on the commit.
- All 3 admin scripts are `chmod +x` with shebangs.
- DCO + Co-Authored-By headers in each script.
- All bash invocations are synchronous (no `&`, no background flags).
- No new Rust crates (placeholder phase — production wiring T-7d).
- `validate_specs.py` not impacted (this doc lives in `_audits/`, exempted).
- `validate_references.py` clean (no spec-ID citations require defs).

---

## 9. Follow-ups

- **T-7d:** wire `apps/server/src/admin/{tier_grant,tier_list,pilot_checkin}.rs`; emit the `corelink_pilot_*` metric family; generate `DASH-PILOT-TENANTS.json` from `dash-pilot-tenants.yml`; add `.github/workflows/pilot-checkin-cron.yml`.
- **T-7d:** lift the `[PLACEHOLDER]` blocks from each script so they speak to the production admin-plane endpoint.
- **GA cutover:** verify "pilot signups ≥ 3" closure per `RB-GA-CUTOVER.md` and close DEBT-027.
