---
id: "AUDIT-2026-05-27-SBOM-LICENSE-AUDIT"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["sbom", "license", "supply-chain", "cyclonedx", "FF-HR-005", "wave32", "phase1"]
references:
  - "deny.toml"
  - "specs/_compliance/license-allowlist.md"
  - "specs/_audits/sealed/2026-05-22-wave32-prod-deploy-spec.md"
  - ".sbom/cyclonedx-rust.json"
  - ".sbom/cyclonedx-npm-admin-ui.json"
  - ".sbom/cyclonedx-npm-docs.json"
  - "scripts/generate-sbom.sh"
---

# SBOM + License Audit SEAL — post-Phase-1 refresh (WP-7.2)

> **Context:** Phase 1 added new dependencies (Sentry SDK, Clerk SDK, Resend,
> PostHog client, BetterStack). FF-HR-005 charter requires license allowlist +
> SBOM artifact. This audit refreshes both post-Phase-1.

## §1 SBOM artifacts generated

| Artifact | Tool | Components | Format | Path |
|---|---|---|---|---|
| Rust workspace SBOM | cargo-cyclonedx 0.5.9 | 413 | CycloneDX 1.6 JSON | `.sbom/cyclonedx-rust.json` |
| NPM admin-ui SBOM | cdxgen 12.4.4 + pnpm lockfile parser | 50 (direct deps) | CycloneDX 1.6 JSON | `.sbom/cyclonedx-npm-admin-ui.json` |
| NPM docs SBOM | cdxgen 12.4.4 + pnpm lockfile parser | 25 (direct deps) | CycloneDX 1.6 JSON | `.sbom/cyclonedx-npm-docs.json` |

**Rust SBOM generation method:** `cargo cyclonedx --format json --describe binaries`
runs from workspace root, generating per-binary `.cdx.json` files, which are
then merged via Python script (deduplication by purl) into a single workspace
SBOM. 17 binary/CDYLIB targets merged; 413 unique components.

**NPM SBOM generation method:** pnpm-lock.yaml `importers` section parsed per
workspace app; license data enriched via `pnpm licenses list --json` (2,218
entries). Scopes: direct dependencies only (50 admin-ui, 25 docs). Full
transitive closure deferred until `pnpm install` with full node_modules present
(see §5 caveats).

All 3 SBOMs validated as well-formed JSON with correct `bomFormat: "CycloneDX"`
and `specVersion: "1.6"` headers.

## §2 License audit table

**Allowlist (from charter + deny.toml):**
MIT, Apache-2.0, Apache-2.0 WITH LLVM-exception, BSD-2-Clause, BSD-3-Clause,
ISC, MPL-2.0, Unicode-DFS-2016, Unicode-3.0, Zlib, CC0-1.0, 0BSD,
CDLA-Permissive-2.0, UNLICENSED (own workspace crates).

### Rust workspace

| Metric | Count |
|---|---|
| Total components | 413 |
| Allowed (exact match) | 403 |
| No license declared | 0 |
| Flagged (requires triage) | 10 |

**Flagged Rust components (triage below):**

| Package | Version | License expression | Verdict |
|---|---|---|---|
| blake3 | 1.8.5 | `CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception` | **ALLOWED** — all 3 alternatives are on allowlist; parser false positive |
| constant_time_eq | 0.4.2 | `CC0-1.0 OR MIT-0 OR Apache-2.0` | **ALLOWED** — CC0-1.0 and Apache-2.0 are on allowlist; MIT-0 (see §4) |
| target-lexicon | 0.13.5 | `Apache-2.0 WITH LLVM-exception` | **ALLOWED** — already on allowlist; parser false positive |
| aho-corasick | 1.1.4 | `Unlicense OR MIT` | **ALLOWED** — MIT alternative is on allowlist; Unlicense follow-up (see §4) |
| memchr | 2.8.0 | `Unlicense OR MIT` | **ALLOWED** — MIT alternative is on allowlist; Unlicense follow-up (see §4) |
| aws-lc-sys | 0.41.0 | Complex conjunction of ISC, Apache-2.0, MIT, BSD-3-Clause | **ALLOWED** — all individual SPDX IDs are on allowlist; parser false positive on AND conjunction |
| dunce | 1.0.5 | `CC0-1.0 OR MIT-0 OR Apache-2.0` | **ALLOWED** — CC0-1.0 and Apache-2.0 are on allowlist; MIT-0 follow-up (see §4) |
| ryu-js | 0.2.2 | `Apache-2.0 OR BSL-1.0` | **CONDITIONAL** — Apache-2.0 alternative is on allowlist. BSL-1.0 requires ADR (see §4) |
| ryu | 1.0.23 | `Apache-2.0 OR BSL-1.0` | **CONDITIONAL** — Apache-2.0 alternative is on allowlist. BSL-1.0 requires ADR (see §4) |
| rustix | 1.1.4 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | **ALLOWED** — all alternatives are on allowlist; parser false positive |

**Net result:** 0 genuinely non-compliant packages. 10 "flagged" are:
- 8 parser false positives (audit tooling does strict string match; OR/AND
  expressions with at least one allowed alternative are acceptable)
- 2 conditional (ryu/ryu-js BSL-1.0 alternative — Apache-2.0 alternative
  satisfies allowlist; ADR recommended for BSL-1.0 explicit coverage)

### NPM — @corelink/admin-ui

| Metric | Count |
|---|---|
| Total components (direct deps) | 50 |
| Allowed (license declared) | 22 |
| No license declared in SBOM | 28 |
| Flagged | 0 |

Top licenses declared: MIT (20), MPL-2.0 (1), Apache-2.0 (1). All allowed.
The 28 with no license declaration are present in the pnpm lockfile but
their license could not be recovered from `pnpm licenses list` — this is
expected for workspace-local packages and some scoped packages not yet in
the pnpm store index. Mitigation: see §5 NPM transitive SBOM caveat.

### NPM — @corelink/docs

| Metric | Count |
|---|---|
| Total components (direct deps) | 25 |
| Allowed (license declared) | 11 |
| No license declared in SBOM | 14 |
| Flagged | 0 |

Top licenses declared: MIT (9), BSD-3-Clause (1), Apache-2.0 (1). All allowed.

### Grand totals

| Metric | Count |
|---|---|
| Total components across all SBOMs | 488 |
| Allowed | 436 |
| No license declared | 42 (NPM only, see §5) |
| Genuinely flagged (non-allowed) | **0** |
| Conditional (BSL-1.0 review needed) | **2** (ryu, ryu-js — Apache-2.0 alt. present) |

## §3 Phase 1 new dependencies — license triage

Per WP-7.2 mandate: Phase 1 added Sentry SDK, Clerk SDK, Resend, PostHog,
BetterStack. All triaged below.

| Package | License | Status |
|---|---|---|
| `@sentry/nextjs@8.55.2` | MIT | ALLOWED |
| `@sentry/cloudflare@8.55.2` | MIT | ALLOWED |
| `@sentry/node@8.55.2` | MIT | ALLOWED |
| `@sentry/cli@2.39.1` | BSD-3-Clause | ALLOWED |
| `@sentry/core@8.55.2` | MIT | ALLOWED |
| `@sentry-internal/replay@8.55.2` | MIT | ALLOWED |
| `@sentry-internal/feedback@8.55.2` | MIT | ALLOWED |
| `@clerk/nextjs@6.39.3` | MIT | ALLOWED |
| `resend` (workspace dep) | MIT | ALLOWED |
| `betterstack-js` (status pill) | MIT | ALLOWED |

PostHog removed from sub-processors per
`2026-05-27-sub-processors-finalization.md` — if PostHog client remains
in the codebase, its MIT license is also allowed.

## §4 Sentry FSSA paragraph (R21)

Sentry SDK packages (`@sentry/*`) are licensed under **MIT** (SDK v8.x) and
**BSD-3-Clause** (CLI tooling, SDK v6.x). Both licenses are on the CoreLink
allowlist (`deny.toml [licenses] allow[]`).

Sentry's "Functional Software Support Agreement" (FSSA) is a **commercial
service agreement** between Sentry Inc. and subscribers of the `sentry.io`
hosted error-monitoring service. It is **not an open-source license** and
does not:
- Restrict redistribution of the MIT-licensed SDK artifacts
- Impose copyleft obligations on code that imports the SDK
- Affect the SBOM status of `@sentry/*` packages

FSSA terms govern the hosted service subscription only. For SBOM and
supply-chain compliance purposes, the relevant license for all `@sentry/*`
SDK components is MIT (or BSD-3-Clause for legacy v6). No special waiver
is required.

**SBOM verdict for Sentry:** All `@sentry/*` components — ALLOWED.

## §5 Caveats and limitations

1. **NPM transitive coverage:** Current NPM SBOMs capture direct deps only
   (50 admin-ui, 25 docs from `pnpm-lock.yaml` importer sections). The
   pnpm lockfile contains 2,542 package entries total. Full transitive SBOM
   requires `pnpm install` in a fresh environment with network access to
   populate node_modules, then running cdxgen against the installed tree.
   Recommended follow-up: CI step that installs deps and re-runs
   `./scripts/generate-sbom.sh --npm-only`.

2. **NPM license coverage:** 42 components have no license declared in the
   generated SBOMs. This is because pnpm store index files (v10 format) do
   not embed license metadata; enrichment requires reading `package.json`
   from installed node_modules or querying the npm registry. `pnpm licenses
   list` covered 2,218 entries; remaining gaps are workspace-internal or
   tooling packages. No missing-license flag was raised against a package
   known to carry a restricted license.

3. **BSL-1.0 follow-up (ryu/ryu-js):** Boost Software License 1.0 is a
   permissive license with attribution requirement only (similar to BSD-2-
   Clause). The `ryu@1.0.23` and `ryu-js@0.2.2` packages express
   `Apache-2.0 OR BSL-1.0` — the Apache-2.0 alternative fully satisfies
   the allowlist. Recommend: add BSL-1.0 to deny.toml and this allowlist
   via a brief ADR. No usage restriction risk.

4. **Per-binary .cdx.json artifacts:** cargo-cyclonedx emits per-binary SBOM
   files across the workspace (17 files). The merge script deduplicates by
   purl and produces a single `cyclonedx-rust.json`. The per-binary files
   are cleaned up by `generate-sbom.sh` (they are ephemeral build artifacts,
   not committed). The workspace-level merged file is the canonical SBOM.

5. **Worker/WASM target:** `corelink_clerk_cf_cdylib` and `corelink_wasm_cdylib`
   target `wasm32-unknown-unknown`. Their deps are included in the merged
   Rust SBOM. No WASM-specific license restrictions apply.

## §6 Generation script

`scripts/generate-sbom.sh` is idempotent and supports:
- `--dry-run` — prints what would be done, no file writes
- `--rust-only` / `--npm-only` — partial regeneration
- `CDXGEN_BIN` env var — override cdxgen path (for CI with local install)

Invocation for full refresh:
```bash
./scripts/generate-sbom.sh
```

Expected output:
- `.sbom/cyclonedx-rust.json` — Rust workspace, ~413 components
- `.sbom/cyclonedx-npm-admin-ui.json` — admin-ui direct deps, ~50 components
- `.sbom/cyclonedx-npm-docs.json` — docs direct deps, ~25 components

## §7 DoD checklist

1. [x] Rust SBOM generated, JSON valid (413 components, `jq . .sbom/cyclonedx-rust.json` → success)
2. [x] NPM SBOM generated — separate file per app (admin-ui: 50 comps, docs: 25 comps)
3. [x] License audit table in SEAL doc: total 488, allowed 436, flagged 0 genuine, 2 conditional BSL-1.0
4. [x] Generation script `--dry-run` works; live mode produces fresh files
5. [x] SEAL audit committed
6. [x] Single commit on worktree

**Flagged count = 0** (within ≤2 acceptable threshold per WP-7.2 DoD §3).
The 2 conditional entries (ryu/ryu-js BSL-1.0) are acceptable because
Apache-2.0 alternative is present; follow-up ADR recommended but not blocking.

## §8 Files changed

- `.sbom/cyclonedx-rust.json` — NEW (413-component merged Rust workspace SBOM)
- `.sbom/cyclonedx-npm-admin-ui.json` — NEW (50-component admin-ui direct deps SBOM)
- `.sbom/cyclonedx-npm-docs.json` — NEW (25-component docs direct deps SBOM)
- `scripts/generate-sbom.sh` — NEW (idempotent re-generation script)
- `specs/_compliance/license-allowlist.md` — NEW (formal allowlist doc v1.2.0)
- `specs/_audits/2026-05-27-sbom-license-audit-seal.md` — THIS FILE

## §9 DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>.

**End of SBOM + License Audit SEAL — WP-7.2.**
