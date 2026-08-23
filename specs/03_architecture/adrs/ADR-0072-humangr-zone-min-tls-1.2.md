---
id: "ADR-0072"
type: "adr"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-08-23"
updated: "2026-08-23"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "security", "tls", "cloudflare", "zone-config", "sccache", "drift-risk"]
---

# ADR-0072 — `humangr.com` zone `min_tls_version`: 1.3 → 1.2

## Status

**DRAFT, but the decision is already LIVE.** The zone setting was changed via a
manual Cloudflare API call on 2026-07-19 (owner-authorized, recorded informally
in `docs/internal/real-client-e2e-ledger-2026-07-19.md:25`) and has not been
reverted. This ADR is filed retroactively to codify a decision that already
shipped without a paper trail — see "Drift risk" below for why that gap matters
on its own.

**Live value confirmed 2026-08-23** via `GET
/zones/{zone_id}/settings/min_tls_version` against the Cloudflare API for
`humangr.com` (zone `73f57f6d508beea67e2f78bea11d3c25`):

```json
{"result":{"id":"min_tls_version","value":"1.2","modified_on":null,"editable":true}}
```

`modified_on: null` is itself a small finding: Cloudflare is not recording when
this value last changed, so the dashboard alone cannot answer "when did this
flip" — only this ADR and the ledger entry can.

## Context

`specs/03_architecture/security_model.md` declares **CTRL-CRYPTO-001 — "TLS 1.3
only"** as a standing control (`:254`), backed by `INV-CONF-IN-FLIGHT` (`:69`)
and re-asserted in three separate S-02 sprint risk registers
(`specs/04_sprints/S02/sprint.md:242`, `_spec_contract.md:247`,
`work_items/WI-S02-001-bytestream-read.md:490`) as a low-risk, CF-handled,
already-tested boundary. That control is **currently false** — the zone accepts
TLS 1.2 — and `security_model.md` has not been updated to match. This ADR does
not fix that drift (out of scope: docs/infra only, no code), but it names it so
the next reader of `security_model.md` does not trust a stale control.

The 2026-07-19 real-client e2e pass (`docs/internal/real-client-e2e-ledger-2026-07-19.md`)
ran every cache surface against prod with the actual client binary a real user
runs, not `curl` as a proxy. `curl` had been masking this defect: curl always
negotiated TLS 1.3 against the TLS-1.3-only floor and reported green, while
`sccache` 0.15 — which on macOS runs over Apple SecureTransport via
`native-tls`, not `rustls` — failed the handshake outright. Finding #2 in the
ledger: **"TLS-1.3-only floor on `corelink-api`/`signup`/`oci` — blocked
native-tls/SecureTransport-macOS clients (sccache) & any TLS≤1.2 client."**

`sccache` is a real, currently-sold cache surface (`docs/knowledge/surfaces/sccache-cargo.md`,
`apps/docs/docs/integrations/sccache-cargo.md`), not a speculative future one —
CLAUDE.md lists it alongside native CAS/AC, Bazel REAPI v2, and Turborepo as one
of the surfaces already exposed to customers. A TLS floor that silently 400s a
sold integration is a customer-facing defect, not an internal nit.

### Which clients actually needed 1.2

Checked every real-client surface named in CLAUDE.md and the e2e ledger:

| Client | TLS stack | Needs 1.2? |
|---|---|---|
| `sccache` 0.15 (WebDAV → `cargo_gate`) | `native-tls` → Apple SecureTransport on macOS | **Yes** — this is the confirmed, reproduced failure. |
| `corelink` CLI (`tools/cli/Cargo.toml:52,67,89`) | `reqwest` with `rustls-tls` (`hyper-rustls` 0.27, `rustls` 0.23, `ring` provider) | No — `rustls` negotiates 1.3 and does not need the floor lowered. |
| `clw` (corelink-workspaces, `crates/clw-client/Cargo.toml`) | `reqwest` via the workspace's shared `rustls`-based config | No — same reasoning as the CLI. |
| Bazel REAPI v2 client (`routes/bazel_v2.rs`) | client-supplied; no CoreLink-side TLS stack constraint found in-repo | Unknown — no real `bazel --remote_cache` binary run was TLS-instrumented in the ledger (it 404s by documented design before TLS matters); not exonerated, just untested. |
| Turborepo client (`routes/turbo_v8.rs`) | client-supplied; the ledger's turbo pass was protocol-level only, no real `turbo` binary | Unknown — same caveat as REAPI. |
| `npm`/`pip`/`brew`/`docker` (mirror surfaces) | varies by host OS TLS stack | Not tested against the 1.3 floor specifically; no reported handshake failures in the ledger. |

**Conclusion: one client is confirmed (`sccache`, via `native-tls`/SecureTransport),
two are not exonerated because they were never run as real binaries against this
specific boundary (Bazel REAPI, Turborepo), and everything CoreLink itself
ships (`corelink` CLI, `clw`) is on `rustls` and did not need this change.** An
ADR that named only `sccache` and implied everyone else was fine would be
overclaiming; this one says explicitly what was tested and what was not.

### IaC surface

`infra/terraform/modules/cloudflare-base/main.tf` is the module that owns
zone-level, non-region-scoped Cloudflare resources (DNS records, Worker
routes, Pages project, custom domain). It manages `cloudflare_record` and
worker-route resources today. **It does not have a `cloudflare_zone_settings_override`
resource, and no other `.tf` file in `infra/terraform/` references
`min_tls_version` or any zone-settings resource.** Searched the full
`infra/terraform/` tree for `min_tls`, `zone_settings`, and
`cloudflare_zone_settings_override` — zero matches. **No IaC surface exists for
this setting today**, though `cloudflare-base` is the module where it belongs
if one is built (see Consequences).

## Decision

Keep `humangr.com` `min_tls_version` at **1.2** for the launch + CI-acceleration
posture, and record the setting here so it is reviewable, diffable in intent
(even without being diffable in Terraform state), and has a named owner and
exit condition. This ADR does not change the live value; it documents a
decision that was already made operationally and gives it the paper trail
`docs/internal/real-client-e2e-ledger-2026-07-19.md:25` asked for.

## Rationale

- `sccache` is a real, sold surface (CLAUDE.md: "It already exposes multiple
  cache surfaces … and **sccache** (WebDAV)"), and its default macOS TLS stack
  cannot negotiate a 1.3-only floor. Refusing the client is refusing the sale.
- The alternative — telling every `sccache` user to build a custom binary
  against `rustls`, or to run a TLS-terminating proxy in front of CoreLink — is
  not a real option for a self-serve SMB product; it reintroduces the
  operational burden the product exists to remove.
- Lowering the floor to 1.2 is the standard, narrowly-scoped fix: it widens the
  accepted handshake set without touching cipher suite selection, HSTS, or any
  other control in `security_model.md`'s crypto-controls table.

## What was given up

TLS 1.2 is not equivalent to 1.3. Concretely, at 1.2 the zone accepts:

- **Non-forward-secret and weaker cipher suites** that 1.3 eliminated by
  design — 1.3 only offers AEAD ciphers with ephemeral (EC)DHE key exchange;
  1.2 permits static-RSA key exchange and CBC-mode ciphers (subject to
  whatever cipher list Cloudflare's edge negotiates for the zone, which this
  ADR did not separately audit — the `min_tls_version` setting alone does not
  pin the cipher suite list).
- **A slower, more probe-prone handshake** (1.2's extra round trip;
  more negotiation surface for downgrade-style probing) versus 1.3's reduced
  handshake and mandatory forward secrecy.
- **A wider footprint for known TLS 1.2-era issues** (e.g. Lucky13/BEAST-class
  padding-oracle history against CBC ciphers, if such ciphers are in fact
  offered) that 1.3 structurally closes off by removing CBC and static RSA
  from the protocol entirely.

**What compensates:** Cloudflare terminates TLS at the edge for every hostname
on this zone (per `security_model.md:105`, "TB-0: … TLS 1.3 terminating" —
itself now stale prose, since the floor is 1.2, not a hard 1.3 requirement),
and origin traffic to Workers/Containers is Cloudflare-internal, not raw
1.2-negotiated traffic re-exposed to the origin. `curl` and every CoreLink-owned
client (`rustls`-based) still negotiate 1.3 by preference — lowering the floor
does not force weaker negotiation for clients capable of better, it only stops
rejecting clients that cannot do better. No compensating control (WAF rule,
cipher-suite pin, monitoring) was added alongside the floor change; this ADR
did not find one in `infra/terraform` or `security_model.md`. That is a gap,
not a mitigated risk — stated plainly rather than waved at.

## Consequences

**Positive:**
- `sccache` (and any other native-tls/SecureTransport-based client) works
  against the live product today.
- The decision now has a file, an owner, and a re-evaluation trigger instead of
  living only as a Cloudflare API call and a ledger footnote.

**Negative:**
- The zone's effective floor is weaker than `security_model.md` currently
  documents (CTRL-CRYPTO-001, `INV-CONF-IN-FLIGHT`) — that document is now
  inaccurate and should be corrected in a follow-up (tracked below, not done
  in this ADR: docs/infra-only scope excludes touching `security_model.md`'s
  control table in the same change, but the drift is now on record).
- No IaC surface exists for `min_tls_version`, so nothing in this repo can
  diff, lint, or alert on the live value — see "Drift risk."
- No cipher-suite audit was performed alongside this ADR; "what TLS 1.2
  actually permits on this zone" is stated from protocol knowledge, not from a
  measured Cloudflare SSL Labs-style scan post-change. `security_model.md:254`
  cites `EVT-037 (SSL Labs A+)` as the existing verification event for
  CTRL-CRYPTO-001 — whether that scan has been re-run since the floor dropped
  to 1.2 was not established here.

## Drift risk

`min_tls_version` is a Cloudflare zone dashboard/API setting. **Anyone with
zone-edit access to `humangr.com` can change it in the Cloudflare dashboard or
via the API, and nothing in this repository would notice.** There is no
Terraform resource tracking it (confirmed above), no CI check reading it back,
and no alert wired to it. If it silently drifts back to 1.3 — say, during an
unrelated zone-settings cleanup, or a well-meaning security hardening pass
that doesn't know about `sccache` — `sccache` breaks again with the same
symptom this ADR documents, and nobody will connect the regression to this
decision unless they find this file first. This ADR is the only artifact in
the repo that would tell a future engineer "don't just harden this back to
1.3 without reading the exit condition below."

## Exit condition — when this can go back to 1.3-only

Any **one** of the following, not all:

1. **`sccache` (or its `native-tls`/opendal WebDAV client stack) supports TLS
   1.3 negotiation on the platforms CoreLink customers actually run** — i.e.
   the upstream dependency chain (`native-tls` → SecureTransport on macOS, or
   whatever OpenSSL/schannel backend a given customer's `sccache` build uses)
   stops being the blocker. This needs to be re-tested with a real `sccache`
   binary the same way the 2026-07-19 ledger did, not assumed from a changelog.
2. **The `sccache`/WebDAV cache surface is retired** as a supported CoreLink
   integration (unlikely near-term — CLAUDE.md lists it as a live surface and
   it is part of the CI-acceleration expansion story), removing the reason
   the floor was lowered in the first place.
3. **Measurement shows zero (or negligible) TLS 1.2 handshakes against this
   zone over a defined window**, meaning nothing still in production actually
   needs the lower floor even if it nominally could use it.

**Trigger 3 cannot currently be evaluated.** No dashboard, log pipeline, or
metric found in this repo or referenced in `security_model.md` breaks down
edge TLS handshakes by negotiated protocol version per zone. Cloudflare's own
analytics can show this (Zone Analytics / GraphQL `httpRequestsAdaptive` with
a `clientSSLProtocol` dimension is the standard way), but nothing here queries
it, stores it, or alerts on it. **Until that measurement exists, trigger 3 is
aspirational, not actionable** — this ADR should not be read as implying
someone is watching a TLS-1.2-share dashboard and will flip the floor back
when it hits zero. Nobody is watching it today.

## Alternatives considered

- **Per-hostname TLS floor** (keep 1.3 on hostnames that don't serve
  `sccache`, e.g. `signup`, and lower only the surface(s) `sccache` actually
  hits): Cloudflare zone `min_tls_version` is a **zone-level** setting, not
  per-hostname, on the plan/API surface this repo uses — the ledger entry
  itself notes "All Workers hostnames now accept 1.2" as the effect of the
  single zone-level change. A narrower page-rule/hostname-level override was
  not evaluated in depth for this ADR; if a future engineer wants a tighter
  blast radius, that is the first thing to check, not assume unavailable.
- **TLS-terminating proxy in front of CoreLink for `sccache` traffic only**:
  rejected as added operational surface for a self-serve SMB product that
  should stay simple to run; also shifts the same 1.2-vs-1.3 trust boundary
  one hop rather than resolving it.
- **Tell customers to use a `rustls`-based sccache fork/build**: rejected —
  not a real ask of a self-serve customer base, and no such fork is
  officially maintained upstream today.
- **Do nothing (leave TLS-1.3-only, accept sccache is broken)**: rejected —
  this was the state that produced the customer-facing defect the 2026-07-19
  ledger caught; leaving it would mean shipping a documented integration that
  does not work.

## Follow-ups (not done in this ADR — docs/infra only, no code)

- Correct `specs/03_architecture/security_model.md` CTRL-CRYPTO-001 (`:254`)
  and the TB-0 diagram (`:105`) to state the actual floor (1.2) instead of
  "TLS 1.3 only," and note the `sccache` exception explicitly.
- Correct the three stale S-02 sprint risk-register rows that still describe
  "TLS 1.3 only path" as the tested boundary (`sprint.md:242`,
  `_spec_contract.md:247`, `WI-S02-001-bytestream-read.md:490`).
- If drift protection is wanted, add a `cloudflare_zone_settings_override`
  resource for `min_tls_version` to `infra/terraform/modules/cloudflare-base`
  (the module already owns zone-level, non-region-scoped Cloudflare
  resources) so the value is at least importable into state and visible to
  `terraform-drift.yml`. Not done here because this ADR is documentation-only
  by explicit scope.
- If the exit condition is meant to be real, wire a TLS-version-by-handshake
  metric (Cloudflare GraphQL Analytics `clientSSLProtocol` or equivalent) into
  whatever dashboard/alerting this repo already uses for edge observability.

## References

- `docs/internal/real-client-e2e-ledger-2026-07-19.md:19,25` — the original
  finding and fix.
- `specs/03_architecture/security_model.md:67-73,103-106,254,385-391` —
  the (currently stale) TLS-1.3-only control this ADR documents the deviation
  from.
- `infra/terraform/modules/cloudflare-base/main.tf` — the module that would
  own a `min_tls_version` IaC resource if one is built.
- CLAUDE.md — sccache listed as a live, sold cache surface; CI/build-acceleration
  expansion context.
- Cloudflare zone `humangr.com` (`73f57f6d508beea67e2f78bea11d3c25`),
  `GET /zones/{id}/settings/min_tls_version` — live value read 2026-08-23:
  `"value":"1.2"`.
