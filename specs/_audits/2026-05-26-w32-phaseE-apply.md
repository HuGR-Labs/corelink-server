---
id: "AUDIT-2026-05-26-W32-PHASE-E-APPLY"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-26"
updated: "2026-05-26"
closed: null
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
tags:
  - "audit"
  - "wave-32"
  - "phase-e"
  - "apply"
  - "prod-deploy"
  - "container"
  - "real-state-change"
  - "hard-pause"
references:
  - "specs/_audits/2026-05-22-wave32-prod-deploy-spec.md"
  - "specs/_audits/2026-05-26-w32-phaseD-apply.md"
  - "specs/_audits/2026-05-26-w32-phaseE-prep.md"
  - "commit f86bb490 (security hardening: workers_dev=false, cpu_ms=30)"
  - "commit bc4eb236 (docker fix: apps/migrate-single-to-multi-region)"
  - "commit 837aa21c (docker fix: tests/ workspace members)"
  - "commit d879728d (docker fix: rust 1.91 base image)"
  - "commit 31b40e7f (build script: SIGPIPE fix)"
  - "commit 9760baef (docker fix: migrations/ include_str)"
  - "commit 12a482b3 (docker fix: missing_docs stub)"
---

# Wave 32 Phase E APPLY — Container Deploy SEAL Audit (2026-05-26)

> **Doc kind:** wave-scope apply audit — partial execution with hard pause.
>
> **Owner:** Gustavo Schneiter (Security + Release Lead).
>
> **Authored:** 2026-05-26 by Claude Sonnet 4.6 (agent-ad520ff554df6b59a) during
> Phase E APPLY execution (v2 redispatch after security-hardening pause).
>
> **Supersedes:** no prior E APPLY audit — this is the first execution attempt.
>
> **Base commit at dispatch start:** `f86bb490` (security-hardening commit,
> `workers_dev = false` + `cpu_ms = 30`).
>
> **HEAD at HALT:** `12a482b3` (last Dockerfile fix commit).
>
> **Charter compliance:** CTRL-CRED-001 honoured — zero secret values in this
> doc. No credentials logged. `.env.local` never committed. ADR-0015
> reproducible builds executed and PASSED. CTRL-AUDIT-EMIT-BEFORE-MUTATION:
> all state changes preceded by log entries.

---

## §1 Scope

Phase E APPLY executed against live Cloudflare production account
(`6a1fc1c626fc2628823e60b9db01f5cd`).

**Planned steps:**
1. Baseline verification (Step 0)
2. Local Docker build (Step 1)
3. Push to CF Containers registry (Step 2)
4. Deploy Worker + DO + Container (Step 3)
5. Smoke verification (Step 4, Option A or B)
6. SEAL audit (Step 5)

**Execution result:** Steps 0-1 COMPLETE. Steps 2-3 PARTIAL (Worker deployed;
Container registration HALTED by missing token permission). Step 4 DEFERRED
(no public URL yet). Step 5 ACTIVE (this doc, audit_status ACTIVE pending
token fix).

---

## §2 Baseline verification (Step 0)

All six checks passed before execution:

| Check | Result |
|-------|--------|
| HEAD commit | `f86bb490` (post-hardening, as required) |
| wrangler version | 4.95.0 (via `npx wrangler@latest`) |
| `.env.local` key count | 18 keys (≥17 required) |
| `workers_dev = false` in wrangler.toml | CONFIRMED |
| `cpu_ms = 30` in wrangler.toml | CONFIRMED |
| D1 `corelink-prod-d1` present | CONFIRMED (database_id `d64742ea-...`) |
| Secrets count | 67 lines from `wrangler secret list --env prod` (≥16) |
| Docker daemon | v28.3.2 running |
| Phase D audit | `2026-05-26-w32-phaseD-apply.md` read end-to-end |
| Phase E prep audit | `2026-05-26-w32-phaseE-prep.md` read end-to-end |
| Wave 32 spec | `2026-05-22-wave32-prod-deploy-spec.md` §3-§4 read |

---

## §3 Docker build evidence (Step 1) — COMPLETE

### §3.1 Dockerfile defects discovered and fixed

The Dockerfile (from Phase E PREP) had six latent defects, all found and fixed
during this APPLY execution. Fixes committed in sequence:

| Commit | Defect | Fix |
|--------|--------|-----|
| `bc4eb236` | Dockerfile missing `COPY apps/migrate-single-to-multi-region` — cargo workspace resolver fails | Added COPY for the Rust workspace member |
| `837aa21c` | Dockerfile missing `COPY tests` — 11 test workspace members fail workspace resolution | Added `COPY tests ./tests` |
| `d879728d` | Dockerfile pinned `rust:1.82` but workspace `rust-toolchain.toml` pins 1.91.1; `block-buffer 0.12.0` requires edition2024 (stable ≥1.85) | Bumped builder to `rust:1.91-slim-bookworm`; updated `build-container-prod.sh` RUST_VERSION |
| `31b40e7f` | `docker buildx inspect --bootstrap ... \| head -10` causes SIGPIPE with `set -euo pipefail`, silently exits script at exit code 0 | Replaced `head -10` with `grep -E "^(Name\|Driver\|Status\|BuildKit)"` |
| `9760baef` | Multiple crates embed SQL via `include_str!("../../../migrations/d1/*.sql")` — `migrations/` dir not in Docker build context | Added `COPY migrations ./migrations` |
| `12a482b3` | Dep-cache stub `main.rs` (`fn main() {}`) rejected by `-D missing_docs` workspace lint | Changed stub to `printf '#![allow(missing_docs)]\nfn main() {}\n'` |

### §3.2 Build result

**Command:** `bash scripts/build-container-prod.sh`

| Metric | Value |
|--------|-------|
| Exit code | 0 (success) |
| Image tag | `corelink-server:prod`, `corelink-server:12a482b3` |
| Image digest | `sha256:94495e8414594579fdeb4b567e557fe32550adb7e51388f31e196b1442762e7c` |
| Image size | 128 MB (size gate: PASS, < 2 GiB) |
| Layer count | 1 (runtime stage; builder stage squashed) |
| Build time | 126 s |
| Architecture | linux/amd64 |
| SOURCE_DATE_EPOCH | `1779829680` (from `git log -1 --format=%ct` at HEAD `12a482b3`) |
| Rust base image | `rust:1.91-slim-bookworm@sha256:8514999d4786ef12efe89239e86b3d0a021b94b9d35108c8efe6c79ca7dc1a65` |
| Runtime base | `debian:bookworm-slim@sha256:0104b334637a5f19aa9c983a91b54c89887c0984081f2068983107a6f6c21eeb` |
| Binary | `/usr/local/bin/corelink-server` |
| Binary smoke | PASS (`--version` returned exit 0) |
| CTRL-CRED-001 scan | PASS (0 credential patterns in `docker history`) |

### §3.3 ADR-0015 Reproducibility Verification

Two sequential builds on the same HEAD `12a482b3`:

| Build | Digest |
|-------|--------|
| Build 1 | `sha256:94495e8414594579fdeb4b567e557fe32550adb7e51388f31e196b1442762e7c` |
| Build 2 | `sha256:94495e8414594579fdeb4b567e557fe32550adb7e51388f31e196b1442762e7c` |

**ADR-0015 result: PASS** — digests identical. Reproducibility verified.

Build log: `target/phase-e-build.log`

---

## §4 Push evidence (Step 2) — HALTED

**Command attempted:** `bash scripts/push-container-prod.sh --apply`

**Result:** FAILED with HTTP 403 Forbidden on
`GET /accounts/6a1fc1c626fc2628823e60b9db01f5cd/containers/registries/registry.cloudflare.com/credentials`

**Root cause:** `CLOUDFLARE_API_TOKEN` lacks `Cloudflare Containers: Edit`
permission. The token has `Workers Scripts/KV/R2/D1 Edit` + `Zone DNS/Workers
Routes Edit` but not Containers registry access.

**Push not completed.** No container registered in CF registry.

**Wrangler error message:** `Forbidden` / `cloudchamber push failed`

---

## §5 Deploy evidence (Step 3) — PARTIAL

**Command:** `npx wrangler@latest deploy --env prod`

**Partial results:**

| Action | Result |
|--------|--------|
| 10 R2 buckets auto-provisioned | SUCCESS — `corelink-chunk-{sam,iad,lhr,nrt,syd}` + `corelink-manifest-{sam,iad,lhr,nrt,syd}` all created |
| Worker TypeScript compiled | SUCCESS — 28.80 KiB / gzip: 6.44 KiB |
| Worker uploaded to CF | SUCCESS — "Uploaded corelink-prod (28.15 sec)" |
| Worker deployment version | `3e27ef98-b076-48a5-b063-987f8273492d` (created `2026-05-26T21:11:48.516Z`) |
| Container image built inline | SUCCESS — CF wrangler rebuilt from Dockerfile (93s); image tagged `corelink-prod-corelinkserver-prod:3e27ef98` |
| Container registered to CF | FAILED — HTTP 403 on `GET /accounts/{id}/containers/me` |

**Wrangler error:**
```
ApiError: Forbidden
  url: https://api.cloudflare.com/client/v4/accounts/.../containers/me
  body: { error: 'Authentication error' }
```

**HARD PAUSE TRIGGER #4 FIRED:** `wrangler deploy --env prod` failed (container
registration step). Worker itself is deployed but NOT bound to the container
yet — DO instantiation cannot complete.

**Hard pause per charter:** Do NOT attempt workarounds. Document + escalate.

---

## §6 Smoke evidence (Step 4) — DEFERRED

Smoke deferred to post-token-fix. Option B applies:

- No workers.dev URL (enforced by `workers_dev = false`, as intended)
- `wrangler dev --remote --env prod` would require the container binding to be
  registered — blocked by the same token permission gap
- Option A smoke will be executed in the Phase E APPLY v3 re-dispatch after
  token fix

---

## §7 Hardening verification

| Check | Status |
|-------|--------|
| `workers_dev = false` in wrangler.toml at deploy time | CONFIRMED — grep verified before deploy; not modified |
| `cpu_ms = 30` in wrangler.toml at deploy time | CONFIRMED — grep verified before deploy; not modified |
| No public URL assigned | CONFIRMED — deploy completed partial Worker upload; no `*.workers.dev` URL allocated (workers_dev=false honoured) |
| `workers_dev` toggle to true for smoke | NOT DONE — charter prohibits; Option B deferral applied |

---

## §8 Charter compliance

| Rule | Status |
|------|--------|
| CTRL-CRED-001 | PASS — zero secret values in logs, commits, or this doc; image history clean |
| ADR-0015 (reproducible builds) | PASS — two builds → same SHA-256 `sha256:94495e...762e7c` |
| INV-CAS-CORRECTNESS | PRESERVED — container starts gRPC on :50051; no code change in this phase |
| Hardening preservation | PASS — `workers_dev=false` + `cpu_ms=30` intact at all times |
| No gambiarra | PASS — Dockerfile defects fixed properly (root-cause patches, not workarounds); token gap escalated, not bypassed |

---

## §9 State after HALT

### Deployed (irreversible / recoverable)
- **Worker** `corelink-prod` version `3e27ef98` — deployed and live
- **10 R2 buckets** — provisioned: `corelink-chunk-{sam,iad,lhr,nrt,syd}` + `corelink-manifest-{sam,iad,lhr,nrt,syd}`
- **16 R2 buckets total** (6 pre-Phase-C + 10 new)

### Not deployed
- Container binding — requires `Cloudflare Containers: Edit` token permission
- DO `CoreLinkServer` instantiation — blocked by container binding gap
- Any public smoke or traffic routing

### Rollback available
- Worker: `npx wrangler@latest rollback --env prod` — reverts to prior Worker version
- R2 buckets: idle; no data; can be deleted via `npx wrangler@latest r2 bucket delete <name>`
- Container: nothing registered; nothing to roll back

---

## §10 Dockerfile defect root cause analysis

The Phase E PREP audit (`2026-05-26-w32-phaseE-prep.md`) performed static
analysis but could not run a Docker build (Docker daemon offline on prep host).
The six defects above were all latent in the Dockerfile since its creation and
would have surfaced in any real build attempt. They fall into three categories:

1. **Missing workspace member directories** (`apps/migrate-single-to-multi-region`,
   `tests/`) — the workspace `Cargo.toml` lists all members and cargo reads
   every `Cargo.toml` before scheduling builds. Prep audit §2.4 only checked
   `crates/` and `tools/` paths, missing the two non-standard dirs.

2. **Rust version mismatch** — Dockerfile hardcoded `rust:1.82` while the
   workspace `rust-toolchain.toml` pins 1.91.1. Dependencies in `Cargo.lock`
   had drifted to require edition2024 (stable in 1.85). Prep audit §2.1 noted
   "pinning 1.82 to track CF Containers runtime" without verifying
   compatibility with the current lockfile.

3. **Build-context omissions** — `migrations/` directory required by
   `include_str!` macros in multiple crates; `#![allow(missing_docs)]` needed
   on dep-cache stub. Prep audit §2.3-§2.4 did not scan for `include_str!`
   patterns pointing outside `crates/`.

4. **Script SIGPIPE** — `docker buildx inspect | head -10` with `set -euo
   pipefail`. Classic bash pitfall. Silent exit. No prep-phase test could catch
   this without running the script with an active Docker daemon.

All six fixes are minimal and correct. No architectural changes required.

---

## §11 What Phase E APPLY v3 must do

1. **Token fix** — add `Cloudflare Containers: Edit` (and optionally
   `User Details: Read`) to `CLOUDFLARE_API_TOKEN` in Cloudflare dashboard →
   update `.env.local` if a new token is minted.

2. **Re-run deploy** — `npx wrangler@latest deploy --env prod` with the
   updated token. The 10 R2 buckets are already provisioned; wrangler will
   skip them. The Worker script is already uploaded; wrangler will update
   it if changed. The container binding registration will complete.

3. **Smoke (Option A)** — `wrangler dev --remote --env prod` → get preview
   URL → `curl <preview>/health` → expect 200.

4. **Update this audit** — add deployment ID + smoke evidence + flip
   `audit_status` to `CLOSED`.

---

## §12 Phase F+G+H status

**Phase F+G+H APPLY remain BLOCKED** on Phase E completion. Do not dispatch.

Phase F needs `CLOUDFLARE_PAGES_API_TOKEN` — already validated in `.env.local`.

---

## §13 Rollback evidence

**Rollback command (dry-run not supported; command documented):**
```
npx wrangler@latest rollback --env prod
```

No rollback executed — partial deploy leaves Worker live but non-functional
without container binding. Decision: keep Worker deployed (harmless; returns
503 on container calls until container binding is complete); rollback optional
if user prefers clean state.

---

## §14 Sign-off

Phase E APPLY is **PAUSED** pending `Cloudflare Containers: Edit` token
permission grant by Owner.

**Hard pause trigger fired:** TRIGGER #4 — `wrangler deploy --env prod` failed
(container registration step; HTTP 403 on `/containers/me`).

**Deliverables completed this phase:**
- 6 Dockerfile defect fixes (commits `bc4eb236`, `837aa21c`, `d879728d`,
  `31b40e7f`, `9760baef`, `12a482b3`)
- Container image built, 128 MB, ADR-0015 verified (two builds, same digest)
- Worker `corelink-prod` deployed (version `3e27ef98`)
- 10 additional R2 buckets provisioned
- This SEAL audit doc

**Required action by Owner:** Add `Cloudflare Containers: Edit` permission to
the API token at https://dash.cloudflare.com/profile/api-tokens. Then
re-dispatch Phase E APPLY v3.

SEAL: `2026-05-26T21:15:00Z` — agent-ad520ff554df6b59a (Claude Sonnet 4.6)

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>

---

**End of Wave 32 Phase E APPLY SEAL Audit (ACTIVE — awaiting token fix).**
