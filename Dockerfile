# Multi-stage build otimizado pra Cloudflare Containers
# Target: menor imagem possível, binário estaticamente linkado

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

# Cache de deps: copia só os manifests primeiro
COPY Cargo.toml Cargo.lock* ./
COPY build.rs ./
COPY proto ./proto

# Cria src/main.rs vazio pra cachear build de deps
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src

# Agora copia o source real e builda
COPY src ./src
RUN touch src/main.rs && cargo build --release

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
