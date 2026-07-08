# RUNBOOK (for owner) — the only thing left is prod-cred deploys. Vetted one-shots, in order. Nothing here is code — the code is landed/PR-ready.

> **By:** corelink-server TL · **Date:** 2026-07-07 · run these when you want to go live. Each is minimal-impact + verify-gated.

## A) Close GDPR (independent of the product launch — do this any time)
The `dsr_anchor` worker fix is **already on `main`** (#657 merged). Two prod steps:

**A1 — deploy the worker** (ships the `dsr_anchor` consumer):
```bash
# from a clean main
gh workflow run cf-deploy-prod.yml --ref main    # OR your usual worker-deploy trigger
# confirm the run goes green; this deploys corelink-prod's worker from main (has #657)
```

**A2 — re-tag recycle the container** (so its OWN anchor gate picks up the bound key — RE-TAG, not rebuild, per clw):
```bash
# re-tag the EXISTING running image to a fresh tag (same digest → no new code ships):
#   204c4832 -> 204c4832-r2   (use your registry re-tag path; do NOT run container-build-push)
# repin wrangler.toml [[env.prod.containers]] image to :204c4832-r2, then:
bash scripts/deploy-container-prod.sh --apply --env prod
#   polls the CF Containers API until running image == the pin. Exit 0 = VERSION-82 instances recycled.
```
**A3 —** ping clw "both live" → clw greens githugr → githugr's single anchor call → **200** → hugit erase → **410 Gone**. GDPR live.
> Bind is DONE — do NOT re-bind the secret. Do NOT repin the container to HEAD (would ship un-merged container code).

## B) Product launch (your order: #655 → #654 → #656)
Each merges to `main`, then deploys. All three are green (only documented flakes: cargo-fuzz-Mac, Sentry critical-flows — both infra, not code).

**B1 — #655 backend** (runners/workspaces/deep-dive):
```bash
bash scripts/pre-merge-gate-check.sh 655      # expect all-green (fuzz flake = documented)
gh pr merge 655 --merge --delete-branch
# then deploy the CONTAINER (real backend deploy):
bash scripts/check-container-pin-fresh.sh     # if stale, repin to HEAD first (this IS a code deploy)
# build+push the new container image (container-build-push-prod.yml on main), repin wrangler.toml, then:
bash scripts/deploy-container-prod.sh --apply --env prod
```

**B2 — #654 FE** (13 screens + runners/workspaces un-stubbed):
```bash
bash scripts/pre-merge-gate-check.sh 654      # Sentry critical-flows flake = documented
gh pr merge 654 --merge --delete-branch
# admin-ui AUTO-deploys on merge to main (admin-ui-deploy.yml fires on apps/admin-ui changes) — watch that run.
```

**B3 — #656 metering** (ROI): merge after #655's container is live (the ROI reads the new usage_daily surface).
```bash
bash scripts/pre-merge-gate-check.sh 656
gh pr merge 656 --merge --delete-branch
# rides the next container deploy (usage_meter + migration 0089 are container-side).
# NOTE: #656 uses migration 0089 assuming #655's 0088 landed first — merge #655 before #656.
```

## Notes
- **I can't run any of these** (prod-cred + I don't run prod deploys, esp. from feature branches). They're yours or the clw coordinator's (owner-authorized).
- **Migration order:** #655 (0088_workspaces) MUST land before #656 (0089_usage_daily) — the B-order handles it.
- **Live keys** (Clerk/Stripe prod) are your launch-day flip — separate from these deploys.
- Say the word on any step and I'll tighten the exact command for your setup (registry re-tag path, worker-deploy trigger name).

— corelink-server TL
