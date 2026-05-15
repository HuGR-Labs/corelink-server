---
id: "TT-05-SUPPLY-CHAIN"
type: "compliance_scenario"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R-5"
parent_wi: "WT-GAP-03"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["compliance", "ir", "tabletop", "scenario", "sev1", "supply-chain", "cargo-audit", "dependency", "cosign", "sbom", "gap-03", "wt-gap-03"]
---

# TT-05 — Supply-Chain Compromise (cargo-audit Fires on Transitive Dep with Active Exploit)

> **Severity:** SEV1 · **Duration:** 90 min · **Quorum:** IC + Scribe + Comms Lead + Tech Lead + Security Lead (5 roles). Customer Comms Lead joins by T+45 if exploit confirms customer-data exposure.
>
> **Parent:** `specs/_compliance/IR-TABLETOP-PLAYBOOK.md` · **Evidence form:** `specs/_compliance/templates/IR-TABLETOP-EVIDENCE.md` · **Schedule:** Q1-2027 (target 2027-02-04).
>
> **FM linkage:** **FM-156** (dep maintainer malicioso, supply chain attack) + **FM-007** (deserialize RCE, secondary if exploit gives code execution) + **FM-253** (cross-tenant read, downstream if compromised dep is in hot path).
> **CTRL coverage:** CTRL-SUPPLY-001..005 (SBOM signed + Cosign verification + INV-SUPPLY-NO-YANKED + immutable Workers) + CTRL-DEP-AUDIT-001 (cargo-audit gate).
> **RB-* invoked:** `RB-FM-156-dep-maintainer-malicioso` (canonical, semestral) · `RB-SECURITY-VULNERABILITY-INTAKE` · `RB-PENTEST-FINDING-RESPONSE`.

## Scenario summary

It is **09:34 UTC on a Tuesday**. The morning CI run on the `main` branch fails: `cargo-audit` reports a CRITICAL advisory against `quick-prng-0.4.7`, a transitive dependency (depth: 3) pulled in through `corelink-cas` → `fast-hashbench` → `quick-prng`. The RustSec advisory `RUSTSEC-2027-0014` says:

> *"Versions 0.4.5–0.4.8 contain a malicious payload introduced by maintainer takeover. The payload exfiltrates environment variables matching `^(AWS|GCP|CF|API|TOKEN|SECRET|KEY)_` to a remote HTTP endpoint at `quick-prng-metrics.<TLD>` on first use after process start. CVE-2027-2014. Active in the wild since 2027-01-29."*

Active in the wild for **6 days**. Current deployment to staging happened **2 days ago** with `quick-prng-0.4.7` in the lockfile. Production is **NOT yet deployed** (still on previous lockfile with `quick-prng-0.4.4`, which is unaffected).

Initial Tech Lead glance — staging is potentially compromised since 2 days ago. Worker isolates run for ≤30s before recycling, but the malicious code runs **on first use after process start** — so every cold-start (hundreds per minute in staging) could have leaked secrets.

The team must:
1. Determine blast radius — what secrets are accessible in the staging Worker environment? (Spoiler: staging BYOK test keys, Stripe test keys, Clerk staging keys, Drata staging API key — all rotatable, but the team must enumerate.)
2. Roll back staging immediately + pin lockfile to known-good version.
3. Rotate every secret that was in staging environment.
4. Communicate to customers: does the lighthouse staging tenant count as customer data? (Yes — lighthouse customers have staging accounts under MSA.)
5. Decide on a customer-trust posture: this is a **near-miss for production**. Do we publicly disclose, even though production was never affected?

## Pre-tabletop preparation

### Facilitator-localised anchors (T-1 week)
- Identify a real recent dependency in the lockfile that could plausibly host this scenario — pick one with depth 3+ for realism. Use `cargo tree` to confirm.
- Have the staging deployment timeline ready (last 3 deploy timestamps).
- Pull a real `cargo-audit` advisory format sample to make Inject 1 read authentic.

### Participants briefed (T-24h Slack post template)
> Tabletop **TT-05** runs **Wed 2027-02-04 14:00 UTC** on Zoom `<link>`. Scenario: supply-chain compromise — cargo-audit fires on transitive dep with active exploit. Roles: IC=`<name>`, Tech Lead=`<name>`, Security Lead=`<name>`, Comms Lead=`<name>`, Scribe=`<name>`. Customer Comms Lead on standby. **Tabletop only.** Bring RB-FM-156, SBOM/Cosign refs, secrets-rotation runbooks.

## Injects (timeline)

### Inject 1 — T+00:00 (Detection)
> *"It is 09:34 UTC. Morning CI on main just failed. cargo-audit reports CRITICAL RUSTSEC-2027-0014 against `quick-prng-0.4.7`, a transitive dep (depth 3) via corelink-cas → fast-hashbench → quick-prng. Advisory text: 'maintainer takeover; payload exfiltrates env vars matching ^(AWS|GCP|CF|API|TOKEN|SECRET|KEY)_ to remote endpoint on first use after process start. Active in the wild since 2027-01-29.' Staging was deployed with this lockfile 2 days ago. Production is NOT yet deployed (still on quick-prng-0.4.4 in lockfile). Go."*

**Expected:** IC declares SEV1 (potential compromise of staging secrets); opens `#inc-2027-02-04-supply-chain`; pages Security Lead.

### Inject 2 — T+8:00 (Analysis, blast radius)
> *"Tech Lead, your job in 10 minutes: enumerate every secret accessible in the staging Worker environment that would match the exfil regex `^(AWS|GCP|CF|API|TOKEN|SECRET|KEY)_`. Pull from wrangler.toml + CF secrets manager + Worker env bindings. The list will drive rotation prioritisation."*

**Expected enumeration (realistic):**
- `AWS_KMS_KEY_STAGING` (BYOK test key, AWS provider)
- `GCP_KMS_KEY_STAGING` (BYOK test key, GCP provider)
- `CF_API_TOKEN_STAGING` (Cloudflare API token for staging Workers + R2 + KV)
- `API_KEY_STRIPE_TEST` (Stripe test API key)
- `API_KEY_CLERK_STAGING` (Clerk staging publishable + secret keys)
- `API_KEY_DRATA_STAGING` (Drata staging API)
- `API_KEY_HUBSPOT_STAGING` (HubSpot CRM staging)
- `API_KEY_POSTHOG` (PostHog product analytics)
- `TOKEN_GITHUB_DEPLOY` (deploy-time only, may not be at runtime — Tech Lead must confirm)
- `SECRET_JWT_SIGNING_STAGING` (JWT signing key for staging)
- `SECRET_PAT_HMAC` (PAT-issuance HMAC key)

**Decision point D-1:** confirm enumeration is complete vs. accept residual risk. If a secret is missed, downstream rotation won't cover it.

### Inject 3 — T+22:00 (Containment, decision pressure)
> *"You have enumerated 11 secrets. Now: (a) rotate all 11 immediately (mass rotation, will require touching 11 different vendor consoles + restarting all staging services — ~2h wall clock); (b) rotate the 5 most critical first (BYOK keys + CF API + Clerk) within 30 min, then rotate the rest within 4h; (c) **freeze staging entirely** (kill all Workers; cut external egress at CF edge) + rotate at leisure. Tech Lead, recommendation? IC, decision?"*

**Decision point D-2:** Containment posture — mass-rotate / prioritised-rotate / freeze. Tradeoffs:
- (a) thorough, slow, customer-visible (staging unavailable to lighthouse customers during rotation).
- (b) prioritises high-blast-radius secrets, accepts residual risk on lower-priority for ~4h.
- (c) strongest containment but breaks staging tests + lighthouse staging access.

### Inject 4 — T+38:00 (Eradication + supply-chain hardening)
> *"Containment decision made + executed (simulated). Now eradication: (i) pin lockfile to known-good — `quick-prng = "=0.4.4"` (the last unaffected version); (ii) **but**: a yank policy concern — we should also confirm quick-prng-0.4.4 isn't also affected (advisory says 0.4.5–0.4.8, but did the maintainer takeover happen BEFORE that range? Trust nothing.); (iii) consider full eradication: remove fast-hashbench → quick-prng dependency chain entirely (requires forking fast-hashbench or finding an alternative — 1–2 sprint of work). Security Lead, what is the eradication threshold for SOC 2 + customer-trust posture?"*

**Decision point D-3:** Eradication depth — pin to known-good (fast, accepts residual maintainer-trust risk) vs replace the entire dep chain (slow, strongest). Reference INV-SUPPLY-NO-YANKED + SBOM signing posture.

### Inject 5 — T+58:00 (Customer-comms decision; near-miss disclosure)
> *"It is 10:32 UTC. Containment + eradication in progress. Production was NEVER deployed with the compromised lockfile (CTRL-SUPPLY-002 Cosign verification + lockfile-pin discipline saved us). But: lighthouse customers have **staging access** under their MSA — their staging tenants ran on Workers with leaked-secret risk for 2 days. Comms Lead + Customer Comms Lead + IC: what is the disclosure posture? Options: (a) silent — no public disclosure since production was unaffected; (b) lighthouse-only disclosure — the 3 lighthouse customers under MSA staging clauses; (c) public postmortem + lighthouse-direct disclosure — frame as 'transparency about a near-miss'."*

**Decision points D-4 / D-5:**
- D-4: disclosure scope.
- D-5: framing — if (c), the public postmortem becomes a **brand-positive** story IF told well ("our SBOM + Cosign discipline caught this before production") and a brand-negative story if told poorly ("we shipped malware to staging for 2 days"). Comms Lead drafts the actual paragraph live.

### Inject 6 (optional) — T+85:00 (Lessons + post-incident structural change)
> *"Action-item brainstorm: (i) cargo-audit on every PR + every nightly + every CI run (already in place — verify); (ii) lockfile-pin discipline for transitive deps in security-critical chains; (iii) `cargo-vet` adoption for higher trust assurance on transitive deps; (iv) staging-secrets fully separated from prod (some currently shared like POSTHOG); (v) RUSTSEC advisory subscription with PD-page on CRITICAL. Rank by impact + effort."*

## Decision points summary

| # | Decision | Owner | NIST phase | Reversibility |
|---|---|---|---|---|
| D-1 | Secret-enumeration completeness | Tech Lead + Security Lead | Detection | reversible |
| D-2 | Containment posture (mass-rotate / prioritised / freeze) | IC + Tech Lead | Containment | reversible (can escalate) |
| D-3 | Eradication depth (pin / replace dep chain) | Tech Lead + Security Lead | Eradication | reversible (replace can come later) |
| D-4 | Disclosure scope (silent / lighthouse / public) | IC + Comms Lead + Customer Comms | Recovery / Post-Incident | (public) **irreversible** |
| D-5 | Disclosure framing | Comms Lead | Recovery | reversible (drafts) but irreversible once published |

## Comms templates

### Internal Slack post (T+5)
```
@channel — SEV1 incident
Channel: #inc-2027-02-04-supply-chain
Symptom: cargo-audit CRITICAL on quick-prng-0.4.7 (transitive dep). Staging deployed 2d ago with this lockfile. Prod NOT deployed.
IC: <name> / Tech Lead: <name> / Security Lead: <name> / Comms Lead: <name> / Scribe: <name>
Hypothesis: maintainer takeover, secrets exfil from Worker env on cold-start.
Customer-visible: TBD (lighthouse staging access in scope).
Status page: holding pending decision.
Update cadence: every 15 min.
```

### Status-page post (T+90, IF disclosure path c chosen)
```
[POSTMORTEM] On 2027-02-04 at 09:34 UTC, our continuous dependency-audit pipeline detected a CRITICAL vulnerability (RUSTSEC-2027-0014) in a transitive dependency present in our staging environment. The vulnerability — a maintainer-takeover supply-chain attack — was caught BEFORE deployment to production thanks to our pre-deploy signed-SBOM verification (Cosign + Rekor) and our lockfile-pin discipline.

Impact assessment:
- Production: NOT AFFECTED. Production deployments are gated on signed-SBOM verification, which would have rejected the malicious version.
- Staging: potentially affected for ~48 hours. We have rotated all 11 secrets accessible to the staging environment and confirmed no anomalous outbound traffic from staging Workers during the window.
- Customer data in production: not at risk.
- Customer data in staging (lighthouse-tenant test data): rotated all credentials; reviewing for any indicators of access.

What we are doing:
- Permanently pinning quick-prng to a known-good version pending dep-chain replacement (in progress, est. 2 sprints).
- Subscribing to RUSTSEC advisory feed with PagerDuty alerts on CRITICAL.
- Adding `cargo-vet` for transitive-dep trust assurance.

Full technical postmortem within 7 days at <URL>.
```

### Lighthouse customer email (T+75, direct)
```
Subject: [INFO] CoreLink supply-chain near-miss affecting your staging environment
Body:
Hi <CISO name>,

We detected and remediated a supply-chain vulnerability (RUSTSEC-2027-0014, CVE-2027-2014) in a transitive dependency that was present in our staging environment from <date> to <date>. Production was never affected — our pre-deploy signed-SBOM gate (Cosign) blocked the vulnerable version from progressing.

Impact on your staging tenant:
- Your staging tenant's data: <no customer-data exposure detected; full forensic review in progress>
- Staging credentials shared with you (PATs, API keys): we recommend rotating any staging credentials you provisioned in your CoreLink staging tenant. Production credentials are unaffected.
- Timeline: we will provide a final forensic report within 7 days.

Why we're telling you:
Your MSA includes a transparency clause on supply-chain near-misses (§5.4). We are honouring that proactively even though no customer data was confirmed exposed.

— CoreLink Security Lead (<name>)
```

### Postmortem doc seed (T+90, draft into `specs/_postmortems/`)
```
Title: 2027-02-04 supply-chain near-miss — RUSTSEC-2027-0014 (quick-prng)
Severity: SEV1 (near-miss for production)
Duration: 09:34 → <resolution> UTC
Root cause: maintainer takeover of `quick-prng` crate; malicious payload exfiltrates env vars on first use after process start.
Impact: 11 secrets accessible to staging Worker environment potentially exposed for ~48h; production NOT affected.
Detection: morning CI cargo-audit run.
Containment: <chosen path>
Eradication: pinned to 0.4.4; full dep-chain replacement scheduled (2 sprints).
Recovery: secrets rotated; staging redeployed on pinned lockfile.
What worked: pre-deploy signed-SBOM gate (Cosign + Rekor) prevented production deployment; cargo-audit caught within 48h of advisory publication.
What didn't: shared staging↔prod secrets (POSTHOG); 48h window from advisory-publish to detection; subscription gap to RUSTSEC.
Action items: <listed>
```

## Post-incident artefacts list

- [ ] cargo-audit output (full advisory text, pinned to commit)
- [ ] Secret-enumeration list (11 items, with rotation timestamps)
- [ ] SBOM diff (before/after lockfile pin)
- [ ] Cosign/Rekor verification log (proving production never accepted the malicious version)
- [ ] Outbound-traffic forensics for staging Worker environment over 48h window
- [ ] Lighthouse customer emails sent (3)
- [ ] Postmortem doc (sealed within 7d)
- [ ] Action items into Linear epic `IR-TT-2027-Q1-01`
- [ ] Updates to `RB-FM-156-dep-maintainer-malicioso.md` if gap surfaced
- [ ] CTRL-SUPPLY-001..005 review proposals

## Success criteria

1. **SEV1 declared** within 5 min.
2. **Secret enumeration** completed within 15 min with named secrets.
3. **Containment posture** chosen with reversibility tag.
4. **Eradication depth** explicitly tradeoff-discussed (pin vs replace).
5. **Customer-disclosure scope + framing** drafted, with explicit reference to MSA §5.4 (lighthouse transparency clause).
6. **CTRL-SUPPLY-001..005 referenced** in Tech Lead + Security Lead discussion (proves SBOM + Cosign discipline is real).
7. **At least 4 action items** captured (supply-chain rehearsals consistently surface multiple gaps).

## Failure modes during exercise

- Team misses POSTHOG (or another) in secret enumeration → action item: automated secret-discovery on Worker bindings.
- Team chooses "silent" disclosure → Customer Comms Lead pushes back ("MSA §5.4 transparency clause"); if no pushback, facilitator flags as gap.
- Team forgets `cargo-vet` / advisory-subscription / staging↔prod secret separation → all become action items.
- Team prematurely concludes "production was never at risk" without reviewing why CTRL-SUPPLY-002 specifically blocked it → action item: document the proof chain.

## Cross-references

- `specs/_compliance/IR-TABLETOP-PLAYBOOK.md`.
- `specs/05_quality/runbooks/RB-FM-156-dep-maintainer-malicioso.md` (canonical, semestral cadence).
- `specs/05_quality/runbooks/RB-FM-156-dep-malicious.md` (variant).
- `specs/_runbooks/RB-SECURITY-VULNERABILITY-INTAKE.md`.
- `specs/_runbooks/RB-PENTEST-FINDING-RESPONSE.md`.
- `specs/03_architecture/failure_modes.md` FM-156, FM-007, FM-253.

## Controls evidenced

CC7.3 · CC7.4 · CC7.5 · CC6.8 (change management — signed-SBOM gate as primary control) · plus CTRL-SUPPLY-001..005, CTRL-DEP-AUDIT-001.

External frameworks: NIST 800-161 (supply-chain risk management), SLSA L3 alignment, ISO/IEC 27035-1 §6–7.
