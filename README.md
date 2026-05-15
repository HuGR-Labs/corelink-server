# HuGR CoreLink — Server

Shared content-addressable cache pra developers. Implementação SOTA de REAPI (Remote Execution API) em Rust, deployada em Cloudflare Containers.

> **Architecture overview?** See [`ARCHITECTURE.md`](./ARCHITECTURE.md) — purpose + invariant guarantees, system context, 12 core crates, tenant model, data lifecycle, BYOK envelope, audit-chain Merkle, top-10 SLOs, failure-mode taxonomy, compliance posture (SOC 2 / LGPD / GDPR / ISO 27001 / PCI DSS), plus 8 companion Mermaid diagrams under [`docs/internal/architecture/diagrams/`](./docs/internal/architecture/diagrams/).
>
> **New engineer?** Start with [`docs/internal/ENGINEERING-ONBOARDING.md`](./docs/internal/ENGINEERING-ONBOARDING.md) — Day-0 through Day-30 path, 5 domain tracks, curated first-PR backlog, glossary cheatsheet, buddy protocol.

## Stack

- **Linguagem:** Rust
- **gRPC:** tonic
- **Protocolo:** REAPI (Remote Execution API v2, Bazel spec)
- **Hashing:** BLAKE3 (primary) + SHA-256 (fallback)
- **CAS:** Merkle-decomposed, 2MiB chunks, cross-file dedup
- **Compression:** Zstandard no ByteStream
- **Storage:** Cloudflare R2
- **Deploy:** Cloudflare Containers (gRPC) + Workers (control plane) + Durable Objects (event bus)
- **DB:** Neon Postgres (migrar pra D1 se latência pedir)
- **Auth:** Clerk
- **Billing:** Stripe (meter-based usage)

## Run localmente

Pré-requisitos:

- Rust 1.80+
- `protoc` instalado (`brew install protobuf`)

```bash
cargo build
cargo run
```

Server sobe em `0.0.0.0:50051` (override via `PORT`).

Teste o health check com `grpcurl`:

```bash
grpcurl -plaintext localhost:50051 corelink.health.v1.Health/Check
```

## Deploy (Cloudflare Containers)

```bash
# Uma vez
wrangler login
wrangler secret put CLERK_SECRET_KEY
wrangler secret put STRIPE_SECRET_KEY
wrangler secret put DATABASE_URL

# Dev
wrangler deploy

# Prod
wrangler deploy --env prod
```

## Estrutura

```
corelink-server/
├── Cargo.toml               # Deps mínimas pro scaffold
├── build.rs                 # compila .proto via tonic-build
├── Dockerfile               # multi-stage pra CF Containers
├── wrangler.toml            # Cloudflare Workers + Containers config
├── proto/
│   └── health.proto         # health check (placeholder até REAPI vendored)
├── src/
│   └── main.rs              # gRPC server bootstrap
└── migrations/
    └── 0001_init.sql        # schema Neon (tenants, CAS index, AC index, usage)
```

## Verifying CLI release downloads (WI-S15-006)

Linux release tarballs are GPG-signed; the matching public key is
published at the canonical `.well-known` location:

```bash
# One-time: import the CoreLink release-signing pubkey.
curl -fsSL https://corelink.dev/.well-known/gpg-pubkey.asc | gpg --import

# Per download: verify the detached signature against the tarball.
gpg --verify corelink-linux-x86_64.tar.gz.asc corelink-linux-x86_64.tar.gz
```

macOS binaries ship Apple-notarized (Gatekeeper accepts them without a
right-click bypass); Windows binaries ship Authenticode-signed (see
`ADR-S15-009` for the deferral-only policy if cert acquisition slips).

## Status

MVP em construção. Ver [TODO.md](./TODO.md) pro roadmap semana 1–12.

## Licença

Proprietária (HuGR). Discussão sobre open-sourcing parcial pendente.
