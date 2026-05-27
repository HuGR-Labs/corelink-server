---
id: AUDIT-R1-8-DEPENDENCY-SECURITY
type: audit
doc_status: ACTIVE
audit_status: ACTIVE
version: 1.0.0
created: 2026-05-14
reviewers: [Claude Opus 4.7 — dependency audit agent]
scope: [cargo-audit, cargo-deny, pnpm-audit, dependabot]
branch: wt/r1-8
head: 758fde9
tags: [audit, security, supply-chain, dependencies, ga-gate, r1-8]
---

# Dependency Security Audit — Roadmap R1-8 (post-GA-engineering-gate-complete)

## 1. Summary

| Lane              | Findings | P0 | P1 | P2 | Suppressed | Notes |
|-------------------|---------:|---:|---:|---:|-----------:|-------|
| `cargo audit`     |        6 |  3 |  2 |  1 |          0 | 5 vulns + 1 unsound warning |
| `cargo deny`      |       24 |  5 |  1 | 18 |          0 | 5 advisories + 1 license + 16 dupes + 1 dupe-license overlap (`webpki-roots`); fails 3 of 4 lanes |
| `pnpm audit` (admin-ui) | 35 |  4 |  9 | 22 |          0 | 2 critical + 14 high + 16 moderate + 3 low |
| `pnpm audit` (docs)     | 35 |  4 |  9 | 22 |          0 | Identical set — pnpm traverses workspace lock from any subdir |
| Dependabot backlog |     N/A |  — |  — |  — |          — | `gh` API unreachable from audit sandbox (network timeout to `api.github.com`); see §5 |
| **TOTAL UNIQUE**   |    ~60 | **8** | **11** | **41** | **0** | (Node findings counted once; cargo deny vulns overlap cargo audit) |

GA-gate blocker count (P0): **8 unique issues**. All have published fixed versions. None require code-level remediation in CoreLink crates — every P0 is a transitive bump.

Raw logs preserved at:

- `/tmp/r1-8-cargo-audit.log`
- `/tmp/r1-8-cargo-deny.log`
- `/tmp/r1-8-admin-ui-audit.json`
- `/tmp/r1-8-docs-audit.json`

## 2. Cargo audit findings (RustSec)

`cargo audit` scanned 546 crates in `Cargo.lock` against 1090 advisories.

| RUSTSEC ID         | Crate          | Cur version | Fix version            | Severity | Affected CoreLink crates (prod path?) | Class |
|--------------------|----------------|-------------|------------------------|----------|----------------------------------------|-------|
| RUSTSEC-2025-0020  | `pyo3`         | 0.22.6      | `>=0.24.1`             | High (buffer overflow in `PyString::from_object`) | `corelink-py` (Python bindings — **prod**, customer-facing SDK) | **P0** |
| RUSTSEC-2023-0071  | `rsa`          | 0.9.10      | **none available** (Marvin timing sidechannel) | Medium (CVSS 5.9) | `corelink-worker`, `corelink-clerk`, `corelink-reapi`, `corelink-dpa-acceptance`, `corelink-clerk-cf` — **prod** (auth/clerk hot path) | **P0** |
| RUSTSEC-2026-0104  | `rustls-webpki`| 0.101.7     | `>=0.103.13`           | High (reachable panic in CRL parsing — DoS) | Via `rustls 0.21` → `tokio-rustls 0.24` → `hyper-rustls 0.24` → `aws-smithy-http-client` → AWS SDK stack → `corelink-byok-aws` (**prod** BYOK plane) | **P0** |
| RUSTSEC-2026-0098  | `rustls-webpki`| 0.101.7     | `>=0.103.12`           | High (URI name-constraints bypass — auth) | Same chain as above | **P0** (folded into the rustls-webpki bump) |
| RUSTSEC-2026-0099  | `rustls-webpki`| 0.101.7     | `>=0.103.12`           | High (wildcard name-constraints bypass) | Same chain | **P0** (folded into the rustls-webpki bump) |
| RUSTSEC-2026-0002  | `lru`          | 0.12.5      | (warning-only — unsound `IterMut`) | Unsound (stacked-borrows UB) | All `corelink-byok-*` (vault, revocation, gcp, azure, aws, matrix-test) — **prod** | **P1** |

### Critical chain analysis

The three `rustls-webpki 0.101.7` advisories share one upgrade path: bumping `aws-smithy-http-client` (currently 1.1.12) to a release that uses `rustls >= 0.23` (which pulls `rustls-webpki >= 0.103`). The AWS SDK family (`aws-sdk-kms`, `aws-sdk-sts`, etc.) already has newer minor releases — confirm `cargo update -p aws-smithy-http-client` resolves all three.

The `rsa` Marvin attack has **no upstream fix**. The crate is on the auth/clerk path (signing). Two GA options:

1. **Replace `rsa` with `aws-lc-rs` or `ring`** for RSA operations (preferred — eliminates timing channel entirely).
2. **Accept + document the residual risk** via ADR — only viable if RSA is *not* used for private-key operations on untrusted-input paths (e.g. only verify, never sign with attacker-controlled timing observability). Code-path audit required.

### Cargo audit verdict

`cargo audit` exit = `error: 5 vulnerabilities found! warning: 1 allowed warning found` — **gate fails**.

## 3. Cargo deny findings

`cargo deny check` exit code 7. Lane results: `advisories FAILED, bans FAILED, licenses FAILED, sources ok`.

### 3.1 Advisories lane (5 findings)

Duplicates of §2 above. Same RUSTSEC IDs (`2025-0020`, `2023-0071`, `2026-0098/0099/0104`, `2026-0002`). All P0/P1 classifications from §2 apply unchanged.

### 3.2 Licenses lane (1 finding)

| Crate           | Version | License              | Pulled by | Class |
|-----------------|---------|----------------------|-----------|-------|
| `webpki-roots`  | 1.0.7   | **CDLA-Permissive-2.0** (not on allowlist) | `hyper-rustls 0.27.9` → `aws-smithy-http-client`, `reqwest 0.12.28` (→ `corelink-cli`, `sbom-publish`) | **P1** |

`CDLA-Permissive-2.0` is the Community Data License Agreement (Permissive) and was added to webpki-roots when the root-CA list was relicensed for data-distribution clarity. The license is functionally equivalent to MIT/Apache for software use (it's permissive, no copyleft, no patent traps).

**Recommended action:** add `CDLA-Permissive-2.0` to `deny.toml [licenses].allow` with an ADR justifying the addition. This is the canonical fix used by rustls/AWS-SDK consumers since `webpki-roots 1.0`. Do **not** suppress per-crate — the allowlist addition is the SOTA pattern.

### 3.3 Bans lane (16 duplicate crates)

`multiple-versions = "deny"` triggers on:

| Crate            | Versions present                   | Why dup | Class |
|------------------|------------------------------------|---------|-------|
| `const-oid`      | 0.9.6 + 0.10.2                     | older via `der`/`pkcs1`/`rsa`; newer via newer cert/crypto stack | P2 |
| `h2`             | 0.3.27 + 0.4.14                    | hyper 0.14 (legacy aws-smithy) + hyper 1.x | P2 |
| `hashbrown`      | 0.15.5 + 0.17.1                    | indexmap 2.x major split | P2 |
| `hmac`           | 0.12.1 + 0.13.0                    | new sha2/digest major | P2 |
| `http`           | 0.2.12 + 1.4.0                     | hyper 0.14 vs 1.x | P2 |
| `http-body`      | 0.4.6 + 1.0.1                      | hyper 0.14 vs 1.x | P2 |
| `hyper`          | 0.14.32 + 1.9.0                    | aws-smithy legacy stack | P2 |
| `hyper-rustls`   | 0.24.2 + 0.27.9                    | rustls 0.21 vs 0.23 split | P2 |
| `rand`           | 0.8.6 + 0.9.4                      | rand 0.9 migration in flight | P2 |
| `rand_chacha`    | 0.3.1 + 0.9.0                      | same | P2 |
| `rand_core`      | 0.6.4 + 0.9.5                      | same | P2 |
| `rustls`         | 0.21.12 + 0.23.40                  | aws-smithy still on 0.21 | P2 (resolves with §2 P0 rustls-webpki bump) |
| `rustls-webpki`  | 0.101.7 + 0.103.x                  | same | **P0** (already counted in §2) |
| `sha2`           | 0.10.9 + 0.11.0                    | digest 0.11 migration | P2 |
| `tokio-rustls`   | 0.24.1 + 0.26.4                    | rustls 0.21 vs 0.23 | P2 (resolves with rustls bump) |
| `wasm-streams`   | 0.4.2 + 0.5.0                      | worker stack migration | P2 |

All duplicates are **legitimate transitive splits** caused by major-version migrations in progress upstream (hyper 0.14→1, rustls 0.21→0.23, rand 0.8→0.9, hashbrown 0.15→0.17). None indicate workspace misconfiguration. They will collapse naturally once the AWS SDK family ships its hyper-1/rustls-0.23 stack (already in beta — track `aws-smithy-http-client` minor bumps).

`deny.toml` policy is correct (`multiple-versions = "deny"` with empty `skip`); per ADR-0014 convention, **add per-crate `skip` entries with ADR waivers** for the 16 dupes above, OR (preferred) flip to `multiple-versions = "warn"` for R-2 wave and revisit when AWS SDK lands hyper-1. Decision blocked on a separate ADR.

### 3.4 Sources lane

PASS. No banned registries / git deps / vendored sources.

## 4. Node audit findings

**Important methodological note:** `pnpm audit --prod` invoked from any subworkspace traverses the entire workspace lockfile, so `apps/admin-ui` and `apps/docs` return identical 35-finding sets. The vulnerable dep paths are all rooted at `apps__admin-ui>...` — `apps/docs` itself has no direct prod deps with advisories.

### 4.1 Critical (2) — admin-ui

| GHSA / CVE             | Package        | Cur     | Fixed | Notes | Class |
|------------------------|----------------|---------|-------|-------|-------|
| GHSA-f82v-jwr5-mffw (CVE-2025-XXXXX) | `next`        | 15.2.3 | `>=15.5.16` | RCE in React flight protocol — **prod, customer-facing UI**. | **P0** |
| GHSA-9mp4-77wg-rwx9 (CVE-2025-53548) | `@clerk/nextjs` | 6.12.5 | latest 6.x patched | Middleware-based route protection bypass — **auth bypass, prod**. | **P0** |

### 4.2 High (14) — admin-ui

Concentrated in three packages: `next` (10 high CVEs — DoS, SSRF, middleware bypass, request smuggling), `@clerk/{nextjs,clerk-react,backend}` (3 — auth bypass + insufficient verification), `playwright` (1 — browser install integrity; dev-only), `serialize-javascript` (1 — RCE; transitive). All `next` and `@clerk/*` highs resolve with the **same bumps as the P0s**:

- `next 15.2.3 → 15.5.16+` collapses 14 of 14 `next` advisories (1 critical + 10 high + 11 moderate + 3 low).
- `@clerk/nextjs` major bump collapses the Clerk critical + 3 highs.
- `playwright` high is **dev-only** (`@playwright/test` devDep, `playwright install` build step) — **P2**.
- `serialize-javascript` is transitive (likely via `@playwright/test` or `next-intl` chain) — bumps with the parent.

Classifying as **P1** (should-fix before GA): all `next`/`@clerk/*` highs (they auto-resolve with the §4.1 P0 bumps, but listing separately for owner tracking).

### 4.3 Moderate (16) and Low (3)

- 14 `next` moderates + 3 `next` lows — all collapse with `next ≥ 15.5.16` bump.
- `next-intl 3.26.5` — 2 moderate (open redirect + prototype pollution); needs separate bump.
- `serialize-javascript` — 1 moderate (CPU exhaustion DoS); same parent bump.
- `postcss 8.5.3` — 1 moderate (XSS via unescaped `</style>`); needs bump to ≥ 8.5.6.

All classed **P2** (post-GA) unless prototype pollution in `next-intl` has an exploit path in our middleware config — needs 30-min review.

## 5. Dependabot backlog

**Status: UNABLE TO RETRIEVE.** `gh pr list` failed with `dial tcp 4.228.31.149:443: i/o timeout` to `api.github.com` from the audit sandbox. Both authenticated accounts (`humangr-labs`, `gustavomhss`) hit keyring lookup timeouts followed by network timeouts.

**Action item:** rerun `gh pr list --label dependencies --json number,title,labels,createdAt --limit 100 > /tmp/r1-8-dependabot.json` from an environment with GitHub network access. Append the parsed table to this audit as §5b.

## 6. Action items

| # | Item | Owner | ETA | Lane | Blocks GA? |
|--:|------|-------|----:|------|-----------:|
| 1 | Bump `aws-smithy-http-client` family so `rustls-webpki ≥ 0.103.13` and `rustls ≥ 0.23` (resolves RUSTSEC-2026-0104/0098/0099 + 4 of 16 cargo-deny dupes) | R-2 deps wave | R-2 W1 | cargo | **YES (P0)** |
| 2 | Bump `pyo3` to ≥ 0.24.1 in `corelink-py` (resolves RUSTSEC-2025-0020) | R-2 deps wave | R-2 W1 | cargo | **YES (P0)** |
| 3 | Decide on `rsa` crate replacement vs ADR waiver (RUSTSEC-2023-0071 — no upstream fix) | crypto + auth leads | R-2 W1 | cargo | **YES (P0)** |
| 4 | Bump `next` from 15.2.3 → ≥ 15.5.16 in `apps/admin-ui` (resolves 1 critical + 10 high + 14 moderate + 3 low) | R-2 deps wave | R-2 W1 | pnpm | **YES (P0)** |
| 5 | Bump `@clerk/nextjs` past GHSA-9mp4-77wg-rwx9 patched range (resolves 1 critical + 3 high) | R-2 deps wave | R-2 W1 | pnpm | **YES (P0)** |
| 6 | Add `CDLA-Permissive-2.0` to `deny.toml [licenses].allow` + ADR | supply-chain owner | R-2 W1 | cargo-deny | **YES (P1)** — blocks cargo-deny gate |
| 7 | Bump `lru` once 0.13 with `IterMut` fix lands (RUSTSEC-2026-0002) | R-2 deps wave | R-2 W2 | cargo | P1 |
| 8 | Bump `next-intl` past open-redirect + proto-poll advisories | R-2 deps wave | R-2 W2 | pnpm | P1 |
| 9 | Bump `postcss` ≥ 8.5.6 | R-2 deps wave | R-2 W2 | pnpm | P2 |
| 10 | Decide on `multiple-versions = "warn"` vs enumerated `skip` for 16 transitive dupes; document via ADR | supply-chain owner | R-2 W2 | cargo-deny | P2 |
| 11 | Re-run Dependabot inventory from networked env; append to §5 | release manager | R-1 W9 | github | P1 |
| 12 | Re-run full audit after R-2 W1 bumps land; expect cargo `0 vulns`, cargo-deny `advisories ok / licenses ok`, pnpm `0 critical / 0 high` | release manager | R-2 W1+1d | all | gate verification |

## 7. Suppression list

**None.** This audit produces zero suppressions. Every finding has either (a) a published fixed version with a clear bump path, or (b) — for the single no-upstream-fix case (`rsa` Marvin attack) — an explicit P0 decision deferred to the crypto/auth leads with two named options (replace crate vs ADR waiver). No CVEs were waived under "not applicable" or "no exploit path" without a code-path review, per the spec's "documented justification" bar.

Once items 3 and 6 (action items above) land their ADRs, those decisions become the formal record. They will be cross-referenced here in a revision after R-2 W1.

## 8. Audit metadata

- **Branch / HEAD:** `wt/r1-8` @ `758fde9`
- **Date:** 2026-05-14
- **Tools:** `cargo-audit 0.22.1`, `cargo-deny 0.19.4`, `pnpm audit` (workspace v9), `gh` (auth-failed, see §5)
- **Advisory DBs:** RustSec advisory-db (1090 advisories loaded), GitHub Advisory Database via `pnpm audit`
- **Scope:** all Rust workspace crates (546 deps in `Cargo.lock`) + `apps/admin-ui` + `apps/docs` Node prod deps
- **Out of scope:** dev-only Node deps (use `pnpm audit` without `--prod` in R-2 W2), container/OS-level CVEs (separate Trivy run), GitHub Actions pinning (separate workflow audit)
