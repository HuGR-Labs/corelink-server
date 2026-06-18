# SOTA launch-hardening tooling — roadmap & decisions (2026-06-18)

TL synthesis of the 3 research reports (`docs/research/2026-06-17-sota-*`) + what I
actually shipped/sized in the autonomous run. Free unless noted. Ordered by value÷effort.

## ✅ DONE this run — PR #330 (`feat/launch-hardening-scanners`)
- **gitleaks** (secret scan) + **trivy** (JS-dep CVE + IaC misconfig) CI gates, on the
  self-hosted **Linux** fleet (off the Mac). Custom `whsec_` rule. Found + fixed: 1 real
  leaked Stripe webhook secret (redacted — **owner: confirm rotation**), GCP WIF
  fail-closed (GCP-0068), `ws` CVE. Workflows are **zizmor-clean**.

## 🔴 NEW FINDING from this run — CI-security exposure (decide soon)
`zizmor` (GHA-workflow SAST) on the 99 workflows surfaced **316 findings total**
(high-severity errors: **98 template-injection · 26 excessive-permissions · 9
dangerous-triggers · 4 cache-poisoning**; plus 171 low `artipacked`/persisted-creds).
Runners = the founder's Mac → this is the **tj-actions/breach attack class**. Mitigated
today by "private repo, no external PRs", but a real pre-launch hardening item.
**Why NOT done tonight (deliberate, rigor > speed):** the template-injection hits cluster
in **release/signing/supply-chain** workflows (`release-slsa3`, `cosign-sign`, `sbom`,
`release-notes`, `compliance-weekly`, `reproducible-build`) — a wrong edit there breaks the
SLSA-3 attestation chain. Needs a careful, phased, owner-aware fix-wave (the env-passing fix
is mechanical but each release/signing workflow must be re-verified), NOT a blind madrugada
sweep. Recommend: phase 1 = template-injection (mechanical env: moves, verify each), phase 2
= the `artipacked` sweep (persist-credentials:false on non-pushing checkouts), phase 3 =
excessive-permissions + dangerous-triggers (semantic — highest care). `zizmor` is installed
locally (brew, v1.25.2) for the fix-wave.

## Next (free, mostly off-Mac) — value÷effort order
| Tool | Gap it closes | Effort | Note |
|---|---|---|---|
| **zizmor fix-wave** (above) | CI-security on self-hosted runners | M (focused) | the 71 errors; highest-value next |
| **loom + shuttle** | **concurrency** verification — would've caught the byte-accounting TOCTOU races we keep finding (Turbo C1, CAS/AC C2) | M–L | ROI #1 on correctness; wire into accrue/release + the per-key locks; touches Rust+Mac |
| **nuclei + WCVS** (DAST) | **zero DAST today** — cache-poisoning probes on CAS/AC/REAPI/Turbo/OCI/brew | M | needs a staging target + content-addressing templates; nightly |
| **iai-callgrind** | no perf-regression gate; instruction-count, immune to Mac noise (Linux CI) | M | hot kernels: BLAKE3/SHA-256/Argon2id/accounting |
| **llvm-cov diff-coverage GATE** | coverage is measured but **not gated** today | S–M | flip existing tool to gating; risk: blocks PRs if cov drops — baseline first |
| **Kani** | bounded proofs on digest/HMAC/accounting (above proptest+fuzz) | M | aligns with the TLA+ work |
| **OCI/REAPI/Turbo conformance + Schemathesis** | no protocol-conformance proof; black-box → off-Mac | M | conformance seal |
| **grype** | CVE-scan the **built OCI image** (trivy already covers dep manifests) | S | marginal vs trivy for PRs unless we scan the image layers |

## Worth a little $ (later, not now)
Caido Pro (~$10/mo, manual authz/tenant-isolation proxy) · CodSpeed (free ≤5 users — perf
gate on PR) · Honeycomb (free 20M ev/mo — multi-tenant latency) · **Blacksmith** (free 3k
min/mo — **move CI off the shared Mac**, the chronic-pain meta-fix). Skip: Antithesis, Burp Pro.

## 🔴 INFRA — self-hosted Linux runner down/saturated (found ~03:1x UTC 2026-06-18)
The `[self-hosted, Linux, X64]` runner stopped servicing this repo mid-run (cargo-deny ran
on it ~00:1x local, then nothing): **all Linux CI is stuck queued** — gitleaks, trivy,
cargo-deny, reproducible-build (queued 13h). 0 jobs in_progress anywhere; the 4 online
macOS `corelink-builder` runners sit idle (label mismatch). **The new gitleaks/trivy gates
(merged #330/#331) cannot validate until this runner is back** (both were LOCAL-verified).
- **Owner action:** revive the Linux runner (`svc.sh start` / check the Linux box or the
  org-level runner). Then the queued gitleaks/trivy/cargo-deny/reproducible-build all run.
- I did NOT move gitleaks/trivy to the idle macOS fleet on purpose: trivy pulls a ~700MB
  vuln DB = exactly the disk-pressure to avoid on the 98%-full Mac. Linux is the right home.
- **Launch-grade fix (roadmap):** **Blacksmith** (free 3k min/mo, drop-in `runs-on:`,
  off-Mac Linux) ends this chronic single-runner fragility.

## Owner actions surfaced by this run
1. **Revive the self-hosted Linux runner** (above) — blocks all Linux CI incl. the new gates.
2. **Rotate** the leaked Stripe `whsec_` (it's stale/dead per the doc, but confirm in Stripe).
3. Decide the **zizmor fix-wave** go (the 316 CI-security findings; 98 template-injection).
4. (pre-existing) Better Stack `--apply` + verify a PagerDuty page lands + add email fallback —
   see `docs/launch/2026-06-18-uptime-monitoring-status-and-activation.md`.
