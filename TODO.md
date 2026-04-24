# CoreLink — Roadmap MVP (12 semanas)

## Semana 1 — REAPI básico funcionando

- [ ] Vendor REAPI protos oficiais em `proto/build/bazel/remote/execution/v2/`
  - Fonte: https://github.com/bazelbuild/remote-apis
  - Deps: google/bytestream, google/rpc, google/protobuf, build/bazel/semver
- [ ] Ativar compile_protos do REAPI em `build.rs`
- [ ] Implementar `Capabilities::GetCapabilities` retornando config fixa do server
- [ ] Implementar `ContentAddressableStorage::FindMissingBlobs` (stub com HashMap em RAM)
- [ ] Implementar `ContentAddressableStorage::BatchUpdateBlobs` (stub com HashMap)
- [ ] Adicionar dependências: `aws-sdk-s3`, `blake3`, `sha2`, `hex`
- [ ] Criar `src/storage/r2.rs` com client básico R2 (S3-compatible)
- [ ] Criar `src/storage/digest.rs` com helpers BLAKE3 + SHA-256
- [ ] Substituir HashMap por R2 real
- [ ] Teste: `bazel build --remote_cache=grpc://localhost:50051` funciona

## Semana 2 — Auth + Tenant isolation

- [ ] Criar `src/auth/clerk.rs` — JWKS fetch + JWT validate
- [ ] Criar `src/auth/tenant.rs` — extrai tenant_id do claim `org_id`
- [ ] Middleware tower pra interceptar gRPC, validar token, injetar tenant
- [ ] Namespacing de R2 key por tenant: `cas/{tenant_id}/{digest}`
- [ ] Teste: 2 tenants diferentes não veem blobs um do outro

## Semana 3 — CAS Decomposition (SOTA)

- [ ] Chunker: arquivos >2MiB viram árvore Merkle de chunks de 2MiB
- [ ] Implementar `ContentAddressableStorage::SplitBlob` (REAPI spec)
- [ ] Implementar `ContentAddressableStorage::SpliceBlob`
- [ ] Manifest serialization (raw Merkle tree nodes)
- [ ] Cross-file dedup: chunks compartilhados entre blobs
- [ ] Ver referência Buildbarn ADR 0003: https://github.com/buildbarn/bb-adrs/blob/main/0003-cas-decomposition.md

## Semana 4 — Action Cache + ByteStream + Compression

- [ ] Implementar `ActionCache::GetActionResult`
- [ ] Implementar `ActionCache::UpdateActionResult`
- [ ] Postgres schema (via sqlx) — action_cache, cas_blobs, tenants
- [ ] Implementar `ByteStream::Read` (streaming download de blobs grandes)
- [ ] Implementar `ByteStream::Write` (streaming upload)
- [ ] Suporte Zstd compressed-blobs (REAPI extension)
- [ ] Teste: `bazel build --remote_cache=... --experimental_remote_cache_compression=true`

## Semana 5 — Rate limit + Billing + Observability

- [ ] Rate limit via `governor` (token bucket por tenant)
- [ ] Emitir eventos de uso pro Stripe Meter
- [ ] OpenTelemetry exporter → Grafana Cloud (OTLP)
- [ ] Métricas: req/s, p50/p99/p999 latency, error rate, R2 GB stored/served
- [ ] Sentry error reporting

## Semana 6 — Cloudflare Containers deploy

- [ ] Build Docker image via Dockerfile
- [ ] Configurar Durable Object pra lifecycle do Container
- [ ] Workers front-end pra roteamento gRPC → Container
- [ ] Primeiro deploy em dev: `wrangler deploy`
- [ ] Teste end-to-end: Bazel local → CF edge → Container → R2

## Semana 7 — Real-time event bus (diferencial SOTA)

- [ ] Durable Object `TenantEventBus` — mantém conexões WS por tenant
- [ ] Server emite evento após UploadBlob success
- [ ] Cliente SDK (lib separada) com WebSocket listener
- [ ] Pre-fetch hint baseado em heurística local

## Semana 8 — Forge como customer zero

- [ ] Integrar Forge pipeline → CoreLink (protocol depende de como Forge builda hoje)
- [ ] Documentar integração
- [ ] Monitorar em produção por 1 semana

## Semanas 9–10 — Dogfooding + tuning

- [ ] Forge rodando 100% via CoreLink
- [ ] Bug fix em produção real
- [ ] Performance tuning (identificar hot spots)
- [ ] Aumentar coverage de testes integração

## Semana 11 — Going public

- [ ] Landing page (`corelink.humangr.com`) em Cloudflare Pages
- [ ] Dashboard MVP em `app.corelink.humangr.com`
- [ ] Stripe checkout integrado (tiers BR + global)
- [ ] Docs em `docs.corelink.humangr.com`
- [ ] Status page Better Stack em `status.corelink.humangr.com`

## Semana 12 — Soft launch

- [ ] 10 devs beta na comunidade BR (Tabnews, Rocketseat, Discord Rust BR)
- [ ] Coleta de feedback
- [ ] Métricas de engagement
- [ ] Ajuste fino pré-launch público

## Pós-MVP (mês 4+)

- [ ] **sccache protocol** (comunidade Rust viral)
- [ ] **Turborepo Remote Cache protocol**
- [ ] **OCI Distribution Spec** (Docker layer cache)
- [ ] **Gradle Build Cache protocol**
- [ ] **target/ offloading virtual mount** (feature "foda" fase 3)
- [ ] **Team tier** (cache compartilhado + dashboard + analytics)
- [ ] **Enterprise tier** (SSO, audit log, on-prem option)

## Decisões pendentes

- [ ] Forge usa build cache hoje? Qual linguagem o core? (define ordem de protocolos após REAPI)
- [ ] Quando migrar Neon → D1? (benchmarkar latência após dogfood)
- [ ] Open-source parcial? (NativeLink é Apache 2.0, comunidade adora; mas perde moat)
