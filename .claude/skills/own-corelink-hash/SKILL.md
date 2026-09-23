---
name: own-corelink-hash
description: >-
  Assuma ownership de corelink-hash ao alterar Digest, VerifiedBody, BlobStoreWrite,
  parsing hexadecimal, comparação de digests, erros ou CACHE_ENTRY_MAX_BYTES.
  Use para diagnosticar incompatibilidade nesses contratos e seus consumidores.
  Não use como owner de autenticação, cobrança, deploy ou implementação criptográfica externa.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-hash"
  manifest: "crates/corelink-hash/Cargo.toml"
  source-commit: "cacc44fc43ee2f481e933419ba88b9a19ac6c8e8"
  evidence-set: "hash-pilot-source-20260919"
---

# Ownership — corelink-hash

Candidata de autoria; não aprovada. O pin atual foi reconciliado estaticamente; comandos Rust, resolução Cargo e cold review ainda não foram executados nesta edição.

[Acionamento](#s01) · [Autoridade](#s02) · [Leitura](#s03) ·
[Decisões](#s04) · [Fluxo](#s05) · [Paradas](#s06) · [Saída](#s07).

<a id="s01"></a>
## S01 — Acionamento

| Use quando | Não use como owner principal quando |
|---|---|
| Alterar representação, parser ou formatação de Digest | Alterar cobrança: encaminhar ao domínio billing |
| Alterar VerifiedBody::new ou BlobStoreWrite | Alterar resolução de tenant: auth/tenant-path |
| Alterar CACHE_ENTRY_MAX_BYTES ou quebrar um consumidor desses símbolos | Operar R2/deploy: encaminhar ao composition root |
| Alterar/remover `Digest::as_bytes` | Tratar como superfície pública potencial; `#[doc(hidden)]` não significa privado nem sem uso. Mapear consumers e coordenar com o owner do formato persistido |

<a id="s02"></a>
## S02 — Território e autoridade

**Implementação própria:** `src/{digest,error,store,verified_body}.rs` e exports de `src/lib.rs`.  

**Dono da implementação:** mantenedores de `corelink-hash` (identidade nominal ainda não verificada). **Dono do contrato público:** esta unidade para API-001–016; consumidores participam de mudanças incompatíveis. **Composition root:** `corelink-server` (`crates/corelink-container/Cargo.toml`) e adapters consumidores. **Operador/runtime:** não identificado nesta evidência. **Autoridade de aprovação:** mantenedores/CODEOWNERS do repositório, ainda não verificados.

**Responsabilidade:** contratos API-001–016 da referência; não implementa storage, autorização ou emissão de auditoria.  

**Escalonamento:** issue/PR em `HuGR-Labs/corelink-server`; atribuição nominal, operador e autoridade de aprovação ainda não confirmados.
**Limite de autoria:** consultar [OKF CAS/AC](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/docs/knowledge/crates/cas-ac-core.md) antes de alterar contratos; não converter comentários de intenção em prova de deployment.
Esta skill não concede autorização de publicação de crate, operação de dados, gastos ou deploy.

<a id="s03"></a>
## S03 — Roteamento de leitura

| Pergunta | Abrir |
|---|---|
| Parsear um digest prova o conteúdo? | [API-002](../../../docs/ownership/crates/corelink-hash/REFERENCE.md#api-002) |
| Por que as_bytes afeta persistência? | [REL-015](../../../docs/ownership/crates/corelink-hash/BLAST_RADIUS.md#rel-015) |
| O que muda com o limite de corpo? | [REL-016](../../../docs/ownership/crates/corelink-hash/BLAST_RADIUS.md#rel-016) |
| Quais testes rodar? | [Matriz](../../../docs/ownership/crates/corelink-hash/MAINTENANCE.md#m04) |
| O mapa de impacto está completo? | [Cobertura](../../../docs/ownership/crates/corelink-hash/BLAST_RADIUS.md#b06) |
| Como funciona a implementação? | [Mapa](../../../docs/ownership/crates/corelink-hash/REFERENCE.md#r03) |
| Como carregar o contexto OKF? | [`okf-context`](../okf-context/SKILL.md) e conceito [CAS/AC](../../../docs/knowledge/crates/cas-ac-core.md) |
| Como separar compilado de alcançável? | [`built-not-wired`](../built-not-wired/SKILL.md) |
| Como coordenar consumers e FFI? | [Relações](../../../docs/ownership/crates/corelink-hash/BLAST_RADIUS.md#b03) e [client-verify](../own-corelink-client-verify/SKILL.md), [crypto](../own-corelink-crypto/SKILL.md), [server/container](../own-corelink-server/SKILL.md) |
| Onde estão os demais consumers? | [AC](../own-corelink-ac/SKILL.md), [Bazel](../own-corelink-bazel-bridge/SKILL.md), [CAS](../own-corelink-cas/SKILL.md), [meta](../own-corelink-meta/SKILL.md), [REAPI](../own-corelink-reapi/SKILL.md), [worker](../own-corelink-worker/SKILL.md), [fuzz](../own-corelink-hash-fuzz/SKILL.md), [signup](../own-e2e-signup-flow/SKILL.md) |

<a id="s04"></a>
## S04 — Decisões e invariantes

| Condição | Ação e evidência exigida | Parada |
|---|---|---|
| Alterar BLAKE3, largura ou hex | API-001/003; REL-014/015; testar leitura de dados anteriores | Migração/compatibilidade não definida |
| Alterar construção do envelope | API-005; teste positivo, mismatch e redaction | Novo caminho evita verificação |
| Alterar as_bytes | Separar o contrato dos bytes do tipo (`corelink-hash`) do formato do envelope (`corelink-server`/adapter); exigir validação e coordenação de ambos | Atribuir ao hash ownership do envelope ou declarar um aprovador sem evidência |
| Alterar limite HTTP | API-009; enumerar routers e testar abaixo/no/acima do limite | Supor que VerifiedBody impõe limite |
| Um teste de tempo falhar | Preservar log, build e carga; diagnóstico isolado | Reduzir rigor ou repetir até obter verde |
| Pedir prova produtiva | Traçar artefato/entrypoint/adapter real | Dispor apenas de fake, fonte ou grafo Cargo |

<a id="s05"></a>
## S05 — Fluxo de trabalho

1. Verifique checkout isolado, manifesto e baseline pelo PROC-001.
2. Abra o conceito OKF e somente API/INV/REL pertinentes à tarefa.
3. Confira aliases, consumidores e lacunas em B06; escolha o PROC adequado.
4. Registre mudança, estado afetado e recuperação antes de editar.
5. Execute gates no ambiente declarado; registre falhas e testes ignorados.
6. Atualize artefatos afetados e solicite cold review dos hashes finais.

<a id="s06"></a>
## S06 — Condições de parada

Pare se a baseline não confere, Cargo pede alteração do lockfile, o target não existe,
a fronteira de storage é desconhecida ou uma alteração invalida dados antigos.
Não substitua revisão de consumidores por um teste apenas desta library.
Não opere serviços nem publique a crate para provar documentação.
Solicite decisão de contrato ao mantenedor pela issue/PR; mantenha o bloqueio explícito.

<a id="s07"></a>
## S07 — Evidência e saída

Entregue baseline, objetivo, APIs/INVs/RELs/PROCs alterados, comandos com exit status,
ambiente/target, resultado observado, recuperação e hashes finais.
Registre `não executado` quando aplicável; revisão do próprio autor não é cold review.
Não promova esta candidata nem os procedimentos a aprovados por passar em checks editoriais.

[Voltar ao início](#s01)
