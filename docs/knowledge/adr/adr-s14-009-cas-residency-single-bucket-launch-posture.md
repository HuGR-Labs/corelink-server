---
type: "ADR"
title: "ADR-S14-009 — CAS residency: single-bucket launch posture + per-region remediation"
description: "Records the decision to launch native CAS as a single physical R2 bucket — an explicit, tracked residency limitation — with a specified per-region-bucket remediation gated on bucket provisioning."
source_files:
  - "specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "storage", "residency", "cas", "r2", "schrems-ii", "lgpd", "s14"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S14-009 — CAS residency: single-bucket launch posture + per-region remediation

The native CAS write path stores whole blobs in one physical R2 bucket with the region carried only as a key prefix, so logical region tagging does not satisfy the physical residency that Schrems II / LGPD demand for EU-subject bytes. This ADR refuses the false-confidence "just add the region var" patch, launches with single-region CAS recorded as a known, explicit, gated limitation, and specifies the per-region-bucket remediation that mirrors the already-regional AC path. It exists so the residency gap is visible, scoped, and scheduled rather than a silent compliance hole. See the related storage concepts [native CAS R2 bucket](/storage/r2-cas-bucket.md) and [regional AC buckets](/storage/r2-ac-regional.md).

# Context

The CAS write path writes to a single `corelink-cas-prod` bucket with region in the key prefix from a global env var, never consulting the tenant's immutable primary region; the regional wrangler envs omit the CAS region var entirely, and — the load-bearing gap — one physical bucket is one physical location, so a key prefix is a logical tag, not physical residency, unlike the per-region AC/chunk buckets, as cold-verified in the context `specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md:26-52`.

# Decision

The decision is to launch with single-region CAS as a known, explicit, tracked limitation (not a silent gap) and to gate residency-sensitive EU customers until remediation lands, recorded at `specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md:54-80`. The remediation (Option A) provisions per-region `corelink-cas-<region>` buckets, sets the per-region bucket/region wrangler vars, switches the DSR erase adapter to one client per region bucket mirroring the AC fan-out, and optionally 403-rejects cross-region writes — with zero data-migration cost if executed before real CAS data accumulates, as specified at `specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md:82-100`.

# Consequences

The residency gap is made explicit and scoped while the freshly-verified DSR pipeline ships on a stable topology, at the cost that native CAS is single-physical-region until Option A and the launch guardrail (no EU-residency-contract customers, or pin the bucket jurisdiction) must be honoured operationally, per `specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md:102-109`.

# Citations

1. `specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md:26-52` — the two-layer gap: missing per-region var + single physical bucket (logical-vs-physical residency).
2. `specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md:54-80` — the launch-with-known-limitation decision + the EU-residency launch guardrail.
3. `specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md:82-100` — Option A per-region-bucket remediation (provision, vars, DSR adapter, optional cross-region 403).
4. `specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md:102-109` — positive/negative consequences and the tracking record.
