### Fixed

- **A página LGPD publicada prometia São Paulo e imutabilidade à prova de ordem judicial (B-085).**
  `apps/docs/docs/explanation/residency/lgpd-brazil.mdx` afirmava a titulares brasileiros
  que blobs, action cache, trilha de auditoria e metadados de um tenant `sam` estavam
  fisicamente em São Paulo, em buckets `cas-sam` / `ac-sam` / `audit-sam` que não existem.
  O binding de CAS do `prod-sam` é o `corelink-cas-prod` dos EUA, o `corelink-ac-sam` é
  provisionado com `locationHint=enam`, e `worker/src/region-map.ts` exclui `sam` do
  conjunto provisionável justamente para não rotular dados dos EUA como brasileiros. A
  página agora declara a indisponibilidade da residência no Brasil e reclassifica todas as
  categorias de Art. 33 *caput* para Art. 33, V + cláusulas do DPA. Caíram junto quatro
  outras afirmações medidas: a trilha de auditoria é *tamper-evident*, não imutável (o R2
  não implementa Object Lock); o rejeito cross-region servido é `409 residency_violation`,
  não `451`; a crate e o comando `cargo test` citados para verificação não existem; e o
  failover bloqueia **escrita** com `503 failover_readonly` mas serve a **leitura
  localmente** — o cabeçalho `x-corelink-failover-read-region` é advisório e **nenhum
  consumidor do edge o lê** (zero em `*.ts`/`*.js`/`*.toml`; controle positivo em `*.rs` e
  em `x-corelink-primary-region`), de modo que o risco de leitura cross-region é
  **latente**, não ativo. As três cópias i18n (`de`, `es-419`, `pt-BR`) receberam o mesmo
  texto e agora estão dentro do portão, que conta as 4 locales e falha se varrer menos.
  Nenhum código foi alterado.

- **A página que linka a LGPD mantinha as duas mentiras que ela retrata (B-085).**
  `apps/docs/docs/explanation/privacy/lgpd-full.mdx` §6 dizia que com a Cloudflare os
  dados *"stay in São Paulo for `sam`-region tenants"* — apontando justamente para a
  página corrigida — e listava AWS São Paulo / GCP `southamerica-east1` / Azure Brazil
  South como sub-processadores que *"host your BYOK envelope key"*. BYOK não é entregue:
  `POST /v1/admin/byok/activate` falha fechado com `501 Not Implemented` /
  `byok_not_available` (`crates/corelink-container/src/routes/byok_admin.rs:245-257`) e o
  único provedor compilado é um fake em memória. Ambas as entradas foram corrigidas na EN
  e nas três cópias i18n; a de BYOK foi mantida como retratação visível em vez de apagada.
