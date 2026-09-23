---
name: own-corelink-worker
description: >-
  Governa a composição estática, features e superfícies públicas de
  corelink-worker; não converte nomes de módulos ou contratos-fonte em prova
  de worker, requisição, storage, provider, deploy ou produção.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-worker"
  manifest: "crates/corelink-worker/Cargo.toml"
  source-commit: "16d9f0303d849a1ab3df14688bd2c7cdbfee8140"
  evidence-set: "w011-worker-static-20260920"
---

# Ownership — corelink-worker

Fonte estática somente. Este ownership governa o grafo declarado de módulos,
features, reexports e dependências do package; não autoriza inferir que qualquer
adapter, middleware, handler ou fake recebe requisições ou alcança um serviço.

[Escopo](#s01) · [Superfície](#s02) · [Triagem](#s03) · [Método](#s04) · [Parada](#s05) · [Saída](#s06) · [Evidência](#s07).

<a id="s01"></a>
## S01 — Escopo e autoridade

| Use quando | Não use quando |
|---|---|
| Mudar `Cargo.toml`, feature, módulo, reexport, trait ou tipo público | Executar Cargo/teste, rede, worker, request, storage, provider ou deploy |
| Censar consumidores Rust estáticos e compatibilidade de paths | Declarar R2/KV/Neon/DO, middleware HTTP/gRPC, auditoria ou produção ativos |
| Atualizar estes três artefatos e este skill | Alterar fonte, manifesto, configuração compartilhada ou consumer sem escopo próprio |

<a id="s02"></a>
## S02 — Contrato sob ownership

O root expõe sempre `cache` e `storage`, e reexporta `Region` e `TenantCtx`;
`auth`, `middleware` e `reapi` dependem de `tower-middleware`. A feature padrão
é vazia. O manifesto separa dependências sempre declaradas das opcionais da
feature e declara targets de teste/benchmark; tais declarações são composição,
não resultado de compilação ou execução.

<a id="s03"></a>
## S03 — Triagem obrigatória

| Se mudar | Preserve / investigue | Pare quando |
|---|---|---|
| Feature ou dependência opcional | Gate, lista de `dep:` e consumidores que a encaminham | Precisar resolver/buildar ou afirmar compatibilidade binária |
| Módulo/reexport público | Path raiz, gate e censo B02–B05 | Consumer externo ou semver publicado não puder ser limitado estaticamente |
| Nome de adapter/handler | Somente assinatura e composição fonte | A pergunta exigir request, storage, provider ou efeito real |
| Target de teste/bench | `required-features` declarado | Pedir execução, cobertura, tempo ou resultado de teste |

<a id="s04"></a>
## S04 — Método documental

1. Fixe manifesto, `lib.rs`, mapa de módulos e SHA em [R01](../../../docs/ownership/crates/corelink-worker/REFERENCE.md#r01).
2. Formule um predicado atômico que um diff de fonte possa refutar.
3. Classifique cada relação como declaração de manifesto, composição de módulo ou consumer estático.
4. Atualize impacto em [B01](../../../docs/ownership/crates/corelink-worker/BLAST_RADIUS.md#b01) antes de mudar path público/gate.
5. Registre modo, lacuna e rota OKF externa; não copie nem revalide OKF.
6. Rode apenas o checker documental e `git diff --check` autorizados em [M06](../../../docs/ownership/crates/corelink-worker/MAINTENANCE.md#m06).

<a id="s05"></a>
## S05 — Paradas obrigatórias

Pare para Cargo/build/teste, rede, benchmark, worker/cron, request HTTP/gRPC,
R2/KV/D1/DO/Neon, credencial, tenant/dado real, auditoria entregue, alerta,
deploy, produção, consumo externo, política operacional ou prova OKF. Encaminhe
ao owner do composition root, provider ou consumer com a lacuna explícita.

<a id="s06"></a>
## S06 — Saída mínima

Registre SHA, paths lidos, relações estáticas, predicados refutáveis, checker,
diff e desconhecidos. Não chame uma busca de censo completo, uma declaração de
manifesto de teste, nem um path `storage` de evidência runtime ou aprovação.

<a id="s07"></a>
## S07 — Evidência e encaminhamento

Mantenha separados manifesto, declaração de módulo, import/reexport e fato
operacional. Quando a fonte estática não responder, entregue o predicado que
faltou, a lacuna e o owner externo apropriado; não substitua essa escalada por
uma inferência ou por revalidação do processo OKF.

[Referência](../../../docs/ownership/crates/corelink-worker/REFERENCE.md#r01) · [Impactos](../../../docs/ownership/crates/corelink-worker/BLAST_RADIUS.md#b01) · [Manutenção](../../../docs/ownership/crates/corelink-worker/MAINTENANCE.md#m01)
