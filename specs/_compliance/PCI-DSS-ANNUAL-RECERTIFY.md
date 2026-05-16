---
id: "PCI-DSS-ANNUAL-RECERTIFY"
type: "compliance_checklist"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R5-3"
parent_wi: "R-PREP-PCI-SAQ-A"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "PCI-DSS-SAQ-A-2026-05-15"
  - "PCI-DSS-BOUNDARY-DIAGRAM"
tags: ["pci-dss", "pci-dss-v4-0", "saq-a", "recertification", "annual", "checklist", "compliance", "r-prep"]
---

# PCI DSS SAQ-A — Annual Recertification Checklist

> **doc_status:** DRAFT · **audit_status:** ACTIVE · **purpose:** ensure
> CoreLink re-attests its SAQ-A compliance every year before the previous
> attestation's validity expires, and re-confirms the boundary claim that
> zero CHD touches CoreLink infrastructure.
>
> **Cadence:** **annual**, anchored to the SAQ-A assessment date
> (currently 2026-05-15 → next due **2027-05-15**, with a target start
> **T-30 days = 2027-04-15**). Early re-attestation is required if any of
> the trigger events in §1 fire.
>
> **Owner:** Security Lead (or Gustavo Schneiter in acting capacity until
> recruit closes).

---

## 1. Trigger events forcing early re-attestation

Re-attestation must begin *immediately* (not on the annual cadence) if any
of the following occurs:

- [ ] **Boundary-diagram-relevant change**: any new payment surface, new
      Stripe integration mode (Connect, ACH, Apple Pay direct, in-person
      via Stripe Terminal), or new script on the checkout page beyond
      `'self' + js.stripe.com`.
- [ ] **Stripe AOC lapse**: Stripe drops off the Visa Global Registry or
      MC Compliant Service Provider list, or Stripe AOC is not refreshed
      within 90 days of expiry.
- [ ] **CHD column appearing in schema**: any D1 schema migration adds a
      column matching `pan|card_number|cvv|cvc|track_data|pin_block` (CI
      gate: `scripts/verify-no-chd-schema.py`, to be authored as part of
      this checklist's first run).
- [ ] **PCI SSC publishes SAQ A v4.x update**: new questions or revised
      eligibility criteria.
- [ ] **Suspected CHD compromise**: any incident where CHD may have
      transited a CoreLink-operated component (covered also by IR
      scenario TT-03).
- [ ] **Acquisition / divestiture / new entity**: any corporate event that
      shifts the merchant-of-record relationship with Stripe.

If a trigger fires, file an incident in the IR system tagged
`pci-recert-trigger` and follow §2 immediately.

---

## 2. Annual recertification — step-by-step checklist

> Target start date: **T-30 days before validity expiry.**
> Target completion: **T-0 (validity expiry day)**.
> Total effort: ~6 hours over 30 calendar days (most of the time is the
> wait for Stripe's annual AOC refresh).

### Step 1 — Verify Stripe AOC is current

- [ ] Pull Stripe's latest **Attestation of Compliance** (AOC) from the
      Stripe trust center or via Drata vendor module.
- [ ] Confirm AOC version covers **PCI DSS v4.0** (or successor).
- [ ] Confirm AOC issuance date is **within the last 12 months**.
- [ ] Confirm Stripe is listed on **Visa Global Registry of Service
      Providers** (https://www.visa.com/splisting/).
- [ ] Confirm Stripe is listed on **Mastercard Compliant Service Provider
      List**.
- [ ] Archive AOC PDF in `specs/_compliance/vendor-dd/DD-STRIPE/` with
      filename `AOC-stripe-YYYY-MM.pdf`.
- [ ] Record the AOC review in
      `specs/_compliance/VENDOR-RISK-REGISTER.md` §Stripe.

**Pass criterion:** AOC is current, on both registries, and archived.
**Fail action:** if Stripe AOC has lapsed > 90 days, escalate to CEO+CTO
within 24 h; consider invoking the *secondary-PSP fallback* (currently
not designed — backlog item for R5-3.1).

### Step 2 — Verify zero-CHD claim still holds on CoreLink systems

- [ ] Run the boundary-verification grep recipe from
      `PCI-DSS-BOUNDARY-DIAGRAM.md` §7:

```bash
# Codebase grep for CHD names outside test/spec docs.
grep -rEn 'card_number|cvv|cvc|\bpan\b' --include='*.rs' --include='*.ts' --include='*.tsx' --include='*.py' --include='*.go' --include='*.js' \
  | grep -vE '/tests/|/test/|/specs/|_test\.|\.test\.'

# Spec-side grep for CHD column names in the data model.
grep -rEn 'card_number|cvv|cvc|\bpan\b' specs/03_architecture/data_model.md
```

- [ ] Confirm hits are **only** in `crates/corelink-logpush/src/redaction.rs`
      (the defense-in-depth redactor). Any other hit is a SEV-2 finding;
      open a ticket and stop the recertification until resolved.
- [ ] Re-confirm D1 schema audit: open the latest migration file in
      `crates/corelink-d1-schema/migrations/` and verify no CHD columns
      were added in the last 12 months.
- [ ] Re-confirm logpush redaction tests still green:
      `cargo test -p corelink-logpush --test pii_redaction_100k_synthetic`.
- [ ] Re-confirm CSP on the live checkout page:
      `curl -sI https://billing.corelink.humangr.com/checkout | grep -i content-security-policy`
      → must include `script-src 'self' https://js.stripe.com`
      (no analytics, no tag manager, no chat widget).

**Pass criterion:** all four sub-checks green.
**Fail action:** open a SEV-2 ticket; do not proceed with re-signing.

### Step 3 — Re-walk the 24 SAQ-A questions

- [ ] Open the previous year's SAQ-A doc
      (`specs/_compliance/PCI-DSS-SAQ-A-YYYY-MM-DD.md`).
- [ ] For each of Q1–Q24, confirm the answer is still **YES** (or **N/A**
      with the same justification). If any answer flips to **NO**, that
      is a SEV-2 finding; stop recertification until remediated.
- [ ] Update the responsibility matrix (§Q10) if Stripe has published a
      new Section 2g matrix in the year.
- [ ] Update the SOC 2 cross-map cells if SOC 2 control IDs have shifted
      (e.g., Type II rollout introduced new criterion-points).

### Step 4 — Refresh the boundary diagram

- [ ] Open `specs/_compliance/PCI-DSS-BOUNDARY-DIAGRAM.md`.
- [ ] Re-confirm the checkout flow Mermaid + ASCII diagrams reflect
      current code (`crates/corelink-stripe-real/`,
      `crates/corelink-billing-stripe/`, `crates/corelink-billing-aggregator/`).
- [ ] Re-confirm the storage map matches the current D1 migration set.
- [ ] If anything changed, bump the diagram doc to `v1.1.0` (or higher)
      and add a changelog row.

### Step 5 — Write the new SAQ-A doc

- [ ] Copy `PCI-DSS-SAQ-A-YYYY-MM-DD.md` to
      `PCI-DSS-SAQ-A-{new_assessment_date}.md`.
- [ ] Update the header: assessment date, version (`+1.0.0` major if
      eligibility shifted, else `+0.1.0`), validity window, assessor
      names.
- [ ] Update each question's "Evidence" sub-bullet with the new dates and
      file references.
- [ ] Mark the previous year's doc with `superseded_by: <new id>` in its
      YAML header.
- [ ] Verify the validator: `python3 scripts/validate_specs.py | tail -2`.

### Step 6 — Customer-facing trust page

- [ ] Update `apps/docs/docs/trust/pci-dss.mdx` with the new assessment
      date in the front-matter `last_updated` field + the body's
      attestation statement.
- [ ] Re-verify cross-links from
      `apps/docs/docs/trust/compliance.mdx` §PCI DSS.

### Step 7 — Sign and archive

- [ ] CEO and CTO sign the new SAQ-A's signature block.
- [ ] Archive the prior year's signed SAQ-A as a PDF in
      `specs/_audits/` with a stable filename
      `SAQ-A-signed-YYYY-MM-DD.pdf`.
- [ ] Update `specs/_compliance/SOC2-EVIDENCE-ROLLUP-…md` with the new
      SAQ-A reference (so SOC 2 auditors see the recert closed).
- [ ] Add a row in `specs/_compliance/VENDOR-RISK-REGISTER.md` change
      log: "Annual SAQ-A re-attested YYYY-MM-DD".
- [ ] Set the next-year calendar reminder for **T-30 d before next
      validity expiry**.

### Step 8 — Communicate

- [ ] Send a one-line note to `trust@humangr.com` distribution list:
      "SAQ-A re-attested YYYY-MM-DD; next due YYYY-MM-DD."
- [ ] Update the public trust page (already covered in Step 6).
- [ ] Notify procurement teams of the 5 largest customers (Drata
      contacts module → "PCI-relevant customers" filter).

---

## 3. Off-cadence checks (continuous; not annual)

In addition to the once-a-year walk-through, the following are continuous
guards. They are not part of the recertification per se but they keep the
recert easy by surfacing drift early.

| Check | Frequency | Mechanism | Owner |
| --- | --- | --- | --- |
| Stripe AOC expiry monitor | Daily | Drata vendor-module alert at AOC T-60 d | Security Lead |
| D1 schema diff for CHD columns | Per PR | `scripts/verify-no-chd-schema.py` in CI (to be authored as part of first recert; tracked as backlog) | Tech Lead |
| CSP regression on checkout page | Every 5 min | Cloudflare Healthchecks synthetic | SRE on-call |
| Logpush redaction regression | Per PR | `pii_redaction_100k_synthetic` property test | CI |
| Stripe Connect / new integration mode | At design time | Any new billing surface PR triggers PCI-recert label review | Security Lead |
| New script on checkout page | Per PR | CSP policy file change requires Security-Lead review | Security Lead |
| Visa / MC SP registry drop | Weekly | Drata vendor module | Security Lead |
| IR scenario TT-03 (Stripe webhook compromise) | Quarterly | `IR-TABLETOP-SCHEDULE-2026.md` | IR program |

---

## 4. Sign-offs required per recertification

| Role | Sign-off needed | Cadence |
| --- | --- | --- |
| Security Lead | YES | Per recert |
| CTO | YES (signature on SAQ-A) | Per recert |
| CEO | YES (signature on SAQ-A) | Per recert |
| DPO | Optional (witness) | Per recert |
| Auditor (Schellman) | NO (SAQ-A is self-assessed; auditor sees it as evidence only) | — |

> Until the dedicated Security-Lead and DPO recruits close (R5-8),
> Gustavo Schneiter executes both founder roles. This is disclosed
> transparently to external auditors per the SOC 2 evidence rollup.

---

## 5. Cross-links

- Parent SAQ-A: `specs/_compliance/PCI-DSS-SAQ-A-2026-05-15.md`
- Boundary diagram: `specs/_compliance/PCI-DSS-BOUNDARY-DIAGRAM.md`
- Compliance matrix: `specs/03_architecture/compliance_matrix.md`
- Vendor risk register (Stripe row): `specs/_compliance/VENDOR-RISK-REGISTER.md`
- LGPD annual cadence sibling (for pattern reference): `specs/_compliance/LGPD-DPO-MONTHLY-CHECKLIST.md`
- IR scenario TT-03 (Stripe webhook compromise): `specs/_compliance/ir-scenarios/TT-03-stripe-webhook-compromise.md`
- Stripe credential rotation runbook: `specs/_runbooks/RB-STRIPE-CREDENTIAL-ROTATION.md`
- Customer-facing trust page: `apps/docs/docs/trust/pci-dss.mdx`
