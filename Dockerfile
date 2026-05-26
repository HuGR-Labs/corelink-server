# Multi-stage build otimizado pra Cloudflare Containers.
# Target: menor imagem possível, binário estaticamente linkado.
#
# Wave-33 Stage 2.B.2 — Fix de bug pré-existente (broken since `50c92f40`):
# o Dockerfile referenciava um `src/` na raiz que nunca existiu (o binário
# vivia em `apps/server/`), o que impedia qualquer build do container. Com
# 2.B.1 o binário canônico passou a viver em `crates/corelink-container/`,
# então o Dockerfile agora monta o workspace inteiro (Cargo.toml +
# Cargo.lock + crates/ + tools/) e builda só o pacote `corelink-server`.
# O `[workspace]` Cargo não permite build single-crate sem o tree
# (members são resolvidos relativos ao manifest do workspace), então
# copiamos tudo. A camada de cache de deps continua sendo a primeira
# linha grossa (manifests + stubs main/lib), e só o source real entra na
# camada subsequente — o `target/` continua sendo invalidado apenas
# quando o código real muda.

# ---- Build stage ----
FROM rust:1.82-slim-bookworm AS builder

# Deps pra compilar protos e linkagem
RUN apt-get update && apt-get install -y --no-install-recommends \
    protobuf-compiler \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Cache-friendly layer: workspace manifest + lock + per-crate manifests.
# Copying the entire `crates/` + `tools/` tree is unavoidable because the
# workspace resolver reads every member's `Cargo.toml` before scheduling
# any build; restricting the copy to manifests-only would require keeping
# a parallel manifest-only synthetic tree, which adds drift risk for
# marginal cache savings (the workspace has 120+ members already).
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY tools ./tools
# Wave-32 Phase E APPLY fix: all workspace members outside crates/ and tools/
# must be present so cargo can resolve the workspace manifest tree. These are
# Rust-only dirs; the non-Rust apps (admin-ui, docs, server) are NOT workspace
# members and are deliberately excluded to keep the build context minimal.
COPY apps/migrate-single-to-multi-region ./apps/migrate-single-to-multi-region
COPY tests ./tests

# Cache de deps: stub `main.rs` + `lib.rs` no container crate (o bin
# depende do lib do mesmo crate via `use corelink_server::*`) + builda só
# o pacote `corelink-server` no release profile. Isso popula
# `target/release/deps/` com todas as transitive deps fechadas (tonic,
# axum, tokio, AWS SDK, hyper, reqwest, etc.) sem ser invalidado quando
# o source real de `corelink-container/src/` muda. Os manifests
# (`Cargo.toml`s) e `Cargo.lock` já foram copiados acima — esta layer
# cacheia ~95% do build time em re-builds incrementais.
RUN echo "fn main() {}" > crates/corelink-container/src/main.rs \
 && echo "//! stub for dep cache layer" > crates/corelink-container/src/lib.rs \
 && cargo build --release -p corelink-server \
 && rm crates/corelink-container/src/main.rs crates/corelink-container/src/lib.rs

# Agora copia o source real do bin crate (sobrescreve a remoção dos
# stubs acima) e rebuilda só o crate `corelink-server` — as deps
# continuam vindo do cache da layer anterior. O `build.rs` + `proto/`
# do crate já vieram no `COPY crates ./crates` inicial e não precisam
# ser recopiados.
COPY crates/corelink-container/src ./crates/corelink-container/src
RUN cargo build --release -p corelink-server --bin corelink-server

# ---- Runtime stage ----
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd --system --gid 1000 corelink \
 && useradd --system --uid 1000 --gid corelink corelink

COPY --from=builder /build/target/release/corelink-server /usr/local/bin/corelink-server

USER corelink

ENV RUST_LOG=info
ENV PORT=50051

EXPOSE 50051

ENTRYPOINT ["/usr/local/bin/corelink-server"]
