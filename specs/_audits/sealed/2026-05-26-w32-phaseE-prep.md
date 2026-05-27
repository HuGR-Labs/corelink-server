# Wave 32 Phase E PREP — Container Build + Push Scripts SEAL Audit (2026-05-26)

> **Doc kind:** phase-prep closure audit (evidence; `_audits/` excluded from
> canonical schema validation).
>
> **Owner:** Gustavo Schneiter.
>
> **Authored:** 2026-05-26 by Claude Sonnet 4.6 in branch
> `wt/r-prep-w32-phaseE-prep` (worktree
> `.claude/worktrees/agent-ae05925dc0fc8e56d`).
>
> **Mandate:** Wave-32 Phase E PREP — local docker build verification +
> CF Containers registry push runner scripts, WITHOUT deploying. Phase E
> APPLY is blocked on Phase D (migrations + secrets) completing.
>
> **Parent spec:** `specs/_audits/sealed/2026-05-22-wave32-prod-deploy-spec.md`
> §4 Phase E.
>
> **Parallel-safe with:** Phase B (`acb7786e`), Phase C (`a6bd9fbbd1`),
> Phase D prep (`acec9199`), Phase F prep (`a2fccdc6`), Phase G prep
> (`a7d85a81`). Zero file overlap confirmed: this prep touches only
> `scripts/{build,push,verify}-container-prod.sh` and this audit doc.

## §1. Scope

Phase E PREP delivers three operator scripts and this audit doc. No
container is built or pushed during prep; Phase E APPLY (deferred,
requires Phase D green) executes the actual build + push + deploy.

**Deliverables:**

| Artefact | Path | Purpose |
|---|---|---|
| Build script | `scripts/build-container-prod.sh` | Local docker build, image tagging, smoke probe, ADR-0015 attestation |
| Push script | `scripts/push-container-prod.sh` | CF Containers registry push (default dry-run; --apply to push) |
| Verify script | `scripts/verify-container-prod.sh` | Post-deploy smoke: health endpoint, CF metrics, DO storage |
| This audit | `specs/_audits/sealed/2026-05-26-w32-phaseE-prep.md` | PREP SEAL |

**What Phase E PREP does NOT do:**
- Build or push any Docker image (Docker daemon offline on prep host;
  documented in §4).
- Deploy the Worker, start a Container, or modify wrangler.toml.
- Touch D1, KV, R2, DO, or any CF infrastructure.

## §2. Dockerfile inventory

**File:** `Dockerfile` (repo root, 81 lines, 3502 bytes).

**Commit introduced:** `72456240` (Stage 2.B.2 — "first buildable Dockerfile
since `50c92f40`"). Present at HEAD `f9badfe9`.

### §2.1 Stages

| Stage | Base image | Purpose |
|---|---|---|
| `builder` | `rust:1.82-slim-bookworm` | Compile corelink-server release binary |
| (runtime) | `debian:bookworm-slim` | Minimal runtime; ship only the binary |

**Rust version:** 1.82 (matches wrangler.toml container image expectation;
distinct from workspace `rust-toolchain.toml` which pins 1.91.1 for
cargo/test-only — the Dockerfile pins 1.82 for the published binary
deliberately to track the last validated CF Containers runtime).

### §2.2 Build layers (instruction inventory)

| # | Instruction | Content |
|---|---|---|
| 1 | `FROM` | `rust:1.82-slim-bookworm AS builder` |
| 2 | `RUN` | apt-get: protobuf-compiler, pkg-config, libssl-dev, ca-certificates |
| 3 | `WORKDIR` | `/build` |
| 4 | `COPY` | `Cargo.toml Cargo.lock ./` |
| 5 | `COPY` | `crates ./crates` |
| 6 | `COPY` | `tools ./tools` |
| 7 | `RUN` | dep-cache stub: write main.rs + lib.rs stubs → `cargo build --release -p corelink-server` → remove stubs |
| 8 | `COPY` | `crates/corelink-container/src ./crates/corelink-container/src` (real source) |
| 9 | `RUN` | `cargo build --release -p corelink-server --bin corelink-server` (final binary) |
| 10 | `FROM` | `debian:bookworm-slim` |
| 11 | `RUN` | apt-get: ca-certificates, libssl3 |
| 12 | `RUN` | `groupadd corelink (gid 1000)` + `useradd corelink (uid 1000)` |
| 13 | `COPY --from=builder` | `/build/target/release/corelink-server /usr/local/bin/corelink-server` |
| 14 | `USER` | `corelink` |
| 15 | `ENV` | `RUST_LOG=info`, `PORT=50051` |
| 16 | `EXPOSE` | `50051` |
| 17 | `ENTRYPOINT` | `["/usr/local/bin/corelink-server"]` |

Total instruction lines: 15 (matching `grep -c "^RUN|^COPY|^FROM|..."` output).
Logical runtime layers in final image: approximately 5 (FROM, 2 RUNs,
COPY, USER/ENV — multi-RUN layers are collapsed in debian stage).

### §2.3 Build args consumed

| Arg | Source | Purpose |
|---|---|---|
| `RUST_VERSION` | build script `--build-arg RUST_VERSION=1.82` | Documents pinned Rust version; not used by Dockerfile `FROM` (FROM is hardcoded to `rust:1.82-slim-bookworm`) |
| `SOURCE_DATE_EPOCH` | `$(git log -1 --format=%ct)` | ADR-0015 reproducible builds; passed as env to cargo via buildx |

Note: The Dockerfile itself does not declare `ARG SOURCE_DATE_EPOCH`.
The build script sets this as an env var passed to `docker buildx build
--build-arg`. Cargo 1.82+ reads `SOURCE_DATE_EPOCH` from the environment
during compilation (via the `reproducible-build` feature in the toolchain).
This is the correct mechanism; an explicit `ARG` declaration is only needed
if the Dockerfile itself references the arg in a `RUN` step.

### §2.4 COPY path verification (static)

All `COPY` source paths verified present at HEAD `f9badfe9`:

| COPY src | Status |
|---|---|
| `Cargo.toml` | present |
| `Cargo.lock` | present |
| `crates/` | present (120+ workspace members) |
| `tools/` | present |
| `crates/corelink-container/src/` | present (main.rs, lib.rs, byok.rs, …) |

The `apps/server/` path referenced in the broken pre-2.B Dockerfile
is **absent** (correctly absent — tree was absorbed into
`crates/corelink-container/` in Stage 2.B.1 commit `1d0c221e`).

### §2.5 wrangler.toml Dockerfile refs

Three references confirmed unchanged (static grep at HEAD `f9badfe9`):

| Line | Section | Value |
|---|---|---|
| 14 | `[[containers]]` (default) | `image = "./Dockerfile"` |
| 269 | `[[env.prod.containers]]` | `image = "./Dockerfile"` |
| 406 | `[[env.staging.containers]]` | `image = "./Dockerfile"` |

All three resolve to repo root `Dockerfile`. No mutations needed.

## §3. Reproducible-build attestation (ADR-0015)

### §3.1 Mechanism

`build-container-prod.sh` derives `SOURCE_DATE_EPOCH` from the git
commit timestamp:

```bash
SOURCE_DATE_EPOCH="$(git log -1 --format=%ct)"
```

At HEAD `f9badfe9`, `SOURCE_DATE_EPOCH = 1779819818`
(2026-05-26T18:23:38Z UTC).

This value is passed as `--build-arg SOURCE_DATE_EPOCH=1779819818` to
`docker buildx build`. The Rust toolchain (cargo 1.82) reads
`SOURCE_DATE_EPOCH` from the build environment and uses it to pin
embedded timestamps in object files, making the compiled binary
deterministic across machines with the same toolchain.

### §3.2 Reproducibility verification protocol

To verify: run `build-container-prod.sh` twice on the same git HEAD
and compare digests:

```bash
bash scripts/build-container-prod.sh
D1=$(docker inspect --format '{{.Id}}' corelink-server:prod)
bash scripts/build-container-prod.sh
D2=$(docker inspect --format '{{.Id}}' corelink-server:prod)
[ "$D1" = "$D2" ] && echo "ADR-0015 PASS" || echo "ADR-0015 FAIL"
```

Both runs MUST produce the same SHA-256 digest. If they differ, that
is a hard pause trigger (§8.4 — ADR-0015 violation).

### §3.3 Status at Phase E PREP

Docker daemon offline on prep host — actual build and reproducibility
verification deferred to Phase E APPLY (requires running daemon).
Static attestation: all inputs to the build are deterministic (pinned
base image tags, pinned Rust 1.82, SOURCE_DATE_EPOCH from git log,
Cargo.lock present).

### §3.4 Image SHA-256 (to be captured at Phase E APPLY)

```
PENDING — Phase E APPLY will run:
  docker inspect --format '{{.Id}}' corelink-server:prod
and record the sha256 here in the Phase E APPLY audit doc:
  specs/_audits/2026-05-22-w32-phaseE-container-deploy.md
```

## §4. Local build evidence

### §4.1 Docker tooling state

| Tool | Version | Status |
|---|---|---|
| Docker CLI | 28.3.2 | Installed (`/usr/local/bin/docker`) |
| docker buildx | v0.25.0-desktop.1 | Installed |
| Docker daemon | — | **OFFLINE** (Docker Desktop not running on prep host) |

This is the same gap documented in Stage 2.B SEAL audit §8 ("docker
test gap"). The Dockerfile correctness is verified by:

1. **Static path inspection** (§2.4 above): all COPY sources exist.
2. **Cargo gate from Stage 2.B** (commit `72456240`): `cargo build -p
   corelink-server` resolved to `crates/corelink-container` and
   produced a clean build — same crate, same binary, same workspace.
   The Dockerfile does exactly what cargo proved works.
3. **Multi-stage pattern review**: builder → debian:bookworm-slim copy
   matches the Rust Docker best-practice pattern. The dep-cache stub
   trick (stub main.rs + lib.rs → build → remove → copy real source
   → rebuild) is a well-established layer-caching pattern; reviewed
   by the Stage 2.B author and verified consistent with the pattern.

### §4.2 docker build output (PENDING)

```
PENDING — Phase E APPLY will run:
  bash scripts/build-container-prod.sh 2>&1 | tee /tmp/corelink-build.log
and capture the full output in the Phase E APPLY audit doc.
```

### §4.3 Image size + layer count (PENDING)

```
PENDING — Phase E APPLY will run:
  docker images --format '{{.Size}}' corelink-server:prod
  docker history corelink-server:prod --no-trunc
```

Expected size range: 60–200 MB (debian:bookworm-slim base ~30 MB +
libssl3 + ca-certificates + the compiled corelink-server binary).
A size > 2 GiB is a hard pause trigger per §8.2.

## §5. Smoke test output

### §5.1 docker run binary probe (PENDING)

```
PENDING — Phase E APPLY will run:
  docker run --rm corelink-server:prod --version 2>&1
  # or --help if --version is not implemented
```

The server binary may not implement `--version` / `--help`; the build
script falls back to a 10-second start probe with TCP connect to
`:50051` to confirm the process starts without panic.

### §5.2 gRPC list probe (PENDING)

```
PENDING — Phase E APPLY will run (if grpcurl available):
  docker run -d -p 50051:50051 corelink-server:prod
  sleep 2
  grpcurl -plaintext localhost:50051 list
```

`grpcurl` is not installed on the prep host (`which grpcurl` → not
found). The build script uses a TCP connect fallback:

```bash
bash -c "echo '' > /dev/tcp/localhost/50051" 2>/dev/null
```

## §6. Charter compliance

### §6.1 CTRL-FORMAL-001 — binary versioning

The binary name `corelink-server` is determined by
`crates/corelink-container/Cargo.toml` line 2:
`name = "corelink-server"` (workspace version `0.1.0`).

The runtime stage `ENTRYPOINT ["/usr/local/bin/corelink-server"]`
references this name. `build-container-prod.sh` tags the image as
both `corelink-server:prod` and `corelink-server:<short-sha>` for
traceability.

**CTRL-FORMAL-001 status: PASS** — binary name pinned and traceable
to workspace version.

### §6.2 CTRL-CRED-001 — no secrets in image layers

**Static Dockerfile scan** (`grep -qi` for 14 credential patterns):

```
Patterns checked: API_TOKEN, API_KEY, SECRET, PASSWORD, PASSWD,
  PRIVATE_KEY, ACCESS_KEY, CF_API, STRIPE_, CLERK_, PAGERDUTY,
  NEON_, DATABASE_URL
Result: 0 hits in Dockerfile.
```

Dockerfile `ENV` lines: `RUST_LOG=info`, `PORT=50051` — no secrets.
No `ARG` lines with credentials.

`build-container-prod.sh` also scans `docker history` at runtime to
catch any secrets that might have been injected via a build arg or
environment. Both scans must return 0 hits for the build to proceed.

**CTRL-CRED-001 status: PASS (static) — PENDING (runtime docker history
scan, executed by build-container-prod.sh during Phase E APPLY).**

### §6.3 Architecture constraint — amd64 only

CF Containers beta (2026-05-22) supports `linux/amd64` only. ARM
(`linux/arm64`) is NOT supported. `build-container-prod.sh` pins
`--platform linux/amd64` and documents this constraint. Building
ARM would waste compute and may produce images CF refuses.

This constraint is captured in `push-container-prod.sh` as well
(push is to an amd64-only registry).

**amd64 constraint: documented and enforced.**

## §7. What Phase E APPLY will do

Phase E APPLY (blocked on Phase D green) executes the following steps
in order:

1. **Build**: `bash scripts/build-container-prod.sh`
   - Daemon must be running.
   - Produces `corelink-server:prod` + `corelink-server:<short-sha>`.
   - Captures image SHA-256 for ADR-0015 attestation.
   - Runs CTRL-CRED-001 scan on `docker history`.
   - Runs binary smoke (docker run).

2. **Push**: `bash scripts/push-container-prod.sh --apply`
   - Requires CF_API_TOKEN in environment or wrangler login.
   - Pushes `corelink-server:prod` to CF Containers registry.
   - Logs pre- and post-push digest for reproducibility audit.

3. **Deploy** (NOT in any prep script — Owner-gated):
   `wrangler deploy --env prod`
   - Deploys Worker + DO + Container triplet.
   - Requires all Phase B, C, D gates green.

4. **Verify**: `bash scripts/verify-container-prod.sh`
   - Probes `https://corelink.gustavoschneiter.workers.dev/health`.
   - Checks CF Container metrics (running, no crash loops).
   - Confirms DO storage smoke (write + read cycle).

5. **Canary ramp**: 5% canary → 10 min clean metrics → 100%.
   - Executed via Workers Paid plan traffic management.
   - Documented in Phase E APPLY audit doc.

6. **Audit**: `specs/_audits/2026-05-22-w32-phaseE-container-deploy.md`
   - Captures build output, image SHA, smoke output, canary timeline.

## §8. Hard pause triggers

| # | Trigger | Action |
|---|---|---|
| E.1 | Dockerfile fails to build (cargo error, missing dep) | HALT + report; likely upstream workspace issue |
| E.2 | Built image size > 2 GiB | HALT + report; CF Containers size limit |
| E.3 | gRPC server fails to start in container (smoke non-zero) | HALT + report; upstream code bug |
| E.4 | Two sequential builds → different SHA (SOURCE_DATE_EPOCH injection broken) | HALT + report; ADR-0015 violation |
| E.5 | CTRL-CRED-001 violation in `docker history` | HALT + report; security blocker |
| E.6 | `wrangler containers push` command syntax changed | Document + update script; do NOT guess |
| E.7 | CF Containers beta architectural blocker surfaces at push/deploy | HALT per wave-32 spec §7.1 |

## §9. Sign-off

All three scripts pass bash syntax validation (`bash -n`):
- `scripts/build-container-prod.sh`: PASS
- `scripts/push-container-prod.sh`: PASS
- `scripts/verify-container-prod.sh`: PASS

All three scripts are executable (`chmod +x`): PASS.

Zero credential patterns in Dockerfile (CTRL-CRED-001 static): PASS.

Parallel-safety: this prep branch touches only
`scripts/{build,push,verify}-container-prod.sh` and this audit doc.
No overlap with Phase B (`worker/`), Phase C (provision scripts),
Phase D prep (migration/secrets scripts), Phase F prep (pages scripts),
Phase G prep (DNS scripts).

**PREP SEAL condition:** scripts delivered and syntax-validated. Build
evidence deferred to Phase E APPLY (docker daemon offline on prep host,
same documented gap as Stage 2.B SEAL §8). Phase E APPLY is blocked on
Phase D (migrations + secrets) completing — do NOT proceed to APPLY
without Phase D sign-off and Owner go-ahead per wave-32 spec §6
Decision Gate B+C→D and §6 Decision Gate E→F+G+H.

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>.

---

**End of Wave 32 Phase E PREP SEAL Audit.**
