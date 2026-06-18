# SOTA Security Hardening + Offensive-Verification Tooling Survey — CoreLink

**Date:** 2026-06-17
**Author:** AppSec architect survey (grounded in 2025–2026 sources; see footnotes)
**System under review:** CoreLink — multi-tenant content-addressable cache + storage-governance
on Cloudflare (TS Worker → Durable Objects → Rust Containers, ~71 crates → R2 + D1 + KV).
Cache surfaces: native CAS/AC (BLAKE3), Bazel REAPI v2 (SHA-256), Turborepo, sccache (WebDAV),
OCI registry (HMAC bearer). Auth = Clerk JWT + per-tenant PATs (Argon2id + HMAC). Billing = Stripe.
Self-serve SMB. Solo founder, cost-sensitive (COGS ~$5/tenant), CI = one chronically-overloaded shared Mac.

---

## 0. What CoreLink ALREADY has (verified in `.github/workflows/`)

The repo is **more mature than the brief assumed**. Confirmed present (do NOT re-recommend):

| Capability | Implementation found |
|---|---|
| SAST (Rust + TS dataflow) | `codeql.yml` (CodeQL) |
| SAST (fast, multi-lang + custom rules) | **`semgrep.yml`** — bundled rulesets + repo-root custom CoreLink rulepack, SARIF upload, fail-closed on ERROR |
| Coverage-guided fuzzing | **`fuzz-nightly.yml`** — cargo-fuzz/libFuzzer, **18 fuzz targets across 10 crates** (BYOK, tenant-path, audit-chain, reapi, ac, hash, meta, worker, client-verify, cli), warm persisted corpus, 30 min/target |
| SBOM | `sbom.yml` + `sbom-consolidated.yml` — **cargo-cyclonedx** (CycloneDX 1.6), NTIA validation, RFC-3161 TSA timestamp, Dependency-Track ingestion |
| Signing + provenance | **`cosign-sign.yml`** (keyless OIDC → Fulcio → Rekor) + `release-slsa3.yml` (SLSA provenance) + SBOM attestation |
| Dep advisories / licenses / bans | cargo-deny, cargo-audit, pnpm-audit |
| GHA hardening (partial) | `action-sha-audit.yml` + `actionlint.yml` + manual SHA-pinning of every action |
| Secrets **inventory** gate | `secrets-drift.yml` + secrets-matrix validators — **NB: this is env-var-inventory completeness, NOT credential scanning of source/git-history** |
| Formal / model checking | TLA+ (`tla_check.yml`), proptest, cargo-mutants, reproducible-build, clippy -D |
| Adversarial review | internal red-team agent workflows (corelink-redteam-*) |

**This reframes the survey.** The big remaining holes are: **credential scanning, image/SBOM CVE
scanning, GHA-workflow SAST (zizmor), DAST + cache-poisoning testing, Cloudflare-config scanning,
authz/multi-tenant policy testing, and bounded-proof verification (Kani).**

---

## 1. The 3 biggest gaps in current posture

1. **No credential scanning of source / git history.** `secrets-drift` only checks that env-var
   *names* are declared in the matrix. Nothing scans actual commits/working-tree for leaked
   `sk_live_…`, `whsec_…`, `CLOUDFLARE_API_TOKEN`, PATs, R2 keys. Given live Stripe/Clerk keys and
   a self-managed `.env.local`, a single bad commit is a launch-ender. **gitleaks + trufflehog.**
2. **No runtime/DAST exercise of the cache HTTP surfaces, and zero cache-poisoning testing of the
   crown jewels.** CAS/AC/REAPI/Turbo/OCI are HTTP APIs with caching semantics; you have *unit/property*
   coverage but no black-box probing of unkeyed-input cache poisoning, host/header reflection, or
   IDOR/BOLA across tenants. **nuclei + Web-Cache-Vulnerability-Scanner + Caido.**
3. **Built artifacts are never CVE-scanned and the GHA supply chain isn't SAST'd.** SBOMs are
   *generated* but never *scanned* (no grype/trivy), the OCI Worker image is never image-scanned, and
   despite manual SHA-pinning there's no `zizmor` to catch `pull_request_target`/injection/poisoned-cache
   workflow bugs — the exact class that hit tj-actions and trivy-action in 2025–2026. **grype + zizmor.**

---

## 2. SAST (Rust + TypeScript)

You have CodeQL **and** Semgrep already. Remaining SAST-adjacent additions are Rust-specific
deep checks, not another generic scanner.

### Semgrep — already adopted; lever the FREE platform tier
- **Cost:** CE is free (LGPL-2.1, 30+ langs, 3,000+ rules, single-file). The **hosted AppSec Platform
  is free for ≤10 contributors / ≤10 private repos** — includes Pro cross-file/cross-function dataflow,
  20,000+ Pro rules, Supply Chain, Secrets, and the **Semgrep Assistant (AI triage)**. Paid Team = **$35/contributor/mo** above that.[^semgrep]
- **Fit:** You already run CE in CI. As a **solo founder you are 1 contributor → the full Pro platform
  is FREE for you.** That upgrades your existing scan from single-file CE to cross-file Pro dataflow +
  AI false-positive triage at zero cost.
- **Effort:** Low — connect the repo to semgrep.dev, keep the CI job as the gate.
- **Gap closed:** deeper taint analysis + AI triage on top of what you have.
- **Verdict: ADOPT-NOW (free upgrade of an existing tool).**

### Kani (Rust bounded model checker) — `cargo kani` / `#[kani::proof]`
- **Cost:** Free, OSS (model-checking/kani, AWS).[^kani]
- **Fit:** Excellent and **strategically aligned** — you already invest in TLA+ for design-level
  model checking; Kani brings *exhaustive, non-random* verification to the **actual Rust code** (proves
  "can never panic / no UB / property holds for all inputs up to a bound"). Ideal for the integrity-critical
  kernels: BLAKE3/SHA-256 digest equivalence checks, tenant-path HMAC derivation, REAPI/OCI length/offset
  parsing, audit-chain append. Has experimental **PropProof** to drive your existing proptest harnesses
  symbolically — so you reuse harnesses you already wrote.
- **Effort:** Med — write `#[kani::proof]` harnesses for ~5 crown-jewel functions; runs in CI via the
  official Kani GitHub Action. Solver time can be heavy → run **nightly, not per-PR** (matches your
  staggered-nightly model; do NOT add to the shared-Mac per-PR path).
- **Gap closed:** bounded *proof* (not sampling) on the integrity/auth kernels — the layer above proptest+fuzz.
- **Verdict: ADOPT-NOW (free, nightly, aligned with existing TLA+ rigor).**

### Miri (UB interpreter) + cargo-geiger (unsafe census)
- **Cost:** Free, OSS.[^miri]
- **Fit:** Only worth it **where you have `unsafe`**. Run `cargo geiger` once to census unsafe across the
  71 crates; if there are non-trivial unsafe blocks (FFI, zero-copy parse, custom sync), add **Miri** on
  those crates' test suites to catch UB, and **loom** only if you have hand-rolled lock-free/atomics in
  the Durable-Object/container concurrency. If the codebase is overwhelmingly safe Rust, **skip Miri/loom**.
- **Effort:** Low to triage (geiger), med to wire Miri (nightly toolchain, slow, nightly-only).
- **Gap closed:** UB / data-race detection in unsafe code — only if such code exists.
- **Verdict: TRIAL (run cargo-geiger first; adopt Miri/loom only if unsafe surface justifies it).**

---

## 3. Coverage-guided fuzzing

You already run **cargo-fuzz/libFuzzer, 18 targets, warm corpus, nightly.** This is genuinely strong.
The improvements are *better engine* and *more targets*, not a new framework.

### Keep cargo-fuzz; consider LibAFL/Ziggy only for the highest-value parsers
- **cargo-fuzz (have it):** de-facto Rust standard, libFuzzer backend, multi-target ergonomics.[^fuzz] Keep it.
- **LibAFL / cargo-libafl:** SOTA modern fuzzer (custom mutators, multi-core, snapshot). Free, OSS.
  Materially better throughput/coverage on **structured protocol parsers** (REAPI v2 framing, OCI
  manifest/descriptor parsing, Turbo protocol). Higher integration cost.
- **Ziggy:** free OSS multi-fuzzer manager (runs honggfuzz + AFL++ on one shared corpus, with coverage
  dashboards).[^ziggy] Useful if you want AFL++/honggfuzz diversity without bespoke harness rewrites.
- **Fit:** Your engine is fine. The **gap is target coverage on the cache-poisoning surface**: add
  *structure-aware* fuzz targets for REAPI request framing, OCI manifest/descriptor/digest parsing, and
  Turbo artifact headers — feeding malformed input at the exact points where a hash/length mismatch could
  cause cross-tenant content confusion. Use `arbitrary`-derived structured inputs.
- **Effort:** Med — author ~4–6 new structured targets in existing `fuzz/` dirs; stay on cargo-fuzz.
  Reach for LibAFL only if a specific parser plateaus on coverage.
- **Gap closed:** coverage-guided exercise of the protocol parsers that gate content-addressing integrity.
- **Verdict: ADOPT-NOW (expand targets on cargo-fuzz). LibAFL/Ziggy = TRIAL for one stubborn parser.**

---

## 4. DAST / web pentest (the HTTP cache APIs)

**This is your single largest blind spot** — strong unit/property/fuzz coverage, zero black-box probing.

### nuclei (ProjectDiscovery)
- **Cost:** Free, OSS (MIT). 12,000+ community templates.[^nuclei]
- **Fit:** Strong. Template-based, fast, CI-friendly, headless, low resource. Templates cover **exactly your
  risk classes**: JWT algorithm confusion (your Clerk JWT path), BOLA/IDOR (cross-tenant), API-key exposure,
  SSRF, plus **web-cache-poisoning** templates. You can write **custom CoreLink templates** to probe each
  cache surface (e.g. assert an AC GET for tenant A can never return tenant B's blob; assert unkeyed
  `X-Forwarded-Host`/`Host` headers don't enter the cache key).
- **Effort:** Low–med — run against a preview/staging deployment, not prod; gate nightly or pre-release.
  Runs on the Mac or a free GH-hosted runner (offloads from the shared Mac).
- **Gap closed:** black-box known-vuln + custom-template probing of every HTTP cache surface.
- **Verdict: ADOPT-NOW (free, highest ROI for the DAST gap).**

### Web-Cache-Vulnerability-Scanner (WCVS, Hackmanit)
- **Cost:** Free, OSS (Go CLI).[^wcvs]
- **Fit:** **Purpose-built for your crown-jewel risk** — automates web-cache-poisoning AND web-cache-deception
  techniques (unkeyed headers/params, cache-key confusion), has a crawler, adapts to the specific cache, and
  integrates into CI. CoreLink IS a cache-in-front-of-multi-tenant-content product; this tool's entire reason
  for existing is your threat model.
- **Effort:** Low — point it at a staging URL per cache surface; nightly/pre-release.
- **Gap closed:** dedicated cache-poisoning/deception coverage that no SAST/fuzz layer gives you.
- **Verdict: ADOPT-NOW (free, directly on the crown jewels).**

### Caido (modern Rust intercepting proxy) — manual exploration
- **Cost:** **Community edition free**; **Pro from ~$10/mo** (verify current tier).[^caido]
- **Fit:** Great for *manual* pentest of auth/tenant-isolation logic (replay, tamper PAT/JWT, swap tenant
  IDs) — the logic bugs nuclei/WCVS won't find. Rust-based, light, fast. A modern Burp alternative.
- **Effort:** Low (interactive, not CI). For periodic hands-on review + your internal red-team agent runs.
- **Gap closed:** manual authz/business-logic probing.
- **Verdict: TRIAL free CE now; Pro ($10/mo) is cheap and worth it if you do regular manual sessions.**

### OWASP ZAP — automated crawl+active scan
- **Cost:** Free, OSS.[^zap]
- **Fit:** Good general DAST (spider + active scan, Docker headless, CI). Complements nuclei (ZAP finds
  app-logic issues; nuclei finds known CVEs/misconfig). For a pure-API product the value over nuclei+WCVS is
  marginal and it's heavier.
- **Verdict: TRIAL (only if nuclei+WCVS leave gaps; don't run all three in CI on the shared Mac).**

### Burp Suite Pro — **SKIP** at this stage
- **Cost:** ~$475/user/yr.[^burp] Excellent (Param Miner extension is the gold standard for unkeyed-input
  hunting), but Caido CE + WCVS + nuclei cover ~90% of it for $0. **Skip (expensive vs. free alternatives
  that fit a solo founder).** Revisit only if you hire an AppSec engineer.

---

## 5. Secrets scanning (genuine gap)

### gitleaks — fast regex, pre-commit + CI
- **Cost:** Free, OSS. 150+ patterns.[^secrets]
- **Fit:** Strong. Millisecond pre-commit hook that blocks `sk_live`, `whsec`, CF tokens, R2/AWS keys,
  PATs *before they ever land*. Custom rules for CoreLink token shapes (PAT prefix, internal-auth key).
- **Effort:** Low — pre-commit hook (local, zero CI cost) + a CI job. Add a custom rule for your PAT/whsec formats.
- **Gap closed:** prevents leaked-credential commits — your `secrets-drift` does NOT do this.
- **Verdict: ADOPT-NOW (free, the pre-commit half costs zero CI on the shared Mac).**

### trufflehog — verified-secret depth in CI
- **Cost:** Free, OSS. 800+ detectors with **live credential verification** (calls the provider to confirm
  the key is active → near-zero false positives).[^secrets]
- **Fit:** Strong. Run in CI (and one full history scan now) — its verification means a hit is a *real,
  live* leak, not noise. Also scans beyond git (R2/S3 buckets, Docker images).
- **Effort:** Low — CI job + one historical sweep of the repo.
- **Gap closed:** verified detection in history + non-git sources.
- **Verdict: ADOPT-NOW. Standard pairing: gitleaks pre-commit (speed) + trufflehog CI (verified depth).**

---

## 6. Supply chain / SLSA / SBOM / provenance / image scanning

You already have SBOM (cargo-cyclonedx + DT + TSA), Cosign keyless, SLSA-3 provenance, cargo-deny/audit.
**The hole: nothing CVE-SCANS the SBOM or the built image, and the GHA workflows aren't SAST'd.**

### grype — scan the SBOM you already produce
- **Cost:** Free, OSS (Anchore).[^supply]
- **Fit:** High and **near-zero marginal effort** — you already emit CycloneDX SBOMs; grype consumes an SBOM
  directly and reports CVEs (OS + language deps). Closes the loop: "we generate an SBOM" → "we act on it."
- **Effort:** Low — one CI step: `grype sbom:./sbom.cdx.json`. Fail-closed on High/Critical.
- **Gap closed:** the build is currently never checked against a CVE feed at the artifact level.
- **Verdict: ADOPT-NOW (free, trivial, completes existing SBOM investment).**

### trivy — image + filesystem + IaC, one tool
- **Cost:** Free, OSS (Aqua).[^supply]
- **Fit:** High. Scans the **OCI Worker image** (`ghcr.io/...corelink-worker`) for CVEs, AND **absorbed the
  entire tfsec ruleset** so it also does IaC/Dockerfile/misconfig scanning (one tool, two gaps).
- **Effort:** Low — CI step post-build, pre-push.
- **⚠️ Supply-chain caveat:** `trivy-action` itself was compromised via a `pull_request_target` misconfig in
  2025–2026.[^trivyhack] **SHA-pin it** (you already SHA-pin everything) and prefer invoking the trivy
  *binary* over the action, or run grype instead for the SBOM half. The tool is fine; the action wrapper was
  the risk.
- **Gap closed:** image CVE scan + Dockerfile/IaC misconfig.
- **Verdict: ADOPT-NOW for image+IaC (SHA-pinned, binary-invoked). grype + trivy together cover SBOM+image+IaC.**

### OpenSSF Scorecard — automated supply-chain posture grade
- **Cost:** Free for repos via the official GHA action.[^scorecard]
- **Fit:** Good. Auto-grades branch protection, pinned actions, dangerous workflows, token permissions,
  signed releases, etc. — a self-audit that *offloads thinking* (valuable for a solo founder) and produces
  SOC2-friendly evidence. You already do most of what it checks; this proves it and catches drift.
- **Effort:** Low — drop-in action, weekly + on push.
- **Gap closed:** continuous, scored supply-chain hygiene self-audit.
- **Verdict: ADOPT-NOW (free, low-effort, generates compliance evidence).**

### zizmor — SAST for GitHub Actions workflows (genuine gap)
- **Cost:** Free, OSS (Trail of Bits / pynt).[^zizmor]
- **Fit:** **High and timely.** You manually SHA-pin and run `actionlint`, but neither catches the
  *semantic* workflow vulns: `pull_request_target` + checkout-of-PR-code, template-injection via
  `${{ github.event.* }}`, over-broad `GITHUB_TOKEN`, cache-poisoning between workflows. This is the exact
  class behind tj-actions and the trivy-action breach. With **70+ self-hosted-Mac runners exposed to PR
  workflows**, a workflow-injection bug is catastrophic (RCE on the founder's Mac + secret exfil).
- **Effort:** Low — `zizmor .github/workflows/`, add as a CI gate (or pre-commit).
- **Gap closed:** the one supply-chain surface most likely to be exploited given self-hosted runners.
- **Verdict: ADOPT-NOW (free, low-effort, closes a self-hosted-runner RCE class).**

### cargo-vet — **TRIAL / mostly SKIP** for now
- **Cost:** Free, OSS (Mozilla).[^vet] High-friction (manual per-crate audits / trusted-org imports).
  Sources note it's best for crypto/firmware/financial code; for most services cargo-deny + reviewed
  Cargo.lock is the right baseline — **which you already have.** Adopt cargo-vet only if you want to import
  Mozilla/Google audit sets to raise assurance on the crypto crates with low effort.
- **Verdict: TRIAL (import-only mode on crypto crates); otherwise SKIP — you already have the baseline.**

### cargo-auditable — embed SBOM in the binary
- **Cost:** Free, OSS.[^supply] Embeds dependency data in the compiled binary so `cargo audit bin` / trivy
  can scan the *shipped artifact* even without the source SBOM. Low effort, nice-to-have.
- **Verdict: TRIAL (low effort, complements grype/trivy on the actual binary).**

---

## 7. Dependency-confusion / typosquat defense

- **You're largely covered** by cargo-deny `sources` (registry allowlist/bans) + reviewed Cargo.lock +
  SHA-pinned actions. For the **TS/pnpm side**, ensure: scoped packages, `pnpm` with a locked registry,
  and a min-package-age / new-dependency review policy. **Socket.dev** (free tier for OSS / GitHub app)
  adds behavioral detection of malicious/typosquatted npm+crates packages (install scripts, network access,
  obfuscation) beyond CVE feeds.[^socket] Verify current free-tier limits.
- **Verdict: TRIAL Socket.dev free tier for the npm side; cargo side already hardened by cargo-deny.**

---

## 8. IaC / Cloudflare-config scanning

- **Reality check:** there is **no mature, Cloudflare-Workers-aware IaC scanner.** Checkov/trivy/tfsec
  target Terraform/K8s/CFN/Dockerfile, not `wrangler.jsonc`/`wrangler.toml` semantics (DO bindings, R2/D1/KV
  bindings, routes, secrets-vs-vars).[^iac] So:
  - Use **trivy** (already recommended) for the **Dockerfile + any Terraform** you have (it ate tfsec's rules).
  - For **wrangler config specifically**, write **custom Semgrep rules** (you already have a custom Semgrep
    rulepack) to assert CoreLink invariants: no secret in plaintext `vars`, no wildcard routes that bypass
    auth, no `compatibility_flags` that weaken isolation, every binding namespaced by tenant. This is the
    *correct* SOTA move here — your own policy-as-code, since no off-the-shelf tool knows your DO/R2 model.
- **Effort:** Low–med — extend the existing Semgrep custom pack with ~6 wrangler-config rules.
- **Verdict: ADOPT-NOW custom Semgrep rules for wrangler config; trivy covers Dockerfile/TF. No new tool needed.**

---

## 9. Threat modeling

- **OWASP Threat Dragon** (free, OSS, browser/desktop) — visual STRIDE diagrams, good for a one-time
  documented model of the Worker→DO→Container→R2/D1 data flows + trust boundaries (Clerk JWT boundary,
  PAT boundary, tenant boundary, internal-auth boundary). Produces an artifact for SOC2 + onboarding.[^td]
- **pytm** (free, OSS) — threat-model-as-code in Python; better if you want the model in-repo, diffable,
  and CI-checkable. Both are joining the CycloneDX TMBOM effort.[^pytm]
- **Fit:** For a solo founder, a **one-time Threat Dragon model** of the trust boundaries is high-value and
  low-cost; pytm only if you want it living in CI.
- **Verdict: ADOPT-NOW Threat Dragon (one-time documented model). pytm = TRIAL (if you want it in-repo).**

---

## 10. Multi-tenant isolation / authz testing

- **The real SOTA move here is custom, not a product.** Your isolation logic lives in Rust (tenant-path
  HMAC, PAT scope, internal-auth). The best tooling:
  1. **Property/fuzz tests for the isolation invariant** — you already have proptest + 18 fuzz targets +
     a `tenant-path` fuzz crate. **Expand**: a proptest/Kani harness asserting "for all tenant A ≠ B, no
     request authenticated as A can resolve a key/blob owned by B" across CAS/AC/REAPI/Turbo/OCI. This is
     more authoritative than any external authz fuzzer because it runs against your actual resolver.
  2. **nuclei/WCVS custom templates** (§4) for the black-box cross-tenant IDOR/BOLA assertion against staging.
  3. **OPA/Rego** — only worth it **if you externalize authz into policy.** Today your authz is in-code
     (Argon2id/HMAC/scope checks); bolting on OPA adds a runtime dependency and latency on the hot cache path
     for little gain. `opa test` is great *if* you adopt Rego, but adopting Rego just to test it is the tail
     wagging the dog. **Skip OPA unless you decide to externalize policy for product reasons.**
- **Verdict: ADOPT-NOW the in-code isolation-invariant proptest/Kani harness + nuclei cross-tenant template.
  SKIP OPA/Rego (don't add a runtime authz engine you don't need).**

---

## 11. Runtime protection / WAF / rate-limit beyond stock Cloudflare

- You're on Cloudflare; the right answer is **use more of what you already pay for**, not a third-party WAF:
  Cloudflare **WAF managed rulesets + custom rules + rate-limiting rules + Bot Management + Turnstile** on
  the auth/checkout endpoints. (Memory note: a zone-wide rate-limit rule already once 429'd the SPA's own JS
  — so tune rules to exclude `/_next/` and asset paths.) A third-party RASP/WAF in front of Cloudflare is
  redundant and costly. For application-layer rate-limit fairness *per tenant* (quota abuse / DoS), enforce
  it in the Worker/DO keyed by tenant_id — which is product code, not a tool to buy.
- **Verdict: SKIP third-party WAF/RASP (intangible cost over stock Cloudflare). Invest in CF WAF/rate-limit
  config + per-tenant limits in code.**

---

## 12. AI-assisted security

- **Semgrep Assistant** — included in the free ≤10-contributor platform tier (§2). AI triages findings,
  proposes fixes, auto-suppresses false positives. **Free for you; adopt with the Semgrep platform upgrade.**[^semgrep]
- **CodeQL + Copilot Autofix** — GitHub's AI-suggested fixes on CodeQL alerts; **free for public repos**,
  part of GitHub Advanced Security (paid) for private. Verify your repo's entitlement; if private without GHAS,
  skip the autofix half but keep CodeQL.[^scorecard]
- **Your internal red-team agent workflows** are already a SOTA AI-assisted offensive layer — keep running
  them (corelink-redteam-brutal/nuclear) as the adversarial complement to the static tools above.
- **Verdict: ADOPT-NOW Semgrep Assistant (free with the tier upgrade). CodeQL Autofix = verify entitlement.**

---

## 13. Cache-poisoning / content-addressing integrity (CROWN JEWELS — cross-cutting)

This is not one tool but a *layered campaign*; pulling the relevant picks together:
1. **Structure-aware fuzzing** of REAPI/OCI/Turbo parsers (§3) — malformed digest/length/offset inputs.
2. **Kani bounded proofs** (§2) on digest computation + key derivation — prove no input maps two distinct
   contents to one address, and no tenant boundary is crossable up to a bound.
3. **WCVS + nuclei custom templates** (§4) — black-box: assert unkeyed headers never enter the cache key,
   assert cross-tenant GET isolation on every surface.
4. **In-code isolation-invariant property test** (§10) — the authoritative assertion against your real resolver.
5. **OCI specifically:** Cosign you already have for *your* published image; for the **registry surface you
   expose to tenants**, the integrity story is digest-pinning + bearer-scope enforcement — test it with
   nuclei templates for JWT/bearer confusion + the cross-tenant isolation harness. No off-the-shelf "OCI
   pentest tool" beats targeted fuzz + property tests here.

---

## TOP-7 "ADOPT NOW" shortlist (free-first)

| # | Tool | Cost | Gap it closes | Effort |
|---|---|---|---|---|
| 1 | **gitleaks (pre-commit) + trufflehog (CI)** | Free | Leaked-credential commits — your secrets-drift does NOT scan for this | Low |
| 2 | **nuclei + Web-Cache-Vulnerability-Scanner** | Free | DAST + cache-poisoning on the crown-jewel HTTP surfaces (your #1 blind spot) | Low–Med |
| 3 | **zizmor** | Free | GHA-workflow injection/`pull_request_target` RCE class — critical with self-hosted Mac runners | Low |
| 4 | **grype (scan existing SBOM) + trivy (image+IaC, SHA-pinned)** | Free | Artifact/image CVE scan — SBOMs are generated but never scanned today | Low |
| 5 | **Semgrep platform free tier + Assistant** (you're 1 contributor) | Free | Upgrade existing CE → Pro cross-file dataflow + AI triage at $0 | Low |
| 6 | **Kani bounded proofs** on digest/HMAC/parse kernels (nightly) | Free | Exhaustive proof layer above proptest+fuzz on integrity/auth | Med |
| 7 | **OpenSSF Scorecard** + **custom Semgrep wrangler-config rules** | Free | Supply-chain posture grade + Cloudflare-config policy-as-code | Low |

(Honorable mention, all free: expand cargo-fuzz structured targets for REAPI/OCI/Turbo; in-code
cross-tenant isolation property test; one-time OWASP Threat Dragon model.)

## "WORTH PAYING FOR" shortlist (with real $)

| Tool | $ | Payoff | Verdict |
|---|---|---|---|
| **Caido Pro** | ~$10/mo (verify) | Modern Rust intercepting proxy for regular manual authz/tenant-isolation pentest sessions; trivially cheap | Worth it IF you do hands-on sessions; CE is free to start |
| **Socket.dev** (paid tier) | Free tier first; paid scales per-seat (verify) | Behavioral malicious-package detection for npm (install-script/network/obfuscation) beyond CVE feeds | Worth a look; start free |

## SKIP (intangible / expensive vs. free alternatives)

- **Burp Suite Pro** (~$475/yr) — Caido CE + WCVS + nuclei cover ~90% for $0. Revisit only with an AppSec hire.
- **Third-party WAF/RASP** — redundant over stock Cloudflare WAF + rate-limit; invest in CF config + per-tenant limits in code.
- **OPA/Rego** — don't add a runtime authz engine just to test it; your authz is in-code and better tested by property/fuzz.
- **cargo-vet (full)** — high-friction; you already have the cargo-deny + reviewed-lockfile baseline (import-only mode = TRIAL).
- **Enterprise SAST/DAST suites** (Snyk/Veracode/Checkmarx tiers) — opaque/expensive; CodeQL + Semgrep already cover SAST.

---

### Note on the shared-Mac constraint
Almost every adopt-now pick is **free** and either runs **pre-commit (zero CI)**, on **free GitHub-hosted
runners** (offloading the Mac entirely — true for gitleaks/trufflehog/zizmor/grype/trivy/scorecard/nuclei
against staging), or **nightly/staggered** (Kani, expanded fuzz) to match the existing load-shedding model.
Do **not** add heavy tools to the per-PR path on the self-hosted Mac.

---

## Footnotes (sources, 2025–2026)

[^semgrep]: Semgrep pricing — CE free (LGPL-2.1, 30+ langs, 3,000+ rules); platform free ≤10 contributors/repos (Pro dataflow, 20k+ rules, Supply Chain, Secrets, Assistant); Team $35/contributor/mo. semgrep.dev/pricing; dev.to "Semgrep Pricing in 2026"; appsecsanta.com/semgrep; github.com/semgrep/semgrep.
[^kani]: github.com/model-checking/kani; model-checking.github.io/kani (cargo integration, `#[kani::proof]`, PropProof, GitHub Action). Free, OSS (AWS).
[^miri]: github.com/rust-lang/miri; markaicode.com Rust static analysis comparison 2025; geiger-rs/cargo-geiger; loom (model checker for concurrency). All free, OSS.
[^fuzz]: rust-fuzz.github.io/book/cargo-fuzz.html; github.com/rust-fuzz/cargo-fuzz; fuzzinglabs.com (cargo-libafl); appsec.guide cargo-fuzz. cargo-fuzz = libFuzzer backend, de-facto standard.
[^ziggy]: github.com/srlabs/ziggy (multi-fuzzer manager: honggfuzz + AFL++, shared corpus). Free, OSS.
[^nuclei]: ProjectDiscovery nuclei — free, MIT, 12,000+ templates incl. JWT confusion, BOLA/IDOR, cache-poisoning, SSRF. appsecsanta.com/api-security-tools; helpnetsecurity 2025-01.
[^wcvs]: github.com/Hackmanit/Web-Cache-Vulnerability-Scanner — Go CLI, cache poisoning + deception, crawler, CI-integrable. helpnetsecurity.com 2025-01-23; kali.org/tools.
[^caido]: Caido — Rust intercepting proxy; CE free, Pro from ~$10/mo (verify current pricing). appscan.dev best-free-DAST-2026.
[^zap]: OWASP ZAP — free/OSS, spider+active scan, Docker headless. appsecsanta.com/zap; owasp.
[^burp]: PortSwigger Burp Suite Pro ~$475/user/yr; Param Miner extension for unkeyed-input/cache-poisoning. portswigger.net (verify current price).
[^secrets]: gitleaks (150+ patterns, fast regex, pre-commit) vs trufflehog (800+ detectors, live verification, non-git sources). Both free/OSS. jit.io trufflehog-vs-gitleaks; appsecsanta.com.
[^supply]: Anchore syft/grype + Aqua trivy + cargo-auditable — all free/OSS; grype consumes CycloneDX SBOM directly; trivy = image+fs+IaC (absorbed tfsec). github.com/anchore/syft; env0 Checkov-vs-Trivy 2026.
[^trivyhack]: trivy-action `pull_request_target` supply-chain compromise (2025–2026) — SHA-pin / prefer binary invocation. snyk.io/articles/trivy-github-actions-supply-chain-compromise; zestsecurity.io.
[^scorecard]: OpenSSF Scorecard — free GHA action for public repos; checks pinned actions, dangerous workflows, token perms, branch protection. scorecard.dev; github.com/ossf/scorecard-action.
[^zizmor]: zizmor — GHA-workflow static analyzer (Trail of Bits hardened it for anchor support, 2026); catches pull_request_target/injection/cache-poisoning. blog.trailofbits.com 2026-05-22; arxiv 2601.14455 (scanner coverage study).
[^vet]: Mozilla cargo-vet (audit-import model) vs cargo-crev (web-of-trust). High-friction; cargo-deny baseline sufficient for most services. mozilla.github.io/cargo-vet; logrocket Rust supply-chain comparison; systemshardening.com.
[^socket]: Socket.dev — behavioral malicious-package detection (install scripts/network/obfuscation) for npm + crates; free tier for OSS/GitHub app (verify limits).
[^iac]: Checkov (Palo Alto, v3.2.x), trivy (absorbed tfsec), tfsec (frozen), Terrascan (archived Nov 2025) — all Terraform/K8s/CFN/Dockerfile-oriented; none are wrangler-config-aware. env0; policyascode.dev; spacelift.io.
[^td]: OWASP Threat Dragon — free/OSS visual STRIDE; TMBOM/CycloneDX export incoming. owasp.org/www-project-threat-dragon.
[^pytm]: OWASP pytm — threat-modeling-as-code (Python); CycloneDX TMBOM participant. owasp.org/www-project-pytm; devguide.owasp.org.
