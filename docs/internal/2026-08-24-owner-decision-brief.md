# Owner decision brief — 2026-08-24

Seven open backlog items cannot be closed by the tech lead, for three different
reasons: some need a credential only the owner can mint, some need a policy call
whose cost lands on the business, and one needs a contract decision. They are
collected here so they can be answered in one sitting instead of seven
interruptions.

Each entry states **what is true today** (measured, not assumed), **what the
options cost**, and **what happens if the answer is "not now"** — because
deferring is a legitimate answer and should be a recorded one rather than a
silence.

---

## 1. B-013 — three private keys sitting in `~/Downloads`

**Today.** Three private keys are in the owner's Downloads folder. Nothing in
this repo can move, read or verify them; a tech lead touching private key
material is the wrong shape regardless of intent.

**Options.**
- **Move to a password manager or hardware token and delete the copies.** Cost:
  minutes. This is the recommendation.
- Leave them and accept that any process with the owner's user account — every
  npm postinstall, every VS Code extension — can read them.

**If deferred:** the exposure is not theoretical. The CI runners execute
third-party code on this same machine, under this same account, several times an
hour.

---

## 2. B-012 — a non-Actions credential so bot PRs get CI

**Today.** A PR opened by `GITHUB_TOKEN` does not trigger workflows. That is
GitHub's loop-prevention rule and cannot be configured away. Consequences already
observed: Dependabot PRs arrive with **zero checks**, which is indistinguishable
from "all checks passed" at a glance, and the fleet-health census in
`scripts/check_runner_fleet.py` runs with its slot check **explicitly disabled**
because listing self-hosted runners needs repository admin that `GITHUB_TOKEN`
cannot hold.

**Options.**
- **A fine-grained PAT** (or GitHub App installation token) with `administration:
  read` + `actions: read` + `contents: write`, stored as a repo secret. Cost: one
  token, 90-day rotation. Unblocks bot-PR CI *and* the fleet slot census with a
  one-line change each.
- Leave it: keep reading Dependabot PRs by eye and keep the census off.

**If deferred:** the missing slot that ran undetected for ~14 days
(`corelink-builder-1`, B-030) stays undetectable by CI; only a human noticing the
count catches the next one.

---

## 3. B-008 — PagerDuty accepts our events; nobody knows if they reach a human

**Today.** The Events API accepts what we send. **Nothing has been observed
arriving at a phone.** `crates/corelink-slo/src/pagerduty.rs` is a trait plus an
in-memory test sink — there is no live dispatcher wired to a routing key.

**Why it blocks something concrete.** B-007 (the near-ceiling `$`-spend warning)
is a bare `tracing::warn!` today. Routing it "to a paging sink" is only worth
doing if that sink is proven to wake somebody; otherwise it manufactures the
exact defect it is meant to fix — an alert that goes nowhere, but now with a
green checkbox next to it.

**Options.**
- **Send one test event end to end and confirm the phone buzzes.** Cost: minutes
  plus a routing key. Then B-007 becomes a small, honest change.
- Decide that paging is out of scope pre-launch and say so — in which case B-007
  should route to a durable queryable sink instead, and the alert rules that
  carry PagerDuty routing labels should say they are aspirational.

**If deferred:** every alert rule in `dashboards/alerts/` that carries a
PagerDuty routing label is an unverified promise.

---

## 4. B-009 — the seven-year Object-Lock drain is a stub

**Today.** GDPR erasure is live and proven. The **retention** half is not: no
stored `retain_until`, no Governance/Compliance mode flag, no R2 Object-Lock
enforcement. The audit archive is sealed in D1 and R2 but nothing enforces the
7-year floor at the storage layer.

**The decision is a policy one and it is genuinely one-way.**
- **Governance mode** — retention enforced, but an account-level role can
  override. Reversible. Weaker claim to an auditor.
- **Compliance mode** — nobody, including Cloudflare and us, can delete before
  the term expires. That is the strong claim, and it is **irreversible**: a
  misconfigured retention window cannot be shortened, and the bytes are billed
  for the full term.

**Recommendation:** Governance now, Compliance at the point a customer contract
actually requires it. Locking bytes for seven years before the first paying
customer is an expensive way to make a claim nobody has asked for yet.

**If deferred:** SOC 2 CC-series evidence and the DSR work item (WI-S11-008) stay
open on their last piece, and the compliance documents that cite a 7-year
retention control describe an intent rather than a control.

---

## 5. B-031 — SLSA L3 needs a GitHub-hosted builder, which this repo does not spend on

**Today.** `release-slsa3.yml` now takes its subjects from the **published
release binaries**, verified byte-identical against `cli-v0.1.0`'s own
`checksums.txt`. Jobs 1 and 3 run on our own fleet. Job 2 is the
`slsa-github-generator` reusable workflow, and **SLSA L3 is defined by that
builder being GitHub-managed and isolated** — it cannot move to our runners
without dropping below L3. Standing rule: zero GitHub-Actions spend.

**Options.**
- **Allow hosted minutes for release provenance only.** Releases are rare (one
  exists), so the bill is bounded and small. Recommendation.
- Drop to a self-hosted attestation (cosign-signed digests, no isolated builder)
  and state the real level wherever SLSA L3 is currently asserted — which means
  editing the ISO 27001 SoA A.5.21 row and the SOC 2 crosswalk.
- Withdraw the provenance claim entirely, with the same document edits.

**If deferred:** no provenance bundle has ever been produced for anything we
ship, while two compliance documents say L3.

---

## 6. B-032 — every Critical vendor review is past its cadence window

**Today.** All seven Critical vendors — Cloudflare, Stripe, Clerk, AWS, Google
Cloud, Azure, Drata — were last reviewed on the 2026-05-15 baseline against a
90-day cadence. That is **101 days**, 11 overdue, and the register's own header
names 2026-08-15 as the next full refresh.

**Why it is the owner's.** The reviews need Drata and each vendor's current SOC 2
/ ISO evidence. No amount of code closes it.

**Options.** Run the seven reviews; or re-baseline the cadence with a written
reason and a new date.

**If deferred:** an auditor reading the register sees a control with a stated
cadence and no evidence of it being followed, which is worse than a control with
no stated cadence.

---

## 7. B-035 — eight contract lines promise a TLS floor we do not enforce

**Today.** The DPA `v1.0.0` (three locales), the EU SCC annex, the sub-processor
commitments and the three privacy notices all say encryption in transit is
**"TLS 1.3+"**. The `humangr.com` edge floor is **1.2** (ADR-0072), lowered
deliberately so `sccache` and other `native-tls` clients could connect at all.
Everything editorial — docs site, prospect questionnaires, compliance
crosswalks, legal templates — was corrected on 2026-08-24; these eight were not,
deliberately, because correcting versioned contract text is not an editorial act.

**Options.**
- **Publish `v1.0.1`** with the corrected clause and, if anyone has executed
  v1.0.0, give notice. Recommendation.
- **Raise the zone floor back to 1.3** and accept that `sccache` and every other
  `native-tls`/SecureTransport client stops working — this is ADR-0072's own exit
  condition, and it re-breaks a live cache surface.

**If deferred:** the gate holds these eight lines as *tracked* drift — visible on
every run, fatal under `--strict` — so they cannot be forgotten. But a contract
that overstates a security control stays a contract that overstates a security
control.

## 8. B-029 — the load-test regression gate has no staging to fire at

**Today.** The nightly k6 load suite (`load-test-nightly.yml`) is now correct in
every part the tech lead controls: the regression comparison is real (PR #1263),
and the runner is fixed (both jobs moved off the dead `[self-hosted, Linux, X64]`
label onto `corelink`). It still cannot run, for one reason left:
**there is no staging environment.** `staging.corelink.humangr.com` does not
resolve, and the `staging` GitHub environment holds none of the secrets the suite
reads (`K6_TARGET_HOST`, `K6_STAGING_PAT`, and the four scenario secrets). The
pre-flight target-host check fail-closes on the first, every run.

**Options.**
- **Stand up a staging deployment** (a Worker + container pin against a
  `staging.*` hostname, with its own D1/R2) and wire the six `K6_*` secrets into
  the `staging` environment. Then re-enable the nightly cron in the same change.
  This is what turns the suite from correct-but-inert into a real perf gate, and
  it is also the environment every other pre-prod check (endurance, cutover
  smoke) assumes exists. Recommendation, if pre-launch load evidence is wanted.
- **Retire the staging-targeted suites** (`load-test-nightly`, `endurance-2h`)
  and drop the "nightly regression gate" claim, keeping only the dispatch-only
  harness for a human to point at an ad-hoc target. Cheapest; gives up automated
  perf-regression coverage before launch.

**If deferred:** the workflow stays dispatch-only and honestly documents why; its
verify keys on the still-disabled cron, so the item stays visibly open rather
than flipping green on the runner fix alone. No perpetual red — it only runs when
dispatched — but also no standing perf-regression signal.

---

## What the tech lead is doing in the meantime

Nothing here blocks the rest of the campaign. B-007 is the only item that is
*technically* ready and deliberately not built: routing a warning into an
unproven sink would manufacture the defect it is meant to fix, so it waits on
item 3 above. Everything else on the open list is tech-lead-owned and in flight.
