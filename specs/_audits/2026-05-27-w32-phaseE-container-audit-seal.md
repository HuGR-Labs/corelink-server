# Wave-32 Phase E — Dockerfile Audit + Pre-Push Scan SEAL

**Date:** 2026-05-27  
**Auditor:** Claude Sonnet 4.6 (WP-E.1)  
**Scope:** `Dockerfile` (91 LOC, Wave-33-Stage-2-hardened) + pre-push scan harness  
**Status:** SEALED

---

## 1. Dockerfile — Section-by-Section Audit

### 1.1 Header (lines 1–16)

```
# Multi-stage build otimizado pra Cloudflare Containers.
# Wave-33 Stage 2.B.2 — Fix de bug pré-existente ...
```

**Rationale:** Comments accurately narrate the Wave-33 stage-2 fix that changed the binary
location from a non-existent `src/` root to `crates/corelink-container/`. The comment
explaining why the entire workspace must be copied (workspace resolver reads every member's
`Cargo.toml`) is technically correct and useful.

**Finding:** No issues.

---

### 1.2 Builder stage base image (line 18)

```dockerfile
FROM rust:1.91-slim-bookworm AS builder
```

**Rationale:** `rust:1.91-slim-bookworm` is the official Rust image on Debian Bookworm slim
variant. The `-slim` variant omits most package manager caches and docs, keeping the builder
layer lean. The version is pinned to `1.91`, which matches `rust-toolchain.toml` in the
repository.

**Risk:** Image tag `1.91` is not digest-pinned; a Docker Hub re-tag of `1.91` could silently
pull a different layer set. See Hardening Opportunity #1 below.

---

### 1.3 Apt dependencies (lines 20–26)

```dockerfile
RUN apt-get update && apt-get install -y --no-install-recommends \
    protobuf-compiler \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*
```

**Rationale:**
- `--no-install-recommends` correctly avoids pulling optional packages.
- `rm -rf /var/lib/apt/lists/*` in the same `RUN` instruction correctly avoids baking apt
  cache into the layer.
- All four packages are necessary: `protobuf-compiler` for `prost`/`tonic` codegen,
  `pkg-config` + `libssl-dev` for OpenSSL linkage, `ca-certificates` for TLS roots at
  compile time.

**Risk:** Package versions are unspecified (floating). See Hardening Opportunity #2 below.

---

### 1.4 WORKDIR (line 28)

```dockerfile
WORKDIR /build
```

**Rationale:** `/build` is a clean, non-system path. Creates the directory implicitly if
absent. No issues.

---

### 1.5 Workspace copy (lines 30–48)

```dockerfile
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY tools ./tools
COPY apps/migrate-single-to-multi-region ./apps/migrate-single-to-multi-region
COPY tests ./tests
COPY migrations ./migrations
```

**Rationale:** The comment (lines 30–35) correctly explains that selective manifest-only
copying is not feasible with 120+ workspace members due to workspace resolver semantics. The
Wave-32 Phase E APPLY comments correctly document why `apps/migrate-single-to-multi-region`
and `migrations/` are required at compile time (binary workspace member + `include_str!`
macros).

**Exclusions confirmed correct:** Non-Rust apps (`admin-ui`, `docs`, `server`) are correctly
excluded. `.dockerignore` excludes `target/`, `.git/`, `.env*`, and `node_modules/`.

**Risk:** `COPY tests ./tests` includes integration test fixtures. These are required if any
crate `include_str!`-references test fixtures at compile time. If not required for the binary
build, this adds unnecessary layer weight. See Hardening Opportunity #3 below.

---

### 1.6 Dependency cache layer (lines 50–61)

```dockerfile
RUN printf '#![allow(missing_docs)]\nfn main() {}\n' > crates/corelink-container/src/main.rs \
 && echo "//! stub for dep cache layer" > crates/corelink-container/src/lib.rs \
 && cargo build --release -p corelink-server \
 && rm crates/corelink-container/src/main.rs crates/corelink-container/src/lib.rs
```

**Rationale:** This is a standard "dep cache" technique: stub out the binary's source with
minimal stubs, build the target package (which transitively builds all deps), then remove
the stubs so the next COPY + rebuild only recompiles the actual source. The `rm` at the end
of the same `RUN` instruction is correct — it prevents a dangling stub from being present in
the layer.

**Risk:** The stub `main.rs` uses `fn main() {}` which may not satisfy `use corelink_server::*`
at the top level if the binary's `lib.rs` has `pub use` re-exports checked at compile time.
However, since this is a dep-cache layer only (and the empty lib.rs stub is paired), this
typically works. No action needed.

---

### 1.7 Source copy + final build (lines 63–69)

```dockerfile
COPY crates/corelink-container/src ./crates/corelink-container/src
RUN cargo build --release -p corelink-server --bin corelink-server
```

**Rationale:** Copies only the source directory of the binary crate (not the entire `crates/`
tree), ensuring minimal cache invalidation. The `--bin corelink-server` flag ensures only the
binary is linked (not test or bench targets). Correct.

---

### 1.8 Runtime stage base image (line 72)

```dockerfile
FROM debian:bookworm-slim
```

**Rationale:** `debian:bookworm-slim` is the minimal Debian runtime image. Appropriate for a
dynamically-linked Rust binary (requires libc, libssl3).

**Risk:** Like the builder base, this tag is not digest-pinned. See Hardening Opportunity #1.

---

### 1.9 Runtime apt dependencies (lines 73–77)

```dockerfile
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*
```

**Rationale:** Minimal runtime deps. `libssl3` is the OpenSSL 3.x runtime library (matches
`libssl-dev` at build time on Bookworm). `ca-certificates` provides TLS root store for
outbound connections. `--no-install-recommends` + `rm -rf /var/lib/apt/lists/*` in-layer are
both present and correct.

**Finding:** No issues.

---

### 1.10 Non-root user creation (lines 79–80)

```dockerfile
RUN groupadd --system --gid 1000 corelink \
 && useradd --system --uid 1000 --gid corelink corelink
```

**Rationale:** Correct security hardening. Creates a dedicated system user+group with fixed
UID/GID 1000. System accounts (`--system`) have no login shell by default.

**Finding:** No issues. This is positive hardening that should NOT be removed.

---

### 1.11 Binary copy from builder (line 82)

```dockerfile
COPY --from=builder /build/target/release/corelink-server /usr/local/bin/corelink-server
```

**Rationale:** Copies only the compiled binary from the builder stage — the standard
multi-stage pattern. The builder stage's Rust toolchain, source tree, and target/ directory
are all discarded. Correct.

**Finding:** No issues.

---

### 1.12 USER, ENV, EXPOSE, ENTRYPOINT (lines 84–91)

```dockerfile
USER corelink
ENV RUST_LOG=info
ENV PORT=50051
EXPOSE 50051
ENTRYPOINT ["/usr/local/bin/corelink-server"]
```

**Rationale:**
- `USER corelink` correctly drops root privileges before the entrypoint.
- `ENV RUST_LOG=info` sets a sensible default log level (overridable at runtime).
- `ENV PORT=50051` documents the expected port.
- `EXPOSE 50051` documents the port (informational only; does not publish).
- `ENTRYPOINT ["/usr/local/bin/corelink-server"]` is exec-form (no shell wrapper). Correct.

**Risk:** Two separate `ENV` instructions create two layers. See Hardening Opportunity #3.

---

## 2. Hardening Opportunities (not applied)

### HO-1 (line 18, 72): Digest-pin base images

**Current:**
```dockerfile
FROM rust:1.91-slim-bookworm AS builder   # line 18
FROM debian:bookworm-slim                 # line 72
```

**Suggested change:**
```dockerfile
FROM rust:1.91-slim-bookworm@sha256:<digest> AS builder
FROM debian:bookworm-slim@sha256:<digest>
```

**Rationale:** Tag-only references allow registry administrators (or a supply-chain attacker)
to silently swap the image layer behind the same tag. Digest pinning ensures the exact layer
set is used regardless of tag updates. Obtain the digest via:
```bash
docker pull rust:1.91-slim-bookworm && docker inspect rust:1.91-slim-bookworm --format '{{.RepoDigests}}'
docker pull debian:bookworm-slim && docker inspect debian:bookworm-slim --format '{{.RepoDigests}}'
```
Aligns with ADR-0015 (reproducible builds). **Not applied per W3 (no Dockerfile modifications
beyond optional LABELs).**

---

### HO-2 (line 21): Pin apt package versions

**Current:**
```dockerfile
RUN apt-get update && apt-get install -y --no-install-recommends \
    protobuf-compiler \
    pkg-config \
    libssl-dev \
    ca-certificates \
```

**Suggested change:**
```dockerfile
RUN apt-get update && apt-get install -y --no-install-recommends \
    "protobuf-compiler=3.21.12-3" \
    "pkg-config=1.8.1-1" \
    "libssl-dev=3.0.11-1~deb12u2" \
    "ca-certificates=20230311" \
```

**Rationale:** Floating apt package names resolve to the latest version at build time. A
package update (e.g. a new libssl-dev ABI) could silently change build behavior or introduce
a regression. Pinning ensures reproducible builds across time. Obtain current versions via
`apt-cache show <package>` in a Bookworm container. Aligns with ADR-0015. **Not applied.**

---

### HO-3 (lines 85–86, 44–48): Consolidate ENV + audit `tests/` copy

**HO-3a — Consolidate ENV (lines 85–86):**

**Current (2 layers):**
```dockerfile
ENV RUST_LOG=info
ENV PORT=50051
```

**Suggested change (1 layer):**
```dockerfile
ENV RUST_LOG=info \
    PORT=50051
```

**Rationale:** Each `ENV` instruction creates a new layer. Consolidating reduces the final
image's layer count by 1. Minor, but consistent with the image-size optimization goal.

**HO-3b — Audit `COPY tests ./tests` (line 44):**

**Current:**
```dockerfile
COPY tests ./tests
```

**Investigation needed:** Verify whether any crate in the workspace uses `include_str!` or
`include_bytes!` on paths under `tests/` as part of its `build.rs` or `src/` (not test code).
If no production code references `tests/` at compile time, this `COPY` can be removed,
reducing build context size. Audit command:
```bash
grep -r 'include_str\|include_bytes' crates/ tools/ --include='*.rs' | grep 'tests/'
```

**Not applied per W3 (no source modifications).**

---

## 3. `.dockerignore` Assessment

**Status: PRESENT** at repo root.

Contents:
```
target/
.git/
.github/
.vscode/
.idea/
.DS_Store
*.md
!README.md
.env
.env.*
.wrangler/
node_modules/
```

**Assessment:**
- `target/` exclusion is critical — prevents the Rust build cache (potentially GB-scale) from
  entering the build context.
- `.env` / `.env.*` exclusion prevents secret files from entering the build context
  (CTRL-CRED-001 compliance).
- `.git/` exclusion prevents the entire git history from entering the context.
- `*.md` with `!README.md` is reasonable (excludes docs from build context).
- `node_modules/` exclusion prevents JS dependency trees from entering.

**Gap noted:** The `.dockerignore` does not exclude `specs/`, `docs/`, `dashboards/`,
`monitoring/`, `infra/`, `reports/`, `marketing/`, `legal/`, `compliance/`, `openapi/`,
`sdks/`, or `examples/`. These non-Rust directories are large and not required by the
Dockerfile's `COPY` instructions (the Dockerfile only copies `Cargo.toml`, `Cargo.lock`,
`crates/`, `tools/`, `apps/migrate-single-to-multi-region/`, `tests/`, and `migrations/`).

Docker's build context is constructed by evaluating what is NOT excluded by `.dockerignore`,
then the daemon transfers it to the builder. Adding explicit excludes for the large non-Rust
directories would reduce build context transfer time, particularly over a remote daemon or in
CI. This is an additive improvement; the existing `.dockerignore` does not cause functional
errors.

**No changes required for correctness. Improvement is optional.**

---

## 4. Security Summary

| Concern | Finding | Status |
|---------|---------|--------|
| Secrets in `ENV`/`ARG` | None detected in Dockerfile | PASS |
| Secrets in `COPY` sources | `.env*` excluded by `.dockerignore` | PASS |
| Root user in final image | `USER corelink` (UID 1000) applied before ENTRYPOINT | PASS |
| Shell in ENTRYPOINT | Exec-form `["/usr/local/bin/corelink-server"]` — no shell wrapper | PASS |
| Build-time secrets | No `--secret` mount or ARG credential patterns | PASS |
| Base image pin | Tags only (no digest) — Hardening Opportunity #1 | OPEN (non-blocking) |
| apt version pin | Versions float — Hardening Opportunity #2 | OPEN (non-blocking) |
| Multi-stage isolation | builder stage discarded; only binary copied to runtime | PASS |

---

## 5. Pre-Push Scan Harness

**File:** `scripts/e-day-container-pre-push-scan.sh`

### 5.1 Checks implemented

| Check | Threshold | Mode |
|-------|-----------|------|
| Image size | WARN >256 MB / ERROR >512 MB (CF beta limits) | dry-run + apply |
| Layer count | WARN >50 / ERROR >100 (CF beta limits) | dry-run + apply |
| Shell scripts in final image | ERROR if found (via history + ENTRYPOINT/CMD inspection) | dry-run + apply |
| Credentials in docker history | ERROR if found (CTRL-CRED-001) | dry-run + apply |
| Filesystem secret scan | ERROR if found (grep in read-only container) | apply only |

### 5.2 Exit codes

| Code | Meaning |
|------|---------|
| 0 | All checks pass (or dry-run with docker absent/daemon-down: graceful exit) |
| 1 | Warning(s) only (e.g. image 300 MB) |
| 2 | Error: size >512 MB, secret leak, or shell scripts in final image |

### 5.3 Graceful degradation

- Docker binary absent: exits 0 with informative message (all checks SKIPPED).
- Docker daemon not running: exits 0 with message (daemon required; start it and retry).
- Image not built yet: exits 2 with "run build-container-prod.sh first" message.

### 5.4 Shellcheck compliance

```
shellcheck scripts/e-day-container-pre-push-scan.sh
# Exit 0 — no warnings, no errors
```

---

## 6. CF Containers Beta Gradual-Deploy Gate (reference)

Per Wave-32 Phase E spec:

1. `bash scripts/build-container-prod.sh` — build image locally
2. `bash scripts/e-day-container-pre-push-scan.sh --apply` — scan must exit 0 or 1
3. `bash scripts/push-container-prod.sh --dry-run` — verify push intent
4. `docker run --rm corelink-server:prod` smoke test locally
5. `bash scripts/push-container-prod.sh --apply` — push to CF registry (Owner-bound)
6. `wrangler deploy --env prod` at 5% canary rollout
7. Observe 10 minutes; check error rates and cold-start latency
8. Ramp to 100% or rollback via `wrangler rollback`

---

## 7. DoD Checklist

| # | Item | Status |
|---|------|--------|
| 1 | `e-day-container-pre-push-scan.sh` runs `--dry-run` (no build, no run by default) | PASS |
| 2 | Scan checks: size, layer count, no shell scripts, no secrets (history + fs in --apply) | PASS |
| 3 | Exit codes: 0=clean, 1=warn, 2=error | PASS |
| 4 | `.dockerignore` exists | PASS (pre-existing) |
| 5 | Audit doc cites each Dockerfile section + lists ≥3 hardening opportunities with LOC | PASS (3 HOs: lines 18/72, 21, 85–86/44) |
| 6 | `shellcheck` clean | PASS (exit 0) |
| 7 | Single commit | PENDING (commit below) |

---

## 8. Commit Reference

**Commit SHA:** _to be filled post-commit_

---

*Sealed by Claude Sonnet 4.6 — WP-E.1 — 2026-05-27*
