# WP-02 REVIEW — Iteration 1 (CRITICAL AUDIT)

**Reviewer:** Self (TechLead persona)  
**Date:** 2026-08-26  
**Verdict:** ❌ **FAIL — 12 BLOCKING ISSUES, 9 HIGH SEVERITY ISSUES, 8 MEDIUM SEVERITY ISSUES**

---

## 🔴 BLOCKING ISSUES (Must fix before sign-off)

### B1. **Missing Crate Sources — Build Will Fail**
- **Location:** Dockerfile `clw-builder` stage, lines 80-87
- **Code (current):**
  ```dockerfile
  COPY Cargo.toml Cargo.lock ./
  COPY crates/clw-cli ./crates/clw-cli
  COPY crates/clw-types ./crates/clw-types
  COPY crates/clw-cache ./crates/clw-cache
  COPY crates/clw-chunk ./crates/clw-chunk
  COPY crates/clw-manifest ./crates/clw-manifest
  COPY crates/clw-client ./crates/clw-client
  COPY crates/clw-manifest ./crates/clw-manifest   ← duplicate
  ```
- **Problem:** `corelink-workspaces/crates/clw-cli/Cargo.toml` depends on `clw-snapshot`, `clw-hydrate`, `clw-run` (not listed in COPY). Cargo resolver requires every member of the dependency closure to be present before any build. Missing crates = `error: package X not found`.
- **Evidence:** `/Users/gustavoschneiter/Documents/HuGR/corelink-workspaces/crates/clw-cli/Cargo.toml:18-26` lists: clw-types, clw-snapshot, clw-hydrate, clw-run, clw-manifest, clw-cache, clw-client. Workspace `Cargo.toml:2-14` has 12 members.
- **Fix:** Add all first-party crate COPY lines: `clw-snapshot`, `clw-hydrate`, `clw-run`, plus the clw-conformance/clw-integration-tests/clw-e2e-evidence members (or exclude them via `exclude` in workspace `[members]` for the build context).

### B2. **Duplicate COPY Line: `clw-manifest` Listed Twice**
- **Location:** Dockerfile lines 85 and 87
- **Problem:** `COPY crates/clw-manifest ./crates/clw-manifest` appears twice. Docker treats the second as a no-op (same source/dest) but the comment "/not just manifests" is contradicted by the missing crates. Indicates copy-paste error.
- **Fix:** Remove the duplicate AND add the missing crates (see B1).

### B3. **`ttyd` and `code-server` Not in Debian Bookworm apt**
- **Location:** Dockerfile `runtime` stage, lines 127-128
- **Code:** `ttyd \` and `code-server \` in `apt-get install`
- **Problem:** Verified against `https://packages.debian.org/bookworm/{ttyd,code-server}` — both return "No such package" / "Debian -- Error". `apt-get install` will fail with `E: Unable to locate package ttyd` and `E: Unable to locate package code-server`. The runtime build will fail before producing any image.
- **Real source:** `ttyd` is at `https://github.com/tsl0922/ttyd/releases` (binary tarball). `code-server` is at `https://github.com/coder/code-server/releases` (binary tarball). `novnc` IS in Debian as the `novnc` package (good), `websockify` IS in Debian (good).
- **Fix:** Replace `ttyd` and `code-server` with multi-stage binary downloads:
  ```dockerfile
  # In a new pre-stage: download + verify SHA256 + extract
  FROM debian:bookworm-slim@sha256:... AS bin-downloader
  ARG TTYD_VERSION=1.7.7
  ARG CODE_SERVER_VERSION=4.96.4
  RUN curl -fsSL "https://github.com/tsl0922/ttyd/releases/download/${TTYD_VERSION}/ttyd.x86_64" -o /usr/local/bin/ttyd \
   && curl -fsSL "https://github.com/coder/code-server/releases/download/v${CODE_SERVER_VERSION}/code-server-${CODE_SERVER_VERSION}-linux-amd64.tar.gz" -o /tmp/cs.tgz \
   && tar -xzf /tmp/cs.tgz -C /tmp/ \
   && cp /tmp/code-server-${CODE_SERVER_VERSION}-linux-amd64/bin/code-server /usr/local/bin/code-server
  ```
  Or, simpler: document and use the `deb` packages from upstream release URLs. Each must be SHA256-pinned per HO-1.

### B4. **`rust:1.91-slim-bookworm` Tag Mismatch with Pinned `rust-toolchain.toml`**
- **Location:** Dockerfile line 66
- **Code:** `FROM rust:1.91-slim-bookworm@sha256:ac77791d...`
- **Problem:** `corelink-workspaces/rust-toolchain.toml` pins `channel = "1.91.1"` (exact patch). Using tag `1.91` resolves to the latest 1.91.x (currently 1.91.1, but could float). The digest pin is the authoritative reference, but the tag SHOULD match `rust-toolchain.toml` to make it self-documenting (and to align with the corelink-server `Dockerfile` precedent which also uses `rust:1.91-slim-bookworm`).
- **Real risk:** If a future operator rebuilds the image AFTER `rust-toolchain.toml` bumps to `1.92.0` but forgets to update the Dockerfile tag, the digest pin still references an old base. The tag pin is a tripwire.
- **Fix:** Add a comment cross-linking the toolchain file. Or use `rust:1.91.1-slim-bookworm` IF that tag exists (it may not — Docker Library pins major.minor only). Safer: keep tag + digest, add comment that the tag MUST be reviewed when `rust-toolchain.toml` bumps.

### B5. **`clw` Binary Will Fail at Runtime: Missing `libssl3` in Runtime Stage**
- **Location:** Runtime stage apt install (lines 122-138)
- **Problem:** `clw` uses `reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "http2"] }` (rustls-tls, not native-tls) per `corelink-workspaces/Cargo.toml`. Rustls is statically linked to the binary — so `libssl3` is NOT strictly required for clw. BUT the build stage also installs `pkg-config` + `libssl-dev` which is fine for build, and the runtime doesn't need it. **However:** the `verify-image.sh` check at line 226 runs `which code-server supervisord` — `code-server` requires `libstdc++6` (Node.js native), `libnss3`, `libatk1.0-0`, `libgtk-3-0`, `libgbm1`, `libasound2`, `libdrm2`. These are NOT in the install list. Chromium has the same issue. `chromium` from Debian bookworm pulls in its own deps via apt's resolver, but `code-server` is a manual binary download (see B3), so its dependencies must be explicitly installed.
- **Real evidence:** `code-server` Debian/Ubuntu install instructions list ~20 system libraries.
- **Fix:** Either (a) install `code-server` from a `.deb` that pulls its own deps (coder.com provides one), or (b) explicitly install: `libnss3 libatk1.0-0 libatk-bridge2.0-0 libcups2 libdrm2 libxkbcommon0 libxcomposite1 libxdamage1 libxfixes3 libxrandr2 libgbm1 libpango-1.0-0 libcairo2 libasound2 libatspi2.0-0`.

### B6. **`novnc` Symlink to `vnc.html` is Wrong Default Page**
- **Location:** Dockerfile lines 156-157
- **Code:**
  ```dockerfile
  RUN mkdir -p /usr/share/novnc \
   && ln -sf /usr/share/novnc/vnc.html /usr/share/novnc/index.html
  ```
- **Problem:** The `novnc` Debian package installs files into `/usr/share/novnc/` (verified in WP-03's supervisord line 350: `websockify --web /usr/share/novnc`). However, `mkdir -p /usr/share/novnc` creates the dir, but if the package already populated it (it does — `novnc` package puts files in `/usr/share/novnc/`), the `mkdir -p` is a no-op. The `ln -sf` creates `index.html` pointing at `vnc.html`. **`vnc.html` is a deprecated/legacy endpoint** in noVNC; modern noVNC uses `vnc_lite.html` or the React UI at `index.html` directly. The legacy `vnc.html` redirects but is no longer the default.
- **Fix:** Use `ln -sf /usr/share/novnc/vnc_lite.html /usr/share/novnc/index.html` OR drop the symlink entirely and configure `websockify --web /usr/share/novnc` to serve its own default file (it serves `index.html` if present).

### B7. **No `HEALTHCHECK` Defined**
- **Location:** Dockerfile (missing)
- **Problem:** WP-02 has no `HEALTHCHECK` instruction. CF Containers SDK uses the container's HTTP health (probes `defaultPort` by default). Without an explicit `HEALTHCHECK`, the SDK relies on the listening port being up — but if chromium/ttyd are slow to start, the container will be marked running while the user-facing services are not yet ready. WP-03's `entrypoint.sh` should signal readiness (e.g., curl localhost:6080) but the Dockerfile should NOT rely on this.
- **Fix:** Add a basic `HEALTHCHECK`:
  ```dockerfile
  HEALTHCHECK --interval=10s --timeout=3s --start-period=60s --retries=5 \
    CMD curl -fsS http://localhost:6080/vnc_lite.html || exit 1
  ```
  (And install `curl` in runtime — already done at line 136.) This tells CF Containers' health probe which service to validate.

### B8. **`USER coder` Set AFTER `EXPOSE` and `ENTRYPOINT` — Order Wrong Semantically**
- **Location:** Dockerfile structure (lines 160, 174, 177)
- **Code:** `USER coder` is at line 160, but `EXPOSE` is at 174 and `ENTRYPOINT ["/entrypoint.sh"]` is at 177.
- **Problem:** Order of `USER` relative to `RUN/COPY/ENV/EXPOSE/ENTRYPOINT` is semantic. `EXPOSE` is documentation (no runtime effect), so its position is cosmetic. BUT: `USER coder` BEFORE the binary is verified (line 149: `RUN clw --version`) is correct, AND `USER coder` BEFORE `WORKDIR /data/workspace` (line 161) means the WORKDIR is created as `coder` if it doesn't exist (it does, so fine). **The actual issue:** `USER coder` before the `RUN` at line 149 means `clw --version` runs as `coder` — which is GOOD. But `RUN chmod 755 /usr/local/bin/clw` at line 146 runs as root (before USER). After USER is set, the binary's group write is removed (755 = rwxr-xr-x). `coder` can execute. ✓ OK actually.
- **Real issue:** `USER coder` is set BEFORE `WORKDIR /data/workspace`. Per Docker best practice, `USER` should be set as late as possible, and `WORKDIR` after `USER` so the directory is created with correct ownership. This is reversed here.
- **Fix:** Reorder: keep all `RUN`/`COPY` as root, set `USER coder` immediately before `ENTRYPOINT` (or right before `WORKDIR`).

### B9. **No `HEALTHCHECK` Port and CF Container Compatibility**
- **Location:** Dockerfile (missing HEALTHCHECK)
- **Problem:** CF Containers SDK documents that the container's `defaultPort` is probed for health. The DEFAULT probe is TCP-only. If the service is HTTP and returns 4xx/5xx on startup, TCP succeeds but app is not ready. Need HTTP-aware probe. **Per WP-01's `defaultPort = 6080`**, the probe is against port 6080. Per WP-03, port 6080 is `websockify --web /usr/share/novnc` which serves HTTP `index.html` (a static HTML page). The probe should hit a static URL. Use `http://localhost:6080/index.html` for the HEALTHCHECK.
- **Fix:** Add `HEALTHCHECK` (see B7) — combine with B7.

### B10. **`--mount=type=cache,id=corelink-target,sharing=locked` Inherits `corelink-server` Cache State**
- **Location:** Dockerfile line 94
- **Code:** `--mount=type=cache,target=/build/target,id=corelink-target,sharing=locked`
- **Problem:** The cache mount ID `corelink-target` is IDENTICAL to the one used in `corelink-server/Dockerfile` (line 120 in that file). Two DIFFERENT projects (`corelink-server` and `corelink-runners`) sharing the same cache ID. BuildKit cache IDs are namespace-scoped to the build context (different `docker build` invocations), so this is technically safe in separate `docker buildx` invocations — but in CI, if a single BuildKit instance builds BOTH images in the same session (multi-target or multi-context build), the mounts collide and the target dir will contain cross-project artifacts, leading to `cargo` confusion (e.g., it may try to link `corelink-server` objects into `clw`).
- **Real risk:** Low (different build invocations) but documented as a gotcha.
- **Fix:** Rename to `runner-devenv-target` or scope by project: `id=clw-target`. Update WP-02 only.

### B11. **`clw --version` Runs at Image BUILD Time — Requires Network or Cargo State**
- **Location:** Dockerfile line 149
- **Code:** `RUN clw --version`
- **Problem:** `clw --version` is `cargo build --release -p clw-cli --bin clw` output copied to `/usr/local/bin/clw`. Running it at build time as `coder` user (after USER is set, line 160). Wait — the RUN at line 149 is BEFORE USER. So runs as root. OK. But: `clw --version` requires no network, just executes the binary. ✓ Safe.
- **Real issue:** This `RUN` is AFTER `USER coder` in actual file order? Let me re-check. Line 149 (RUN clw --version) is BEFORE line 160 (USER coder). So runs as root. ✓ OK. **The actual issue:** The verification `RUN` doesn't fail the build if clw crashes. Use `&&` chain or explicit error.
- **Fix:** Wrap in `RUN ["/usr/local/bin/clw", "--version"]` (exec form) to surface signals properly, OR add `|| (echo "clw binary verification FAILED" && exit 1)`.

### B12. **`PASSWORD_STORE=basic` and `CLW_CACHE_DIR` Are Runtime ENV, Not Static Image ENV**
- **Location:** Dockerfile lines 164-168
- **Code:**
  ```dockerfile
  ENV HOME=/home/coder \
      CLW_CACHE_DIR=/home/coder/.clw/cache \
      PASSWORD_STORE=basic \
      ...
  ```
- **Problem:** `HOME` baked into image — this is a known Docker footgun. When `envVars` is passed at container spawn time (`{envVars, ...}` per `@cloudflare/containers` SDK), the runtime env MERGES with image ENV. So `HOME=/home/coder` is fine, BUT if the SDK `envVars` doesn't include `HOME`, the user's shell (`coder` user, `bash` entrypoint) will see the baked value. ✓ OK.
- **Real issue:** `PASSWORD_STORE=basic` is unrelated to clw or the supervisor. It's a `pass` (passwordstore) gpg env. Why is it here? No pass utility is installed. This is a leftover from a different project.
- **Fix:** Remove `PASSWORD_STORE=basic` — it has no purpose in this image.

---

## 🟠 HIGH SEVERITY ISSUES

### H1. **No Lockfile for Runtime Image — Re-builds May Pick Different Versions**
- **Location:** Runtime stage `apt-get install` (lines 122-138)
- **Problem:** No `apt-mark hold` or version pinning. `apt-get install -y --no-install-recommends foo` picks the LATEST in the pinned base image's apt cache. If the base image's `Packages` is refreshed (Debian updates Packages occasionally between base image rebuilds), the SAME digest of `debian:bookworm-slim` could install DIFFERENT versions across time. This violates "reproducible builds" stated in I2 and DoD #9.
- **Fix:** Add version pins: `apt-get install -y --no-install-recommends xvfb=2:21.1.7-3+deb12u9 ...` (query `apt-cache madison` for current versions), OR use `apt-get install` with `apt-mark hold` after, OR commit to periodic base image rebuilds + re-verification.

### H2. **`.dockerignore` Excludes `*.toml` But Allows `Cargo.toml` — Inconsistent**
- **Location:** `.dockerignore` lines 191-194
- **Code:**
  ```
  *.toml
  *.lock
  !Cargo.toml
  !Cargo.lock
  ```
- **Problem:** The pattern `*.toml` matches `Cargo.toml` too. The negation `!Cargo.toml` is correctly applied AFTER, but the order of rules in `.dockerignore` matters and the pattern `*.toml` will match `deny.toml`, `rust-toolchain.toml` if those were in the build context. For `corelink-workspaces`, those files are at the repo root and they ARE needed (`deny.toml` not, but `rust-toolchain.toml` IS used by cargo's toolchain proxy). 
- **Real issue:** If `deny.toml` is in the build context (it is, at workspace root) and gets copied inadvertently, it doesn't break the build. But `rust-toolchain.toml` is NEEDED for cargo to know which toolchain to use — if `.dockerignore` excludes it, the build falls back to whatever rustc is in the PATH (the base image's, which is `1.91`, may differ from the workspace pin of `1.91.1`).
- **Fix:** Add `!rust-toolchain.toml` to the negation list.

### H3. **`WORKDIR /build` Conflicts with Cache Mount `/build/target`**
- **Location:** Dockerfile lines 77, 94
- **Code:** `WORKDIR /build` then `--mount=type=cache,target=/build/target,id=corelink-target`
- **Problem:** `WORKDIR` creates `/build` if it doesn't exist. The cache mount at `/build/target` requires `/build` to exist BEFORE the mount is applied. Per BuildKit docs, mounts are created AFTER WORKDIR is set, so this is OK. But: the `WORKDIR /build` at line 77 means `cargo metadata` is run in `/build` (where `Cargo.toml` was COPYed to). ✓ Correct.
- **Real risk:** None, but `WORKDIR /build` is the same as `corelink-server/Dockerfile`. Sharing WORKDIR names is a documentation hazard (readers may confuse the two). Not a code bug.
- **Verdict:** Acceptable but worth a comment.

### H4. **`EXPOSE 6080 7681 8080` Lacks Protocol Specifier**
- **Location:** Dockerfile line 174
- **Code:** `EXPOSE 6080 7681 8080`
- **Problem:** Modern Docker best practice: `EXPOSE 6080/tcp 7681/tcp 8080/tcp`. TCP is default, so this is technically equivalent, but explicit is better. The `verify-image.sh` script at line 236 checks for `'^(6080|7681|8080)/tcp$'` — which is `tcp` (lowercase). If `EXPOSE` is written as `EXPOSE 6080/tcp`, the check still matches. If written as `EXPOSE 6080/TCP` (uppercase), the check FAILS. Add the suffix for safety.
- **Fix:** `EXPOSE 6080/tcp 7681/tcp 8080/tcp`.

### H5. **No `LABEL` Metadata for Image Provenance**
- **Location:** Dockerfile (missing)
- **Problem:** SOTA image hygiene includes `LABEL org.opencontainers.image.source=...`, `LABEL org.opencontainers.image.revision=$(git rev-parse HEAD)`, `LABEL org.opencontainers.image.created=...`. Without these, the image can't be traced back to its source commit. This violates supply-chain traceability (related to HO-1 from WP-01 review).
- **Fix:** Add LABEL block:
  ```dockerfile
  LABEL org.opencontainers.image.title="corelink-runner-devenv" \
        org.opencontainers.image.source="https://github.com/HuGR-Labs/corelink-runners" \
        org.opencontainers.image.licenses="LicenseRef-Proprietary" \
        org.opencontainers.image.vendor="HuGR Labs"
  ```

### H6. **No `STOPSIGNAL` Set — Default is `SIGTERM` But `entrypoint.sh` Traps `SIGTERM` and Runs Snapshot**
- **Location:** Dockerfile (missing)
- **Problem:** WP-03's entrypoint (line 17 of WP-03 visible excerpt) is bash. The container's main process is the entrypoint. If Docker sends SIGTERM (default), the entrypoint's `trap` fires, runs `snapshot_all`, then `exec supervisord` (or whatever). If Docker sends SIGKILL (no grace period), no trap fires. The Dockerfile should explicitly set `STOPSIGNAL SIGTERM` AND a `stop_grace_period` equivalent. CF Containers uses a default of 30s. Per WP-03, the snapshot can take >30s on large workspaces → data loss risk.
- **Fix:** `STOPSIGNAL SIGTERM` is the default — but add a comment documenting the dependency on CF Containers' grace period setting (which is configurable in `wrangler.jsonc`).

### H7. **`verify-image.sh` Lacks `--no-cache` for Reproducibility Check (DoD #9)**
- **Location:** `verify-image.sh` lines 218
- **Code:** `docker build -f "$DOCKERFILE" -t "$IMAGE_TAG" "$CONTEXT"`
- **Problem:** DoD #9 says "Two builds produce identical image digest" — the script should test this with TWO builds + diff. Currently only one build is done.
- **Fix:** Add a reproducibility check:
  ```bash
  IMG1=$(docker build -q --no-cache ...)
  IMG2=$(docker build -q --no-cache ...)
  [ "$IMG1" = "$IMG2" ] || { echo "REPRODUCIBILITY FAILED"; exit 1; }
  ```

### H8. **`verify-image.sh` Uses `docker` Not `docker buildx`**
- **Location:** `verify-image.sh` line 218
- **Problem:** The Dockerfile uses BuildKit-only features (`--mount=type=cache`, `syntax=docker/dockerfile:1.6`). Classic `docker build` does NOT support cache mounts. The build will fail with `ERROR: failed to solve: failed to create LLB definition: unknown instruction: --mount`.
- **Fix:** Use `docker buildx build` (or set `DOCKER_BUILDKIT=1` for classic `docker build` — deprecated).

### H9. **`ENTRYPOINT ["/entrypoint.sh"]` But Dockerfile Doesn't Copy `entrypoint.sh`**
- **Location:** Dockerfile line 177
- **Code:** `ENTRYPOINT ["/entrypoint.sh"]`
- **Problem:** `entrypoint.sh` is the artifact of WP-03. The Dockerfile (WP-02) does NOT `COPY entrypoint.sh` into the image. WP-03 lists it as scope: "Out of Scope: clw binary build → WP-02". This is a CROSS-WP GAP: the Dockerfile references a file that another WP creates. There's no contract for HOW WP-03's file gets into the image.
- **Fix:** Add `COPY entrypoint.sh /entrypoint.sh` and `COPY supervisord.conf /etc/supervisor/conf.d/supervisord.conf` to WP-02's Dockerfile, OR document a separate `COPY` in WP-03 that gets applied AFTER WP-02's image is built (multi-stage WITHIN WP-03?). The cleaner pattern: WP-02 builds the base image; WP-03 ADDS the entrypoint in a SECOND FROM in the same Dockerfile (extend WP-02's Dockerfile in WP-03). Document the dependency.

---

## 🟡 MEDIUM SEVERITY ISSUES

### M1. **No `.dockerignore` for `target/` in Workspace Subdirs (only top-level)**
- **Location:** `.dockerignore` lines 195-196
- **Code:** `target` and `**/target`
- **Problem:** The `**/target` should cover all subdirs, but BuildKit's `**` is globstar — confirm it works. Actually `**/target` in `.dockerignore` is supported. ✓ OK.
- **Verdict:** Probably fine; worth a sanity check.

### M2. **`ENTRYPOINT` Exec Form Good, but `WORKDIR /data/workspace` Owned by `coder:coder` Conflict**
- **Location:** Dockerfile lines 152-153, 161
- **Code:** `RUN mkdir -p /data/chrome /data/workspace && chown -R coder:coder /data` then later `USER coder` then `WORKDIR /data/workspace`.
- **Problem:** The `WORKDIR` instruction will set CWD to `/data/workspace` for subsequent RUN/CMD/ENTRYPOINT, but since USER is `coder`, it must be readable+writable. `chown -R coder:coder /data` makes `/data` (and `/data/workspace` by inheritance) `coder:coder`. ✓ Correct. **However:** if `/data` is intended to be a MOUNT POINT for an external volume (CF Container's ephemeral NVMe per WP-01), the `mkdir -p` at build time creates it INSIDE the image layer. When the CF Container mounts over `/data` with the per-lease snapshot mount, the in-image `/data` is HIDDEN. The `chown` is wasted on the ephemeral mount. Not a bug, just wasted bytes (small).
- **Fix:** Acceptable, OR add comment explaining the overlay semantics.

### M3. **`fonts-dejavu-core` Duplicates `fonts-liberation` Function**
- **Location:** Dockerfile line 132
- **Code:** `fonts-dejavu-core \`
- **Problem:** `fonts-liberation` (Liberation Sans/Serif/Mono) and `fonts-dejavu-core` (DejaVu Sans/Serif/Mono) are both font families that are NOT in the Debian base. The image could probably get by with just one. The full minimal set for Chromium rendering is `fonts-liberation` (Chrome uses Liberation as a default metric-compatible replacement for Arial/Times). `fonts-dejavu-core` adds bulk for the emoji Unicode range only — and `fonts-noto-color-emoji` is already installed for that.
- **Fix:** Drop `fonts-dejavu-core`, keep Liberation + Noto.

### M4. **No `ARG` for Build-time Variables (e.g., `DEBIAN_FRONTEND=noninteractive`)**
- **Location:** Dockerfile (missing)
- **Problem:** `apt-get install` in a non-interactive Docker build will warn or fail on `tzdata` or similar interactive prompts. WP-02 doesn't have `tzdata` in the list, so it won't trigger. But the standard practice is `ARG DEBIAN_FRONTEND=noninteractive` at the top.
- **Fix:** Add `ARG DEBIAN_FRONTEND=noninteractive` before any `RUN apt-get`.

### M5. **Builder Stage Image Layer Includes `apt-get install` Cleanup in Same Layer — Good**
- **Location:** Dockerfile lines 69-75
- **Verdict:** ✓ Correct. Same RUN for `apt-get update && install && rm -rf /var/lib/apt/lists/*`. No issue. **Documented as PASS for reference.**

### M6. **`PASSWORD_STORE=basic` ENV Has No Purpose (also called out in B12)**
- **Location:** Dockerfile line 166
- **Duplicate of B12 (counted separately for severity).**
- **Fix:** Remove.

### M7. **`/data/workspace` Initial CWD May Be Empty on First Boot**
- **Location:** Dockerfile line 161 + WP-03 entrypoint
- **Problem:** `WORKDIR /data/workspace` is set as the image's default CWD. WP-03's entrypoint doesn't `cd` explicitly — it relies on CWD. If the CF Container overlay-mounts `/data` with a fresh (empty) dir, CWD `/data/workspace` may not exist → supervisord fails. The supervisord.conf (per WP-03 line 364) uses `--cwd /data/workspace` for ttyd, so it explicitly handles this. But the entrypoint's `validate_env` checks `[[ -d "${dir}" ]]` for `${WORKSPACE_DIR}` which IS `/data/workspace`. If overlay mount creates it empty, the check passes. ✓ OK actually. But if the overlay mount is SLOW and `/data/workspace` doesn't exist at entrypoint startup, validate_env fails.
- **Fix:** Document the dependency on CF Container mount timing, OR add `mkdir -p /data/{chrome,workspace}` in the entrypoint's pre-validation step (defensive).

### M8. **`verify-image.sh` Not Executable in WP**
- **Location:** `verify-image.sh` (line 207)
- **Problem:** The script is provided inline but the WP doesn't mention `chmod +x` or set shebang permissions. The Completeness Checklist says "verify-image.sh created and executable" — this requires the file to be created with executable bit.
- **Fix:** Document the `chmod +x` step explicitly in the build instructions.

---

## 📋 DoD Gap Analysis

The DoD has 11 items. Checking each:

| # | DoD Item | Verdict | Evidence |
|---|----------|---------|----------|
| 1 | Dockerfile builds without errors | ❌ **FAIL** | B1, B2, B3: missing crates + `ttyd`/`code-server` not in apt = build will fail |
| 2 | Base images pinned by SHA256 digest | ✅ Pass | Lines 66, 109 both have `@sha256:` |
| 3 | `clw` binary compiles and runs in image | ❌ **FAIL** | B1: missing crates prevent compile; B5: missing libs may prevent runtime |
| 4 | All required binaries present | ❌ **FAIL** | B3: `ttyd` and `code-server` will not be installed via apt |
| 5 | Non-root user `coder` (uid=1000) | ✅ Pass | Lines 141-142 |
| 6 | Data directories exist with correct perms | ✅ Pass | Lines 152-153 |
| 7 | Ports 6080, 7681, 8080 exposed | ✅ Pass | Line 174 (H4 nit: missing /tcp suffix) |
| 8 | Image size < 2GB compressed | ⚠️ **Cannot verify** | B5: adding 20+ system libs for code-server increases size significantly. `code-server` tarball is ~300MB. Risk: could exceed 2GB. |
| 9 | Build is reproducible | ❌ **FAIL** | H7: verify script doesn't test reproducibility; H1: no apt version pins |
| 10 | No secrets in image | ✅ Pass | No `COPY` of secret files; only `clw` binary which is a public build |
| 11 | `verify-image.sh` passes | ❌ **FAIL** | H8: uses `docker` not `docker buildx`; H7: missing reproducibility test |

**DoD Score: 4/11 PASS, 1/11 PARTIAL, 6/11 FAIL**

---

## 📋 Invariants Verification

| Invariant | Enforced in Code? | Verdict |
|-----------|-------------------|---------|
| I1: Base image digests never change without explicit version bump | ✅ Pinned | **ENFORCED** |
| I2: `clw` built from pinned `Cargo.lock` | ✅ `--locked` flag | **ENFORCED** |
| I3: No root processes in final image | ✅ `USER coder` set | **ENFORCED** (but B8 orders suboptimally) |
| I4: `/data/chrome` and `/data/workspace` owned by `coder:coder` | ✅ Lines 152-153 | **ENFORCED** |
| I5: Only ports 6080, 7681, 8080 exposed | ✅ Single `EXPOSE` line | **ENFORCED** |
| I6: Image contains no `.git`, `node_modules`, `target`, `*.md` | ✅ Via `.dockerignore` | **ENFORCED** (with H2 caveat on `rust-toolchain.toml`) |
| I6 (dup): `clw` statically linked | ❌ **WRONG** | **NOT ENFORCED** — `clw` is dynamically linked to glibc + libgcc (built on Debian glibc). WP-02's invariant text says "statically linked (musl not required; debian glibc compatible)" — this is **incorrect**. `clw` is dynamically linked to glibc, not statically linked. The invariant label is wrong. |
| **I7 (NEW)**: Reproducible build | ❌ Not enforced | **NOT ENFORCED** — H1, H7 |

**Invariants Enforced: 5/8 (62.5%)** — INSUFFICIENT (I6 duplicate + new I7)

---

## 📋 Quality Standards Verification

| Standard | Met? | Evidence |
|----------|------|----------|
| Supply Chain: All `FROM` use SHA256 digest | ✅ Pass | Lines 66, 109 |
| Reproducibility: Identical build → identical digest | ❌ Fail | H1, H7 |
| Minimal Attack Surface: No pkg mgr in runtime, no build tools, no docs/man | ⚠️ Partial | `apt-get install` leaves apt's package index? No, `rm -rf /var/lib/apt/lists/*` is in same RUN ✓. `apt` binary remains in image — could be removed via `apt-get purge` but that breaks Debian trust. Acceptable. |
| Least Privilege: Non-root, no sudo, no setuid | ✅ Pass | `USER coder`, no sudo in install list |
| Layer Optimization: Single RUN for apt-get + rm lists | ✅ Pass | Lines 69-75 (builder), 122-138 (runtime) |
| Cache Efficiency: BuildKit cache mounts | ✅ Pass | Lines 92-94 |

**Quality Standards: 4/6 MET, 1/6 PARTIAL, 1/6 FAIL** — INSUFFICIENT

---

## 📋 Self-Check Points Analysis

### Self-Check 1: SOTA Supply Chain
- [x] All FROM pinned by SHA256 digest — ✅
- [x] Cargo build uses `--locked` — ✅
- [x] No `curl | bash` or unverified downloads — ❌ **B3 introduces GitHub release downloads WITHOUT SHA256 verification (must pin SHA256 for ttyd + code-server binaries)**
- [x] Base images from official Docker Library — ✅
- [x] Digest verification documented in comments — ⚠️ Comments reference `containerd/containerd/blob/main/docs/hosts.md` which is unrelated to Docker Hub digest verification. Wrong link.

**Verdict: 3/5 PASS, 1/5 FAIL, 1/5 PARTIAL**

### Self-Check 2: Image Minimality
- [x] Only 14 packages installed (count them) — ❌ **17 packages listed** (counting each line of `apt-get install`). 14 is wrong count. And after B3 fix, it'll be even more.
- [x] No `build-essential`/`cmake`/`pkg-config` in runtime — ✅ (those are in builder only)
- [x] No `man`/`doc`/`locale` packages — ✅ (Debian base has man-db but `--no-install-recommends` minimizes)
- [x] `apt-get clean` + `rm -rf` in same layer — ✅
- [x] Image compressed size < 2GB — ⚠️ Depends on B3 fix; B5 risks >2GB.

**Verdict: 3/5 PASS, 1/5 FAIL, 1/5 PARTIAL**

### Self-Check 3: clw Binary Integration
- [x] Build stage copies all `clw-*` crate sources — ❌ **B1: missing 3 crates**
- [x] `cargo build --release --locked -p clw-cli --bin clw` succeeds — ❌ **Will fail per B1**
- [x] Binary copied to `/out/clw` then to `/usr/local/bin/clw` — ✅
- [x] `clw --version` runs in final image — ⚠️ B11: doesn't fail build on error
- [x] Binary is executable by `coder` user — ✅ (chmod 755 + USER coder)

**Verdict: 2/5 PASS, 2/5 FAIL, 1/5 PARTIAL**

---

## 📊 SCORECARD

| Category | Score | Required | Gap |
|----------|-------|----------|-----|
| Blocking Issues | 12 | 0 | **-12** |
| High Issues | 9 | 0 | **-9** |
| Medium Issues | 8 | 0 | **-8** |
| DoD Pass Rate | 36% | 100% | **-64%** |
| Invariants Enforced | 62.5% | 100% | **-37.5%** |
| Quality Standards | 67% | 100% | **-33%** |
| Self-Check Pass | 53% | 100% | **-47%** |

**OVERALL VERDICT: ❌ FAIL — Requires major rework before sign-off**

---

## 🔧 FIX PRIORITY

### Must Fix (Blockers) — non-negotiable before build attempt
1. **B1, B2** — Add ALL 8 first-party clw-* crates to COPY (clw-cli, clw-types, clw-cache, clw-chunk, clw-manifest, clw-client, clw-snapshot, clw-hydrate, clw-run). Remove duplicate. Add `clw-integration-tests`/`clw-conformance`/`clw-e2e-evidence` to `[workspace.exclude]` OR copy them.
2. **B3** — Replace `ttyd` and `code-server` apt installs with SHA256-pinned GitHub release downloads, OR pin to upstream `.deb` packages.
3. **B5** — Install `code-server` runtime dependencies (libnss3, libatk1.0-0, etc.) — at least if B3 uses tarball.
4. **B4** — Tag/digest cross-link with `rust-toolchain.toml`.
5. **B6** — noVNC `index.html` symlink target.
6. **B7, B9** — Add `HEALTHCHECK` (combined fix).
7. **B8** — Reorder `USER`/`WORKDIR`.
8. **B10** — Rename cache mount ID to `clw-target`.
9. **B11** — Exec form for `clw --version`.
10. **B12** — Remove `PASSWORD_STORE=basic`.

### Should Fix (High)
1. **H1** — Apt version pins.
2. **H2** — `.dockerignore` allow `rust-toolchain.toml`.
3. **H4** — Add `/tcp` suffix to EXPOSE.
4. **H5** — Add OCI LABELs.
5. **H6** — Document `STOPSIGNAL`.
6. **H7** — Reproducibility check in verify script.
7. **H8** — `docker buildx` in verify script.
8. **H9** — `COPY entrypoint.sh` + `supervisord.conf` (cross-WP coordination with WP-03).

### Nice to Fix (Medium)
1. **M3** — Drop `fonts-dejavu-core`.
2. **M4** — Add `DEBIAN_FRONTEND=noninteractive` ARG.
3. **M6, M8** — Remove `PASSWORD_STORE`, document `chmod +x`.

### Cross-WP Coordination Required
- **WP-03** must add `entrypoint.sh` + `supervisord.conf` COPY to the WP-02 Dockerfile (H9), OR WP-02 must add stub COPY lines that WP-03 fills in.
- **WP-01** `defaultPort = 6080` is consistent with WP-02's noVNC port (✓ no change).
- **WP-04** (clw integration) needs `clw` binary to be callable by `coder` user. WP-02 sets `chmod 755` + USER coder. ✓ no change.

---

## NEXT STEPS

1. **Apply all BLOCKING fixes** (B1-B12) — non-negotiable
2. **Apply all HIGH fixes** (H1-H9) — required for build + verify
3. **Apply all MEDIUM fixes** (M1-M8) — required for quality
4. **Re-verify all 11 DoD items pass**
5. **Re-verify all 8 invariants enforced in code**
6. **Re-verify all 6 quality standards met**
7. **Coordinate with WP-03 owner on entrypoint.sh COPY location** (H9)
8. **Proceed to Iteration 2 review**

**Do NOT proceed to WP-03+ until WP-02 is fixed and passes review.**

---

**END OF WP-02 ITERATION 1 REVIEW**
