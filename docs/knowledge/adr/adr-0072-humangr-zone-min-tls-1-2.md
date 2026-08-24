---
type: "ADR"
title: "ADR-0072 — humangr.com zone min_tls_version lowered 1.3 → 1.2 for sccache"
description: "Why the edge TLS floor is 1.2 and not 1.3: a 1.3-only zone silently rejected sccache clients on macOS SecureTransport, breaking a live cache surface. Records what was given up, the exit condition that is not currently measurable, and the drift risk of a security setting that lives only in a vendor dashboard."
source_files:
  - "specs/03_architecture/adrs/ADR-0072-humangr-zone-min-tls-1.2.md"
  - "specs/03_architecture/security_model.md"
  - "scripts/check_tls_floor.py"
checkpoint_sha: "6e334917a4308b0c5e5cb24679356a7461537b1d"
provenance: "AUTHORED"
tags: ["adr", "security", "tls", "cloudflare", "zone-config", "sccache", "drift-risk"]
timestamp: "2026-08-23T00:00:00Z"
---

# ADR-0072 — `humangr.com` zone `min_tls_version` lowered 1.3 → 1.2

The edge TLS floor for `humangr.com` is **1.2**, not 1.3. A 1.3-only floor rejected `sccache`
clients outright — `sccache` on macOS links `native-tls`/SecureTransport, which negotiated 1.2
and was refused at the handshake, so a live cache surface simply stopped working for those users.
The setting was lowered by a manual Cloudflare API call on 2026-07-19 and confirmed still live on
2026-08-23 (`min_tls_version = "1.2"`, zone `73f57f6d508beea67e2f78bea11d3c25`).

# Context

The decision itself is unremarkable — a client surface needs 1.2, so the floor is 1.2. What makes
it worth a concept is everything around it.

It shipped with **no paper trail**: no ADR, no infrastructure-as-code entry, only a line in an
informal e2e ledger. There is no Terraform or equivalent surface for zone settings at all
(`infra/terraform/modules/cloudflare-base/` owns other zone-level Cloudflare resources and would
be its home if one were built), so the value exists **only** in the vendor dashboard.

It also left a **stale control claim** behind, since closed. `specs/03_architecture/security_model.md:254` listed
CTRL-CRYPTO-001 as "TLS 1.3 only" with an SSL Labs A+ as its evidence, and the claim had spread to
25 places across 13 compliance documents, both INV-CONF-IN-FLIGHT rows, the compliance matrix and
the sprint-creation contract — which was still ordering every future sprint to refuse TLS < 1.2's
successor. All of it now states the 1.2 floor (BACKLOG B-019); the two FROZEN/AUDITED compliance
documents keep their audited bodies under a dated errata. The cited SSL Labs A+ scan predates the
floor change and the control now says so rather than implying otherwise.

# Decision

- The floor is **TLS 1.2** on `humangr.com`, accepted deliberately rather than tolerated.
- What was given up is real and is stated rather than waved at: 1.2 permits cipher suites and a
  handshake shape 1.3 does not, and no compensating control was added at the time.
- Only `sccache` is **confirmed** to have needed it. The Bazel REAPI and Turborepo client binaries
  were never exercised against this boundary — recorded as untested, not exonerated. The `corelink`
  CLI and `clw` are `reqwest`+`rustls` and were never blocked.

# Consequences

- **The exit condition is named but not currently measurable.** Returning to 1.3-only requires one
  of: `sccache` gaining 1.3 support, that surface being retired, or a measured TLS-1.2 handshake
  share of approximately zero over a defined window. Nothing in the repo measures handshake version
  share for this zone today, so the third trigger cannot be evaluated. A decision with no evaluable
  exit condition is a permanent one by default, and saying so is the point.
- **The drift risk is the durable lesson, and it is now instrumented.** Anyone with dashboard
  access can change this setting, and until 2026-08-24 nothing in this repository would have
  noticed. `scripts/check_tls_floor.py`, run daily by `.github/workflows/tls-floor-drift.yml`,
  asserts **equality** with the documented floor: a raise back to 1.3 breaks `sccache` again, and a
  drop below 1.2 makes the compliance set overstate the control. It exits non-zero when it cannot
  authenticate, because a drift check that cannot read the value must never report "no drift". The
  general lesson stands for every other zone setting, which remain uninstrumented.

# Citations

1. `specs/03_architecture/adrs/ADR-0072-humangr-zone-min-tls-1.2.md:17-31` — the Status section: the
   decision was already LIVE before the ADR existed, changed by a manual Cloudflare API call on
   2026-07-19 and recorded only in an informal e2e ledger; the live value was re-read on 2026-08-23
   against `GET /zones/{zone_id}/settings/min_tls_version` and confirmed as `1.2`.
2. `specs/03_architecture/adrs/ADR-0072-humangr-zone-min-tls-1.2.md` — the Decision and its cost: the
   floor is 1.2 deliberately; `sccache` on macOS links `native-tls`/SecureTransport and was refused
   at the handshake by a 1.3-only floor; the Bazel REAPI and Turborepo client binaries were never
   exercised against this boundary and are recorded as untested rather than exonerated; the
   `corelink` CLI and `clw` are `reqwest`+`rustls` and were never blocked.
3. `specs/03_architecture/adrs/ADR-0072-humangr-zone-min-tls-1.2.md` — the Consequences: the exit
   condition (sccache gains 1.3, the surface is retired, or a measured ~0 share of 1.2 handshakes)
   and the fact that the third trigger is **not currently measurable**, since nothing in the repo
   tracks handshake-version share for this zone; plus the drift risk that the setting lives only in
   the vendor dashboard with no IaC surface to diff it against.
4. `specs/03_architecture/security_model.md:254` — CTRL-CRYPTO-001, now stating the 1.2 floor with
   a pointer to this ADR and an explicit note that the SSL Labs A+ evidence predates the change.
   Until 2026-08-24 this row read "TLS 1.3 only", describing a floor that had not been in force
   since 2026-07-19.

5. `scripts/check_tls_floor.py:31-40` — the instrumentation this concept's drift risk called for:
   the zone id and the documented floor are declared in the script, and the comparison is equality
   rather than a minimum, so drift in either direction is a failure. Authentication failure exits
   non-zero rather than passing, because a check that cannot read the value cannot claim it has not
   drifted.
