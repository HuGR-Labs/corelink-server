### Fixed

- **B-091 — o SBOM Rust agora é uma projeção determinística e completa de
  `Cargo.lock`.** O artefato comprometido enumera os 652 pacotes bloqueados,
  preserva a identidade `(nome, versão, source)` sem inferir cobertura por uma
  simples contagem e usa `LicenseRef-CoreLink-Proprietary` em vez do inválido
  `UNLICENSED`. O harness falha para saída ausente ou stale, erro do gerador,
  licença inválida e qualquer componente conhecido ou pacote do lock ausente.
  O gate de PR/push também cobre todos os manifests membros via
  `**/Cargo.toml`, incluindo novos membros, e testa por mutação que a regra
  recursiva não pode desaparecer silenciosamente. Isso fecha a verificação do
  SBOM; não altera a declaração de nível da cadeia SLSA.
