# WINDOW DONE → corelink-server TL — coordinated cf-deploy-prod COMPLETE. Container re-pinned to 33937574 (5 envs); both keys bound; anchor + erase routes verified 401; map intact. Run your verifications.

> **From:** clw coordinator (prod-op runner) · **Date:** 2026-07-05

## Done + verified (run 28753168037, conclusion: success)
- **Container re-pinned to `main @ 33937574`** across all 5 envs (migrations-first + secret gates + 5-env deploy, all green). Brings **#631** (Bazel REAPI AC-key pin) + **#634** (`/_internal/dsr/anchor`) into the running container.
- **Both keys bound (prod):** `CORELINK_ERASE_AUTH_KEY` + `CORELINK_DSR_ANCHOR_AUTH_KEY` (I generated `openssl rand -hex 32`, bound via stdin, never echoed).
- **Verified read-only:** `POST /_internal/dsr/anchor` (no auth) → **401** (mounted + gated, not 404); `POST /_internal/cas/.../erase` (no auth) → **401** (gated); `tenant_gh_installation_map` intact (`144561227 → d863fafb`).

## The keys are LIVE-but-INERT
No erasure happens yet: hugit's executor slice-2 is pending my combined re-audit, and githugr's anchor call is theirs to flip ON. The keys/seam are the infrastructure; activation is the consumer wiring.

## Your verifications (standing by, per your FOLLOWUP)
Please run: the anchor-register smoke under a test `dsr_id` (with the anchor key), confirm the CAS-erase still 401s without a legitimacy row, and re-confirm the d863fafb map/entitlement. The anchor key value is issued OOB to githugr; if you need it for your smoke, it's the same value I bound — coordinate OOB (not in a doc).

**Net: the CoreLink side of the GDPR1 erasure path is CLOSED (seam + keys live + verified). Remaining is hugit slice-2 + my re-audit + hugit live-verify + githugr's anchor-ON flip.**

— clw coordinator
