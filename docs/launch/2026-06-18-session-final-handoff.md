# Session final handoff — launch-hardening + remaining-tail attack (2026-06-18)

Single referenceable summary of the autonomous launch-hardening run + the owner-directed "attack all remaining" wave. Validation model (owner-ratified): **CI = local-verify on a quiet Mac → `--admin` merge with a documented reason** (self-hosted Linux runner down + shared Mac disk-starved). Every merge below was local-verified + cold-reviewed.

## ✅ Merged (≈15 PRs, #343–#356, #358)
- **Money:** payment-bypass gate — container Stripe-webhook materializer no longer (re-)grants `active` for non-granting status (#344).
- **Auth:** Argon2id per-tenant fairness, two-tier semaphore, deadlock-free + fail-safe (#354); forgery-safe multi-region quota, constant-time secret match (#349).
- **Cache DoS:** Turbo `/events` per-tenant cap + slow-body timeout (#353).
- **#10 OCI cap-on-downgrade — FULLY CLOSED** (#358): resolved cap signed into the `/token` bearer (HMAC-covered → unforgeable), threaded to the byte-accounting reserve.
- **CI honesty:** vitest harness made real + fully green, 0 reds (#345, #350).
- **Observability:** Workers Logs + inert Sentry, ready-to-activate (#351).
- **Hygiene:** scanners→Mac (#343) · OCI-key forward + env-contract gate (#347) · safe-deps consolidation, majors deferred (#348) · docs dead-hosts + generator guard (#352) · migration DR-tooling (#355) · legal-doc hygiene (#356).

## ✅ Prod operations (not PRs)
- **DSR/0069 reconcile:** applied the missing `dsr_requested` table → **GDPR erasure unblocked in prod**; ledger reconciled (0069/0070/0071; 0072 left correctly pending).
- **hugit CAS tenant provisioned:** tenant `d863fafb-17c3-4ec3-92f6-b5a85c27d7bd` + read-write (ingest) + read-only (serve) PATs, round-trip-verified before insert. **Creds: `~/Downloads/hugit-corelink-pats.txt` (mode 600).**
- **Stripe:** leaked `whsec` endpoint confirmed gone; webhook config audited (no live double-writer — money path = the container webhook protected by #344). **Pager e2e:** PagerDuty ingested a test event.

## ✅ git-from-CAS contract — DECIDED (CoreLink Server TL)
Reply doc: **`docs/handoff/2026-06-18-corelink-server-tl-reply-git-from-cas-contract.md`** (relay to the hugit TL). Decision: CoreLink CAS keys = content digest (BLAKE3, 64-hex, verified) — option B (oid→hash index hugit-side; refs manifest in hugit's R2; objects in CAS get dedup + integrity + 410-erase for free). With the tenant+PATs provisioned, the hugit TL can build (c)/(d)/(e) and go live — zero CoreLink code change.

## ⛔ Only-you remaining
- **Stripe hygiene:** tighten the container webhook's 236-event over-subscription · decide the vestigial wallet endpoint (`we_1TJ1YZ`).
- **Legal:** DPO-email decision (`privacy@hugr.dev` vs `humangr.com`) · attorney sign-off · **create `specs/_runbooks/RB-INCIDENT-RESPONSE.md`** (the DPA cites it but it doesn't exist) · commit the `docs/compliance/vendor-reviews/` evidence.
- **Observability activation:** deploy + set `SENTRY_DSN` (+ optional Logpush sink).
- **Confirm:** the pager actually reached your phone/email · (optional) a live DSR-erasure e2e.

## Pointers
- Roadmap: `docs/launch/2026-06-18-go-live-sota-roadmap.md`
- git-from-CAS reply: `docs/handoff/2026-06-18-corelink-server-tl-reply-git-from-cas-contract.md`
- hugit creds: `~/Downloads/hugit-corelink-pats.txt`
- Memory: `launch-hardening-campaign-2026-06-18` · `stripe-webhook-duplicate-endpoint` (corrected)
