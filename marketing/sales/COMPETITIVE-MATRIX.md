---
id: "SALES-COMPETITIVE-MATRIX"
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
tags: ["sales", "competitive", "r-prep", "ga", "honest-comparison"]
---

# CoreLink Competitive Matrix — Honest Comparison

> **Rule of engagement:** we do not bash competitors. Every other product in this space exists because real teams chose it. The point of this matrix is to help a prospect figure out **which tool fits their constraints** — including the case where the answer is not CoreLink.
> **Audience:** Founder, Customer Success, partner SE. Use the rows to anchor a conversation, not to win one.
> **Companion docs:** `marketing/sales/FAQ-MASTER.md`, `marketing/sales/OBJECTION-HANDLING.md`, `marketing/sales/PROOF-POINTS.md`.

---

## The five reference points

1. **bazel-remote** — the reference cache implementation. Single-tenant. Self-hosted.
2. **BuildBuddy** — polished hosted RBE + remote cache. Single primary region per customer.
3. **BuildBarn** — high-quality self-hosted modular implementation.
4. **NativeLink** — newer Rust-implemented self-hosted entrant with strong performance.
5. **Self-hosted on S3 / Harbor / internal Bazel cache** — any in-house build.

Reference posts: `marketing/launch/BLOG-POSTS/01-introducing-corelink.md#the-build-cache-landscape`; `REMOTE-CACHE-PRODUCT-PROFILE`.

---

## 1. Headline matrix

| Dimension | CoreLink | bazel-remote | BuildBuddy | BuildBarn | NativeLink | Self-hosted S3 |
| --- | --- | --- | --- | --- | --- | --- |
| **Deployment model** | Managed SaaS | Self-hosted | Hosted + self-hosted | Self-hosted | Self-hosted | Self-hosted |
| **Multi-tenant by construction** | Yes | No (single-tenant by design) | Yes (hosted) | Operator-built | Operator-built | Operator-built |
| **Tenant-isolation invariant (TLA+)** | Yes (`tenant_isolation.tla` in CI) | N/A | Not published | N/A | N/A | N/A |
| **BYOK with kill switch < 5 min** | AWS KMS at GA (GCP/Azure/Vault roadmap) | No | Limited | Operator-built | Operator-built | Operator-built |
| **Audit chain (Merkle / RFC 6962)** | Yes (RFC 6962 + RFC 8785 JCS) | No | Logging-style | Operator-built | Operator-built | Operator-built |
| **Multi-region residency invariant** | Yes (4 regions + 3 on-request, `INV-REGION-NO-CROSS-LEAK`) | Per deployment | Single primary region per customer | Operator-built | Operator-built | Operator-built |
| **REAPI v2 conformant** | Yes (CAS + AC) | Yes | Yes (CAS + AC + RBE) | Yes | Yes | Operator-built |
| **Remote Build Execution (RBE)** | No — Phase 2 post-GA | No | **Yes** | Yes | Yes | Operator-built |
| **Zero-egress cache reads** | Yes (R2 substrate) | Depends on host | Depends on host | Depends on host | Depends on host | No (S3 egress) |
| **24/7 on-call SaaS support** | Yes | DIY | Yes | DIY | DIY | DIY |
| **SOC 2** | Type I Q4-2026 target (83.7% ready) | N/A | Yes | N/A | N/A | Your responsibility |
| **ISO 27001** | Q1-2027 target (98.9% crosswalk) | N/A | Yes | N/A | N/A | Your responsibility |
| **LGPD / GDPR processor DPA** | Yes (3-locale review) | N/A | Yes | N/A | N/A | Your responsibility |
| **License (managed offering)** | Proprietary SaaS | Apache 2.0 (self-host) | Proprietary SaaS + OSS core | Apache 2.0 | Apache 2.0 | Proprietary or BSD |
| **OSS verification toolkit** | Yes (audit-chain verifier in Rust + TS) | N/A | Partial | N/A | N/A | N/A |

---

## 2. Detailed comparisons

### 2.1 vs. **bazel-remote**

**Where bazel-remote wins:**
- Zero cost (Apache 2.0).
- Operationally minimal for a single team running their own cache.
- Mature reference implementation; most teams start here.
- Maximum control — you own the deployment, the data, the operational tail.

**Where CoreLink wins:**
- **Multi-tenant by construction** with TLA+-modeled isolation invariants. bazel-remote is single-tenant; multi-tenant is achieved by running N instances.
- **No operational tail.** Self-hosted bazel-remote is a part-time job for an SRE — eviction tuning, GC correctness, dashboard maintenance, region replication, 3am pages on disk-fill. CoreLink is a vendor relationship plus an SLO.
- **Zero egress cache reads** (R2 substrate). bazel-remote on S3 pays egress on every hit; on a 70% hit ratio with multi-TB working set, egress dominates the bill.
- **Audit chain with RFC 6962 Merkle proofs**, Trust Center, sub-processor disclosure, residency invariants. None of these exist in bazel-remote because that's not what it's for.
- **24/7 on-call across three regions**, weekly synthetic page exercise sustained for 30 days pre-GA.

**Honest framing:** bazel-remote is the right answer for teams that (a) have an SRE rotation that wants to own a cache and (b) don't have multi-tenancy, residency, or BYOK requirements. If you're running a regulated workload or operating multiple teams under one umbrella, the gap widens fast.

### 2.2 vs. **BuildBuddy**

**Where BuildBuddy wins:**
- **Remote Build Execution (RBE) shipping today.** CoreLink RBE is Phase 2 post-GA — anti-scoped from GA explicitly. If your bottleneck is compute (not cache), BuildBuddy is the right answer today.
- Mature product with years of production traffic.
- Polished hosted offering with active developer relations.
- Both hosted and self-hosted deployment options.
- They have done excellent work normalizing the REAPI surface; we owe them a debt.

**Where CoreLink wins:**
- **Multi-region residency invariant.** BuildBuddy's hosted plane runs in a single primary region per customer. CoreLink has 4 GA regions (3 more on request) with `INV-REGION-NO-CROSS-LEAK` as a CRITICAL invariant — region pinning is structural, not configuration.
- **BYOK at the envelope-encryption layer** — AWS KMS at GA (GCP / Azure / Vault on the roadmap) with a 5-minute structural DEK-cache kill switch. This is not currently part of BuildBuddy's shipped scope.
- **TLA+-modeled tenant isolation** checked in CI.
- **Audit chain as a primary artifact** (RFC 6962 + RFC 8785 JCS, independently re-derivable) — not logging-style retention.

**Honest framing:** if your evaluation is RBE-led, BuildBuddy is shipped and we are not. If your evaluation is cache-led with regulated-industry constraints (residency / BYOK / audit), the structural differences favor us.

### 2.3 vs. **BuildBarn**

**Where BuildBarn wins:**
- Modular, operationally serious self-hosted implementation.
- Open-source, full source control over your deployment.
- Strong community and a track record at scale.
- Maximum customization — you can shape it to non-standard topologies.

**Where CoreLink wins:**
- Same managed-vs-self-hosted axis as bazel-remote: BuildBarn's self-hosted nature means the customer absorbs the operational tail — eviction tuning, GC correctness, deduplication ratios, blob sprawl, region replication.
- Audit chain, BYOK with kill switch, residency invariants, sub-processor disclosure, three-locale DPA — all are operator-built territory in a BuildBarn deployment.
- Managed-service economics: a CoreLink Enterprise contract maps to ~0.5–1 SRE-FTE of operational work absorbed.

**Honest framing:** BuildBarn is the right answer for teams who *want* to operate the cache themselves and have the headcount + expertise to do it well.

### 2.4 vs. **NativeLink**

**Where NativeLink wins:**
- Permissive license; runtime is fast (Rust-implemented).
- Active development; recent entrant with momentum.
- Self-hosted control.
- Strong performance characteristics on the data path.

**Where CoreLink wins:**
- Same self-hosted-vs-managed framing — NativeLink leaves tenancy, residency, and key management to the operator.
- BYOK / kill switch / 4-provider matrix / FIPS-endpoint pinning is product surface we ship; it's operator-built in NativeLink.
- Audit chain as RFC 6962 with offline-verifiable Merkle proofs: ours; theirs is whatever the operator builds.
- Managed-service operational model: 24/7 on-call, weekly synthetic page, status page, incident response, breach-notification commitments.

**Honest framing:** NativeLink and CoreLink have different shapes — they self-host a fast Rust cache; we operate a managed multi-tenant fabric on Cloudflare's edge. We are not directly substitutable for the operator who actively wants to self-host.

### 2.5 vs. **Self-hosted on S3 / Harbor / internal Bazel cache**

**Where self-hosted wins:**
- Zero vendor lock-in by definition.
- You own the operational substrate, the data, the keys, the security posture.
- If you have an existing internal-platforms team already building this kind of infrastructure, the marginal cost of one more cache may genuinely be small.
- Some procurement regimes prefer in-house over any vendor.

**Where CoreLink wins:**
- **Egress economics.** Self-hosted on S3 pays cloud egress on every cache hit. `BLOG-POSTS/05-fast-cache-hit-economics.md` walks the math; the line item is usually the largest one for any non-trivial hit ratio.
- **Operational toil.** Multi-tenant + BYOK + audit chain + residency invariants is a 6–12 month engineering project minimum, plus ongoing carry — eviction, GC, dashboards, on-call.
- **Compliance overhead.** SOC 2 / ISO 27001 / LGPD / GDPR mappings inherited via CoreLink's posture vs. constructed in-house.
- **Time-to-value.** D+1 first cache write; D+7 measurable hit ratio; D+14 cutover-ready.
- **Harbor specifically** is a Docker registry, not a build cache — partial overlap. Layer caching is one valid CAS workload, but for *registry + layer caching combined*, keep Harbor for manifests and offload layer-cache traffic to us (see FAQ M3).

**Honest framing:** for teams with established platform-engineering capacity and tolerance for the operational tail, self-hosted is a defensible choice. The question CoreLink helps answer is: *is build-cache infrastructure where your platform team should be spending the next year?*

---

## 3. Decision tree (one-page)

```
Q1: Do you need Remote Build Execution (RBE / compute), or remote cache (CAS / AC) only?
    → RBE today: BuildBuddy is shipped; CoreLink RBE is Phase 2 post-GA.
    → Cache only: continue to Q2.

Q2: Are you in a regulated industry (financial, healthcare-adjacent, gov-adjacent, EU/BR with sector regulator)?
    → Yes: CoreLink's structural residency + BYOK kill switch + audit chain + 3-locale DPA + Schrems II TIA is the differentiator. Continue to Q3.
    → No: continue to Q3.

Q3: Do you have an SRE rotation that wants to own a cache, AND tolerance for the operational tail?
    → Yes: self-hosted is a real option (bazel-remote / BuildBarn / NativeLink). Run the TCO comparison at corelink-docs.humangr.com.
    → No: managed (CoreLink / BuildBuddy hosted) is the smaller-toil path. Continue to Q4.

Q4: Single primary region acceptable, or multi-region residency required?
    → Single region: BuildBuddy is mature and competitive.
    → Multi-region with structural no-cross-region-leak: CoreLink is the differentiator.

Q5: BYOK with sub-5-minute kill switch required?
    → Yes: CoreLink (4 providers).
    → No: any of the managed options work.
```

---

## 4. Where we will *not* compete

There are conversations we will lose on purpose and the customer should win:

- **HIPAA / PHI workloads.** Out of scope by design. Do not push us in. See FAQ C5.
- **FedRAMP-required deployments.** We don't have it and don't have a 12-18 month plan to get it. NIST 800-53 Rev 5 crosswalk at 87% today, informational only. See FAQ C7.
- **Air-gapped on-prem.** On-prem and self-hosted offerings are post-GA roadmap; we don't ship them today.
- **Customers whose primary build tool is not in {Bazel, Buck2, Pants, generic-HTTP-CAS-client}.** We won't admit a lighthouse customer if integration requires new product work. Lower-tier paid customers are welcome to use the HTTP/REAPI surface; we just won't promise a built-in integration we don't have.
- **Customers who need explicit cross-tenant deduplication via convergent encryption.** We deliberately do not do this — cross-tenant ciphertext convergence is a side channel. If you want convergent dedup across tenants, we're a no.

---

## 5. Internal-only — competitor watch cadence

This matrix is reviewed quarterly. Triggers for between-quarter refresh:
- Any of the four reference competitors ships a feature that lands in the matrix's "Where they win" column.
- New entrant gets ≥ 100 GitHub stars/week sustained for 4 weeks.
- New entrant lands a public customer ≥ 5,000 engineers.

Maintenance ticket: `r-prep-competitive-matrix-refresh-Q*-YYYY`.

---

## Cross-references

- **FAQ:** `marketing/sales/FAQ-MASTER.md`.
- **Objection handling:** `marketing/sales/OBJECTION-HANDLING.md` (Obj-6 through Obj-10 walk competitor objections individually).
- **Proof points:** `marketing/sales/PROOF-POINTS.md`.
- **Build-cache landscape post:** `marketing/launch/BLOG-POSTS/01-introducing-corelink.md#the-build-cache-landscape`.
- **Product profile:** `REMOTE-CACHE-PRODUCT-PROFILE`.

---

**Fim SALES-COMPETITIVE-MATRIX.**
