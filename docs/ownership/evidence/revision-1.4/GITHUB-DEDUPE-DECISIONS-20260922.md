# GitHub deduplication decisions — 2026-09-22

The 23 package-level text-hit rows were inspected against the current issue
titles/bodies in snapshot R2. Every row is **`distinct`** from the ownership
issue because the matched issue is a product defect, security/compliance item,
runtime/deploy concern, migration umbrella, CI/release contract or hygiene
tracker—not the four ownership deliverables (skill, reference, blast radius,
maintenance manual). No issue carries the ownership title or marker.

| Package row(s) | Matched issue(s) | Decision | Boundary reason |
|---|---|---|---|
| `corelink-adapter-host`, `e2e-user-journeys` | #485 | `distinct` | cache behavior/testability product work |
| `corelink-audit`, `corelink-audit-chain` | #1647, #1646 | `distinct` | audit hash and object-lock security/compliance |
| `corelink-billing`, `corelink-billing-stripe`, `corelink-billing-stripe-materializer` | #2003, #1639, #1638, #1629 | `distinct` | hosted mutants and materializer/webhook defects |
| `corelink-byok` | #1653 | `distinct` | BYOK KMS lifecycle and live evidence |
| `corelink-cli` | #1924, #1702 | `distinct` | release workflow contract and repository migration |
| `corelink-core` | #1626 | `distinct` | grant UUID contract defect |
| `corelink-gc` | #1651 | `distinct` | garbage collection/eviction product capability |
| `corelink-handler-cas` | #1663 | `distinct` | storage performance defect |
| `corelink-openapi` | #1952 | `distinct` | OpenAPI/version CI consistency |
| `corelink-ops` | #1644 | `distinct` | vendor-review cadence/compliance |
| `corelink-pat` | #1650 | `distinct` | ignored real-service test execution |
| `corelink-ratelimit` | #1659 | `distinct` | cargo write-path performance |
| `corelink-runbook-tracker` | #1657 | `distinct` | worktree/repository hygiene |
| `corelink-runner-aggregate`, `corelink-runner-overage` | #1630 | `distinct` | shadow-charge calculation defect |
| `corelink-server` | 208 textual hits, including #2049…#43 | `distinct` | composition-root collision; no ownership scope match |
| `corelink-signup` | #1727 | `distinct` | signup-worker deployment prerequisites |
| `corelink-stripe-real` | #1632, #1629 | `distinct` | Stripe idempotency/webhook defects |
| `corelink-worker` | #1702 | `distinct` | migration umbrella, not package ownership |

This resolves the **23 direct-hit decisions** only. The backlog/deduplication
gate remains pending for the 82 no-hit packages until canonical backlog and
alias searches are recorded; `distinct` here is not a publication approval.
