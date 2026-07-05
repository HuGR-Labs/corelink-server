# SPEC → corelink-server / CAS TL — the account-exclusive CAS physical-GC seam (2-part API). This is now the SINGLE HARD pre-launch blocker on the GDPR1 erasure path. Precise contract + ETA ask.

> **From:** clw coordinator · **Relay:** owner · **Date:** 2026-07-05
> Supersedes my `2026-07-04-FLAG-to-server-TL-GDPR1-CAS-GC-...` with the exact API contract hugit needs.
> Grounded: hugit landed the audit fixes (#255, merged); `CAS_GC_SEAM_WIRED=false` → `is_launch_blocked()` fires.

## Why this is now THE blocker (not a disclosure-satisfiable residual)
hugit's erasure planner (audited, merged #255) correctly **splits** the CAS leg into two:
- **`CasShared`** — objects also referenced by another tenant. Cannot be unilaterally deleted (would erase
  another tenant's data). **Disclosure is the honest, correct answer here — endorsed, closed.**
- **`CasGcObligation`** — objects referenced ONLY by the erasing subject's (now-severed) manifests =
  **account-EXCLUSIVE**. Under the owner's no-waiver bar, an exclusive object still `fetch-by-digest`-able
  after `executed` is a **real GDPR erasure gap** — disclosure does NOT satisfy it. It must be physically GC'd.

hugit cannot do this alone: the CAS is CoreLink-owned and content-addressed/cross-tenant-deduped. hugit knows
*which digests* the subject referenced; only the CAS owner can answer *"is this digest exclusive?"* and *delete it*.

## The 2-part API hugit needs the CAS/server side to expose

### (1) Reachability / exclusivity check
> Given a **content digest** + the **erasing tenant**, is the object referenced by ANY surviving manifest of
> ANY *other* tenant?
- **account-EXCLUSIVE** ⇔ referenced only by the subject's manifests (which are being tombstoned/severed).
- Must be **conservative / fail-closed**: if exclusivity cannot be proven (indeterminate index), treat as
  **shared** (do NOT delete) — never delete an object that might be another tenant's. (Over-retention is a
  disclosed residual; over-deletion is cross-tenant data loss — asymmetric, so bias to retain.)

### (2) Physical GC / delete of an account-exclusive object
> Delete the raw content-addressed bytes so a subsequent `fetch-by-digest` **404s** — not merely an unreachable
> named/manifest path.
- Idempotent (delete-of-already-deleted = ok), so hugit's `executed⇒durable` retry converges.
- Scoped to digests (1) classified exclusive; a shared digest is never passed here.

## Integration shape (how hugit consumes it — no ambiguity)
When this seam lands, hugit flips `CAS_GC_SEAM_WIRED=true`, and the executor: enumerates the subject's referenced
digests → calls (1) to partition shared vs exclusive → calls (2) on the exclusive set → only then appends
`erasure.executed` (D3 fail-closed ordering: no `executed` unless every exclusive object physically 404s). Then it
re-points me at the executor PR and I **re-audit** against the checklist item *"the executor physically GCs
account-exclusive objects (fetch-by-digest 404 after)."*

## The ask
1. **Confirm ownership** of this seam (CAS/server side) — it is not hugit-closable.
2. **ETA** for (1) + (2) — it gates the GDPR1 executor's completeness, which is a **hard pre-launch gate**
   (owner: no waiver on the only irreversible-delete path).
3. Flag any coupling to the cf-multitenant work so we sequence it (it's independent of the mint seam, but shares
   your plate).

This is the last open dependency on the erasure path; everything else (B1 under-erasure, B2 identity) is closed
and verified. Send the ETA and I'll track it to the executor gate.

— clw coordinator
