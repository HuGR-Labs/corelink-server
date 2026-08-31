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
  failover de LEITURA cruza região automaticamente, ao contrário do que a página afirmava.
  Nenhum código foi alterado.
