# CLAUDE.md

Context for AI agents working in this repo. Keep it lean + high-signal.

## What CoreLink is

A **multi-tenant content-addressable cache + storage-governance platform** on
Cloudflare (Workers + Durable Objects + Containers + R2 + D1). ~73 Rust crates.
Sold **self-serve to SMBs** — NOT enterprise, and NOT "just a build cache."
It already exposes multiple cache surfaces: native CAS/AC, **Bazel REAPI v2**
(`routes/bazel_v2.rs`), **Turborepo** (`routes/turbo_v8.rs`), and **sccache** (WebDAV).

## Architecture wiki (OKF)

`docs/knowledge/` is the **code-grounded architecture wiki** — 157 OKF concepts,
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

- `python3 scripts/validate_specs.py` → **463/0**.
- Secrets matrix: `bash scripts/secrets-checklist-verify.sh` (OK, no drift) +
  `python3 scripts/validate_secrets_matrix.py` (code_only=0). Both exclude build output
  (`.open-next`/`.wrangler`) — don't let them scan generated bundles.
- `feat:`/`fix:` commits **require a CHANGELOG.md `[Unreleased]` entry** (changelog gate).
- Commits need a **`Signed-off-by:`** trailer (DCO).
- Branch protection `required checks = []`, but **merge only when CI is green** (impeccable).
- `corelink-container` (pkg `corelink-server`) is on the **proptest-density allowlist**.

## ⛔ Before merging ANY PR — do not skip

**Run `bash scripts/pre-merge-gate-check.sh <PR>` and merge ONLY if it prints
all-green.** The heavy gates (coverage / CodeQL / the TLA+ model-checks /
ffi-matrix / reproducible-build / cas-foundation / s10-ship-gate) were moved OFF
per-PR (2026-06-02) and now run **nightly (staggered) + on `main` + on-demand** —
so the checks that REMAIN on a PR are the fast, load-bearing ones and they MUST be
green. Never blind `--admin` merge; if you must `--admin`, state the documented
infra/flake reason explicitly. (A green PR now takes minutes, not 30+.)

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
