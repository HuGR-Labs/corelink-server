# syntax=docker/dockerfile:1.6
# Multi-stage build otimizado pra Cloudflare Containers.
# Target: menor imagem possível, binário estaticamente linkado.
#
# === 2026-05-28 cache-invalidation fix (WHY this layout) ===
#
# The PRIOR layout used a stub-build trick (write empty `main.rs`+`lib.rs`,
# `cargo build -p corelink-server`, delete the stubs, COPY real source,
# rebuild) to keep transitive deps in a cached image layer. That layout
# shipped a prod incident: a one-file fix in `crates/corelink-container/
# src/routes/cas.rs` produced bit-identical image hashes on three
# consecutive `wrangler containers build` invocations, blocking deploy.
#
# Root cause: the stub build leaves a `target/` populated with cargo
# fingerprints derived from the stub source contents. That target/ ends
# up baked into the cached image layer. On the subsequent real-source
# `cargo build`, cargo's incremental compilation walks those fingerprints
# and — under certain inputs (subtle file-mtime or content-hash overlap,
# and confirmed under `wrangler containers build`'s caching path which
# isn't cleared by `docker buildx prune -af`) — concludes the
# `corelink-server` crate doesn't need recompilation. Result: the
# release binary at `/build/target/release/corelink-server` is the
# STUB binary, not the real one. The `COPY --from=builder` then ships
# that stub-derived binary, layer hash unchanged across builds.
#
# Fix (chosen for minimum complexity + maximum correctness): replace the
# stub-build trick with BuildKit cache mounts for both `$CARGO_HOME`
# (registry/git) AND `/build/target`. Both live outside the image layer
# tree, so:
#   1. The image layer for the build RUN step is content-addressed only
#      by the COPY inputs + RUN script — no stale fingerprint state can
#      hide in it across builds.
#   2. `cargo build` still gets full incremental + dep cache benefits on
#      warm rebuilds via the cache mount.
#   3. We copy the binary OUT of the cache mount to `/out/` within the
#      same RUN step so the runtime stage's `COPY --from=builder`
#      can find it (cache mounts only exist during their RUN step).
#
# We additionally pass `CARGO_INCREMENTAL=0`: in release-profile builds
# incremental adds little (per cargo docs) and removes the precise
# class of fingerprint-staleness that bit us. Clean release rebuilds of
# the bin crate on warm cache are still cheap (deps cached in mount).
#
# Cold-build cost: unchanged from prior layout (~10–15 min on amd64).
# Warm-build cost: comparable (~1–3 min for a one-file bin-crate edit)
# because the cargo cache mount still holds compiled deps.
# Layer-hash invariant: any change under `crates/`, `tools/`,
# `tests/`, `migrations/`, `apps/migrate-single-to-multi-region/`,
# `Cargo.toml`, or `Cargo.lock` busts the build RUN step and produces
# a fresh binary in the output image.
#
# Wave-33 Stage 2.B.2 — Dockerfile referenced a `src/` at repo root that
# never existed; binary lives in `crates/corelink-container/`. The
# `[workspace]` Cargo resolver requires every member's manifest before
# scheduling any build, so we copy the whole workspace tree.

# ---- Build stage ----
# HO-1: digest-pinned per Wave-32 Phase E audit (supply-chain integrity).
# Tag rust:1.91-slim-bookworm is preserved alongside the digest for human
# readability; the digest is the authoritative reference. Refresh both
# together when bumping the Rust toolchain.
FROM rust:1.91-slim-bookworm@sha256:ac77791dbc2ab3cd3ab732fe9b45b0414a794743da99e679fa99e8faa3b6c1e3 AS builder

# Deps pra compilar protos e linkagem
RUN apt-get update && apt-get install -y --no-install-recommends \
    protobuf-compiler \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Workspace manifest + lock + per-crate manifests + sources.
# Copying the entire `crates/` + `tools/` tree is unavoidable because the
# workspace resolver reads every member's `Cargo.toml` before scheduling
# any build; restricting the copy to manifests-only would require a
# parallel manifest-only synthetic tree (adds drift risk for marginal
# cache savings on a workspace with 120+ members).
#
# All COPY layers below feed the cache key of the build RUN step. Any
# source-byte change under these paths invalidates the build layer and
# forces a fresh `cargo build` against the cache-mounted state.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY tools ./tools
# Wave-32 Phase E APPLY fix: all workspace members outside crates/ and tools/
# must be present so cargo can resolve the workspace manifest tree. These are
# Rust-only dirs; the non-Rust apps (admin-ui, docs, server) are NOT workspace
# members and are deliberately excluded to keep the build context minimal.
COPY apps/migrate-single-to-multi-region ./apps/migrate-single-to-multi-region
COPY tests ./tests
# Wave-32 Phase E APPLY fix: many crates embed SQL migration files via
# include_str!("../../../migrations/d1/*.sql") at compile time. The
# migrations/ directory must be present in the build context.
COPY migrations ./migrations

# Single build RUN with BuildKit cache mounts.
#
# - `--mount=type=cache,target=/usr/local/cargo/registry` caches the
#   crates.io index + downloaded crate sources across builds.
# - `--mount=type=cache,target=/usr/local/cargo/git` caches git-deps.
# - `--mount=type=cache,target=/build/target,id=corelink-target` caches
#   compiled artifacts + cargo's incremental fingerprint database.
#   `sharing=locked` (default) so concurrent builds don't corrupt it.
#
# Cache mounts are RUN-scoped — they don't appear in the resulting image
# layer. That's the whole point: the build layer hash depends on COPY
# content + RUN script only, never on accumulated cargo state. After
# `cargo build` completes, we copy the binary out of the cache mount
# into `/out/` so the runtime stage's `COPY --from=builder` can read it
# (runtime stage can't see this stage's cache mounts).
#
# `CARGO_INCREMENTAL=0` — release builds don't benefit much from
# incremental, and removing it eliminates the fingerprint-staleness
# class that the prior stub-build layout could trigger.
ENV CARGO_INCREMENTAL=0
RUN --mount=type=cache,target=/usr/local/cargo/registry,id=corelink-cargo-registry \
    --mount=type=cache,target=/usr/local/cargo/git,id=corelink-cargo-git \
    --mount=type=cache,target=/build/target,id=corelink-target,sharing=locked \
    cargo build --release -p corelink-server --bin corelink-server \
 && mkdir -p /out \
 && cp /build/target/release/corelink-server /out/corelink-server

# ---- Runtime stage ----
# HO-1: digest-pinned per Wave-32 Phase E audit (supply-chain integrity).
FROM debian:bookworm-slim@sha256:b29f74a267526ae6ea104eed6c46133b0ca70ce812525df8cd5817698f0a624a

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd --system --gid 1000 corelink \
 && useradd --system --uid 1000 --gid corelink corelink

# Binary copied out of the builder's cache mount into /out/ (see build
# RUN step above). The cache mount itself is not visible to this stage.
COPY --from=builder /out/corelink-server /usr/local/bin/corelink-server

USER corelink

# Consolidated runtime env (HO-3a — per Wave-32 Phase E audit).
ENV RUST_LOG=info \
    PORT=50051

EXPOSE 50051

ENTRYPOINT ["/usr/local/bin/corelink-server"]
