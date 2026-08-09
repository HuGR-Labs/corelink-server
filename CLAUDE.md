# CLAUDE.md

Context for AI agents working in this repo. Keep it lean + high-signal.

## What CoreLink is

A **multi-tenant content-addressable cache + storage-governance platform** on
Cloudflare (Workers + Durable Objects + Containers + R2 + D1). ~73 Rust crates.
Sold **self-serve to SMBs** — NOT enterprise, and NOT "just a build cache."
It already exposes multiple cache surfaces: native CAS/AC, **Bazel REAPI v2**
(`routes/bazel_v2.rs`), **Turborepo** (`routes/turbo_v8.rs`), and **sccache** (WebDAV).

## Architecture wiki (OKF)

`docs/knowledge/` is the **code-grounded architecture wiki** — 160 OKF concepts,
each naming the `source_files` it explains (anti-drift gated against them).
Browse `docs/knowledge/index.md`, or the rendered site `docs/okf-wiki-site/index.html` (search + cross-link graph; regen via `scripts/okf_render.py`). **Rule: before modifying a
subsystem, load its concepts first** — don't work blind. Query them with
`python3 scripts/okf_context.py --file <path>` / `--tag <area>` (add `--full`
for bodies), or invoke the **`okf-context`** skill.

## Working on this repo — gotchas that actually bite

- **Toolchain:** the rustup proxy is broken. Put the toolchain on PATH:
  `PATH="$HOME/.rustup/toolchains/1.91.1-x86_64-apple-darwin/bin:$PATH" cargo …`
- **The self-hosted CI runners ARE the founder's Mac.** Heavy compiles + CI bursts
  overload it (it has crashed). Use `CARGO_BUILD_JOBS=4` for local builds; don't run
  big compiles while CI is hammering; cancel storms; keep the dependabot backlog low
  (a pile of open dependabot PRs + auto-rebase re-floods the Mac on every main merge).
- **Cloudflare secrets are write-only** — you can list deployed secret *names* via the
  CF API but never read values back. Real values live only in `.env.local` (gitignored,
  must be backed up). `CLOUDFLARE_API_TOKEN` in `.env.local` has D1 + Workers read/write.
- **`.env.local` keys are TEST keys** (`sk_test_…`). Live keys are the operator's
  launch-day step (their Clerk/Stripe dashboards).

## Gates (must stay green before merge)

- `python3 scripts/validate_specs.py` → **469 full-schema + 11 YAML-only (480 total), 0 failures**.
- Secrets matrix: `bash scripts/secrets-checklist-verify.sh` (OK, no drift) +
  `python3 scripts/validate_secrets_matrix.py` (code_only=0). Both exclude build output
  (`.open-next`/`.wrangler`) — don't let them scan generated bundles.
- `feat:`/`fix:` commits **require a CHANGELOG.md `[Unreleased]` entry** (changelog gate).
- Commits need a **`Signed-off-by:`** trailer (DCO).
- Branch protection `required checks = []`, but **merge only when CI is green** (impeccable).
- `corelink-container` (pkg `corelink-server`) is on the **proptest-density allowlist**.

## ⛔ Before merging ANY PR — do not skip

**Merge with ONE command: `bash scripts/pre-merge-gate-check.sh --merge <PR>`.**
It gates, then `gh pr merge --squash` only if every check is green — the merge
is unreachable otherwise — deletes the merged remote branch itself, and exits on
whether the PR merged, not on whether local cleanup worked (#1051). A **draft**
is refused outright, `--admin-reason` included (#1048). **Never chain
`pre-merge-gate-check.sh <PR> | tail -N && gh pr merge`:** a pipeline's exit
status is `tail`'s, so the gate's refusal is discarded — that is how #1049
merged with 4 checks pending. The report-only form (no flag) is unchanged: run
it and merge ONLY if it prints all-green. The heavy gates (coverage / CodeQL / ffi-matrix / reproducible-build
/ cas-foundation / s10-ship-gate) were moved OFF per-PR (2026-06-02) and run
**on a cron + on-demand only** — no `pull_request`, no `push` lane — so nothing
gates on them between scheduled runs. Cadence after the 2026-08-01 CI cost diet:
**CodeQL stays nightly; reproducible-build (Wed) + coverage / cas-foundation /
ffi-matrix / s10-ship-gate are WEEKLY** (Tue/Wed/Thu/Fri) — and TLA+ (`tla_check`)
had its cron REMOVED 2026-08-02 (now PR-path + on-demand only). Dispatch the weekly
ones explicitly when a PR touches their surface. The checks that REMAIN on a PR
are the fast, load-bearing ones and they MUST be green. Never blind `--admin` merge; if you must `--admin`, state the documented
infra/flake reason explicitly — `--merge --admin-reason "<why>"`, which the
script refuses on draft/pending/conflicting/missing-gate states (those never ran).
(A green PR now takes minutes, not 30+.)

**2026-08-02 correction — the "cron only" rule no longer covers everything it
used to.** 14 workflows that DID have a `pull_request` trigger were still also
running a daily cron, i.e. re-verifying byte-identical code on a clock. Their
crons were removed (`tla_check`, `region_pinning`, `cargo-deny`, `s07/s08/s09-
ship-gate`, `corelink-worker/meta/hash/reapi/client-verify/adapter-host`,
`tenant-path`, `corelink-server`). They are PR-gated and on-demand now.

**The rule to apply going forward: a cron earns its keep ONLY when something can
change WITHOUT a commit** — CVE feeds (cargo-audit / semgrep / CodeQL / trivy /
pnpm-audit), prod state (e2e-prod / canaries / smoke), backups, cert + infra
drift, billing reconciliation, DR drills. Those keep their schedules. Anything
that only changes when code changes belongs on a PR/push trigger. Measured
before the change: 10 of the previous 14 days had ZERO commits to `main`, while
the self-hosted Mac — the founder's own machine — burned ~8.8 h/day on scheduled
jobs, 75% of which failed.

## Workflow

- Owner mandate: **zero debt, no loose ends, impeccable repo.** Verify claims; never
  loosen rigor without an explicit waiver.
- Use the `/techlead` skill to review before merging. Branch → PR → merge (no direct
  pushes to `main`). End commit messages with the
  `Co-Authored-By: Claude …` trailer; end PR bodies with the Generated-with footer.

## Don't touch

Other projects share the parent dir (`hugr-wallet`, `HuGR-Smith`, `HuGR-Arsenal`,
Datadog/other runners, `hugr-juiceshop`, etc.). **Only work on CoreLink.**

## Product strategy & roadmap

- **Launch (now):** the cache + storage-governance product. Auth = **Clerk**, billing =
  **Stripe** (the already-built, audited path — not the wallet, not Keycloak).
  Keystone in flight: the `tier_select.rs` checkout backend.
- **Expansion campaign #1 (post-launch, phase 3):** **CI / build-acceleration** —
  ephemeral runners on cheap third-party infra (Hetzner-class) + the cache. It is a
  **feature / natural evolution of CoreLink, NOT a separate product.** The economic
  engine: the cache makes builds **faster (customer loves it) AND cheaper to run (our
  margin)** — a true **win-win** — and a multi-tenant content-addressed cache has a
  **network-effect moat** (more customers → fuller cache → faster+cheaper for everyone;
  share public deterministic deps, isolate private). Solo $30/mo, COGS ~$5 (~80% margin).
  Tailwind: GitHub starts charging for self-hosted runners Mar 2026.
  **Full brief: `marketing/expansion/ci-build-acceleration.md`.**
