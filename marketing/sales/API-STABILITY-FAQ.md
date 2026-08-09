---
id: "SALES-API-STABILITY-FAQ"
type: "marketing"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "R-PREP-SALES-ENABLEMENT"
tags: ["sales", "faq", "api", "stability", "deprecation", "enterprise", "r-prep", "ga"]
---

# CoreLink Sales FAQ — API Stability & Deprecation (10 questions)

> **Audience:** anyone running a CoreLink enterprise-prospect conversation
> when the buyer's platform / DevEx / staff-engineering team is doing API due
> diligence. These are the questions the **buyer** asks before they will let
> their CI fleet depend on us.
>
> **Tone:** factual, no marketing varnish. The 24-month GA deprecation window
> and the 3-tier model are **contractual** — every numeric claim has a doc
> pointer.
>
> **Companion docs:**
> - `apps/docs/docs/explanation/api-stability.mdx` (the public policy — this FAQ paraphrases it)
> - `specs/_audits/sealed/2026-05-15-api-stability-baseline.md` (the per-endpoint tier matrix)
> - `marketing/sales/FAQ-MASTER.md` (master 50-question sales FAQ)
> - `apps/docs/docs/trust/index.mdx` (Trust Center; links to this policy)

---

## How to use this file

Each entry has **Q**, **A** (the canonical answer to read or paraphrase), and
**Sources** (doc / spec pointers proving the claim is real). Numbers in
**bold** are part of the public commitment and must not be paraphrased away.

---

## 1. "How long will the v1 API keep working if you ship v2?"

**Q:** If you ship a v2 of the API, how long do I have to migrate?

**A:** For any **GA-tier** endpoint, you get a minimum **24 months** from
the moment we announce the deprecation to the moment the endpoint returns
`410 Gone`. That window is intentional — it's long enough that even
slow-moving enterprise platform teams on annual release trains can absorb
the change in their normal cadence. We run at most two major REST versions
concurrently (`v1` + `v2`), so once we ship v2 you can stay on v1 for the
full 24 months while you migrate. The 24-month clock is **contractual**, not
aspirational — it's enforced in CI on every PR that touches our OpenAPI spec.

**Sources:** `apps/docs/docs/explanation/api-stability.mdx` §1, §3, §6;
`.github/workflows/api-deprecation-check.yml`.

---

## 2. "How will I know an endpoint is being deprecated?"

**Q:** What's the customer-visible signal that an endpoint is going away?

**A:** Five overlapping signals, in this order:

1. **`api-announce@humangr.com` mailing list** — low-volume (≈6 messages/year),
   sent at T+0 day. This list is **not** subject to data-erasure DSRs — you
   stay subscribed across personnel changes.
2. **Status-page banner** at T+0.
3. **CHANGELOG entry** under `[Unreleased]` § Stability at T+0.
4. **`Sunset:` HTTP response header** (RFC 8594) starting at T+30.
5. **Targeted email blast** to your tenant's notification address at T+60.

All four of our first-party SDKs already log a structured WARN line when
they see a `Sunset:` header, so any SDK-based integration gets a visible
breadcrumb in your own logs before T+60.

**Sources:** `apps/docs/docs/explanation/api-stability.mdx` §3, §5.

---

## 3. "What's the difference between GA, Preview, and Internal?"

**Q:** Some of your endpoints aren't covered by the 24-month window. Which
ones, and how do I avoid them?

**A:** Three tiers, all **explicitly classified** in the OpenAPI spec:

- **GA** — full SLO, 24-month deprecation window. **23 of 35** endpoints
  today, including all signup / DPA / PAT / DSR / consent surfaces and the
  full REAPI v2 CAS + ByteStream surface.
- **Preview** — best-effort, no SLO, **90-day** removal window, opt-in via
  the `X-CoreLink-Preview: 1` header. **2 of 35** today
  (`PATCH /v1/users/me`, `GET /v1/admin/audit/events`). Both are on track
  for GA promotion in 2026-Q4.
- **Internal** — CoreLink-only or system-to-system; **no deprecation
  notice**. **10 of 35** today (health probes, CSP report sink, Stripe
  webhook ingest, admin dual-control ops). You will not hit these endpoints
  in normal customer use — the SDKs do not expose them.

You can read the per-endpoint matrix at
`specs/_audits/sealed/2026-05-15-api-stability-baseline.md`.

**Sources:** `specs/_audits/sealed/2026-05-15-api-stability-baseline.md` §1, §3;
`apps/docs/docs/explanation/api-stability.mdx` §1.

---

## 4. "Can I lock my CI fleet to a specific API minor version?"

**Q:** I'm running tens of thousands of CI jobs per day. I can't have a
silent shape change break my fleet at 3am. Can I pin?

**A:** Yes — set the `X-CoreLink-API-Minor: 2026-05-15` request header (or
the date of your choice). Once your PAT is issued, the server pins to the
minor active at issuance unless you change the header. Enterprise-tier
tenants can also set `pin_api_minor: true` to disable the "default to
latest" behaviour entirely. The Stripe and GitHub APIs use the same model;
we picked it because it's the lowest-surprise pattern for platform teams.

We will run a minimum of **two minor versions concurrently** at any time —
the previous and the current — so even short pins survive a normal release.

**Sources:** `apps/docs/docs/explanation/api-stability.mdx` §2.2.

---

## 5. "What happens to the SDK when the underlying endpoint is deprecated?"

**Q:** I depend on `corelink-sdk-rust`. Will an SDK major version break my
build before the API endpoint disappears?

**A:** No. We **couple** SDK major bumps to API endpoint removals — a 2.0
SDK release ships **at the moment** the matching endpoints flip to `410
Gone`, not before. The 1.x line then receives security patches for **6
more months** before EOL. The practical timeline you can plan against:

- T+0 (announce) → 1.x SDK keeps working, gets a deprecation warning on the
  affected calls.
- T+24mo → 2.0 SDK ships, 1.x endpoints go to 410.
- T+30mo → 1.x line EOL.

We do **not** ship SDK majors for ergonomics, internal refactors, or
"cleanup." Every major has a corresponding API change.

**Sources:** `apps/docs/docs/explanation/api-stability.mdx` §4.1.

---

## 6. "Can you bring a sunset date forward if business changes?"

**Q:** What stops you from changing your mind and shortening the deprecation
window after I've committed?

**A:** Two enforcement layers:

1. **CI gate.** The `api-deprecation-check.yml` workflow runs on every PR
   that touches the OpenAPI spec. It compares the base-ref vs head-ref and
   **rejects** any PR that moves an `x-sunset-date` earlier. Sunset dates
   can only be **extended**.
2. **Dual-hat sign-off.** Extensions themselves require Tech Lead + DPO
   approval via an ADR; the same ADR is announced on the same channels as
   the original deprecation, so the change is observable.

The 24-month window is the floor. We can give you longer; we cannot give
you shorter.

**Sources:** `.github/workflows/api-deprecation-check.yml`;
`apps/docs/docs/explanation/api-stability.mdx` §3.2, §6.

---

## 7. "What's the contract on the REAPI v2 (gRPC) surface?"

**Q:** Bazel/Buck2/RBE clients talk to your CAS + ByteStream over gRPC.
Same stability tier?

**A:** We don't offer a gRPC transport — CoreLink runs on Cloudflare
Workers (workerd), and workerd has no HTTP/2 trailers support, which
gRPC requires. REAPI v2 is served **REST-only**
(`crates/corelink-container/src/routes/bazel_v2.rs`). All seven
customer-facing REAPI v2 endpoints are **GA** over REST:
`Capabilities.GetCapabilities`, `CAS.BatchUpdateBlobs`,
`CAS.FindMissingBlobs`, `CAS.BatchReadBlobs`, `ByteStream.Read`,
`ByteStream.Write`, `ByteStream.QueryWriteStatus`. We are pinned to the
upstream `build.bazel.remote.execution.v2` namespace; a v3 ships only if
upstream ships v3. The same 24-month GA deprecation window applies. The
`Health.Check` endpoint is Internal (LB probe; not part of the customer
contract).

Action Cache (`GetActionResult`, `UpdateActionResult`) is not yet exposed
on the public surface. When it lands it will be **Preview** for 90 days
before being promoted to GA.

**Sources:** `specs/_audits/sealed/2026-05-15-api-stability-baseline.md` §2;
`apps/docs/docs/reference/reapi/`.

---

## 8. "Will you break my workflow with a 'minor' change?"

**Q:** Stripe's "minor" header model is famous, but breaking changes still
sneak in. What's actually breaking-vs-non-breaking for you?

**A:** The full table is in the policy doc, but the short version:

| Change                                  | Breaking? |
|-----------------------------------------|-----------|
| New optional response field             | No        |
| New optional request parameter          | No        |
| New endpoint                            | No        |
| New required input on existing endpoint | **Yes** (new major) |
| Field **removed**                       | **Yes** (new major) |
| Field type changed (e.g. string → int)  | **Yes** (new major) |
| Default value changed                   | **Yes** (new major) |
| Validation **tightened**                | **Yes** (new major) |

A breaking change *only* ships via a new URL major (`/v2/...`). It cannot
ship inside `/v1/*`. This is enforced by code review + the regeneration CI
on `corelink-openapi`/`corelink-reapi`.

**Sources:** `apps/docs/docs/explanation/api-stability.mdx` §2.1, §2.3.

---

## 9. "What's the data residency story for the API surface itself?"

**Q:** Does the stability policy change by region?

**A:** No. The same tiers and the same 24-month / 90-day windows apply in
every region (SAM today; EU + APAC on the roadmap). Regional roll-out of a
new minor or major version may be staggered (typically 1-2 weeks), but the
deprecation clock starts at the **announcement** commit on `main`, not at
regional roll-out — so customers in late regions get the **same total
runway**, not less.

**Sources:** `apps/docs/docs/explanation/api-stability.mdx` §2, §3;
`apps/docs/docs/trust/data-handling.mdx`.

---

## 10. "What's the minimum work I have to do to stay on the right side of the contract?"

**Q:** TL;DR — what does my team need to do to never be surprised?

**A:** Five things, all light-touch:

1. Have **one human** subscribed to `api-announce@humangr.com`.
2. Make sure your client (SDK or middleware) logs the `Sunset:` response
   header at WARN. First-party SDKs do this automatically.
3. Set `X-CoreLink-API-Minor: <date>` if your CI fleet is sensitive to
   shape changes. Otherwise default behaviour is fine.
4. Read the CHANGELOG `### Stability` section every SDK minor release
   (≈once a month).
5. Run any **Preview** endpoints in non-production only, or accept the
   90-day removal window.

That's it. Do those five and the worst case is that 24 months out you
notice a `Sunset:` header in your logs, file a 1-hour migration ticket,
and move on.

**Sources:** `apps/docs/docs/explanation/api-stability.mdx` §5.

---

## Cross-references

- Public policy: `apps/docs/docs/explanation/api-stability.mdx`
- Per-endpoint matrix: `specs/_audits/sealed/2026-05-15-api-stability-baseline.md`
- CI gate: `.github/workflows/api-deprecation-check.yml`
- Extractor: `scripts/extract-api-deprecations.py`
- Master sales FAQ: `marketing/sales/FAQ-MASTER.md`
- Trust Center: `apps/docs/docs/trust/index.mdx`
- Roadmap §7 R-7 (GA Gate): `ROADMAP-TO-GA.md` §7

---

*Material changes to this FAQ track material changes to the public policy.
The policy is the source of truth; if this FAQ ever drifts, the policy
wins.*
