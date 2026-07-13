---
id: cas-vs-ac
title: Armazenamento endereçável por conteúdo e Action Cache
sidebar_position: 1
description: Como CAS e AC diferem, quando cada um é usado e como o Bazel usa os dois juntos.
---

<!-- i18n:MT (pt-BR) — TMX-seeded machine-translation stub; replace with native-speaker translation before GA -->

> MT: Esta página está em tradução. A versão canônica em inglês é a fonte de verdade até a revisão por falante nativo (D+10).
>
> Canonical EN source: `docs/concepts/cas-vs-ac.md`

# Armazenamento endereçável por conteúdo e Action Cache

O CoreLink expõe dois caches distintos que funcionam juntos. A maioria dos
usuários só pensa em um — o que armazena as saídas de build deles — mas vale os 5
minutos de leitura entender ambos.

## Armazenamento endereçável por conteúdo (CAS)

O CAS armazena blobs arbitrários indexados pelo seu digest SHA-256. A chave *é* o
digest: não é necessário nome de arquivo, tag de versão ou metadados separados.

```
key:   sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
value: <bytes>
```

Propriedades:

- **Imutável**: uma vez que um blob é armazenado em um determinado digest, o
  conteúdo naquele digest nunca muda.
- **Deduplicado**: se dois tenants (ou dois jobs de CI) enviam os mesmos bytes, a
  camada de armazenamento guarda uma cópia. Ambos os tenants pagam pelo acesso,
  não pelo armazenamento duplicado.
- **Verificável**: o cliente que calcula `sha256(downloaded_bytes)` sempre
  corresponderá à chave usada para recuperá-lo.

### CAS no contexto da REAPI

Na [Remote Execution API](https://github.com/bazelbuild/remote-apis), o CAS é
usado para:

- Conteúdo de arquivos de origem (entradas de ações)
- Arquivos objeto compilados e artefatos finais (saídas de ações)
- Mensagens proto `Directory` que descrevem a árvore de entrada

O Bazel envia as entradas ao CAS antes de despachar uma ação remota. O executor lê
as entradas do CAS, executa a ação e envia as saídas de volta ao CAS.

### Endpoints HTTP do CAS

```
PUT /v1/cas/<tenant_id>/<sha256>   body: raw bytes  → 201 + {"hash": "sha256:<digest>"}
GET /v1/cas/<tenant_id>/<sha256>                    → 200 + raw bytes
```

Consulte a [referência da API HTTP](../api/http.md) para todos os detalhes.

## Action Cache (AC)

O Action Cache mapeia um **action digest** para um **action result**. Um action
digest é o SHA-256 de um proto `Action` serializado — ele codifica o comando, a
árvore de entrada e as propriedades de plataforma de forma determinística. O
action result registra os digests de saída, o código de saída e a temporização.

```
key:   sha256(<Action proto>)
value: ActionResult { output_files: [...], exit_code: 0, ... }
```

Propriedades:

- **Evita trabalho redundante**: se `action_digest` estiver no AC, a ferramenta de
  build busca as saídas em cache no CAS e evita reexecutar a ação.
- **Com escopo de tenant**: entradas do AC de um tenant nunca são visíveis para
  outro.
- **Invalidado por qualquer mudança de entrada**: como a chave é o digest das
  entradas + comando, qualquer alteração em arquivos de origem, flags ou na
  toolchain produz uma chave diferente — o cache dá miss de forma limpa.

### Quando o Bazel usa o AC

```
build tool
  1. Compute action_digest = sha256(Action{command, inputs, platform})
  2. GET /ac/<tenant>/action_digest  → HIT: fetch outputs from CAS, done
                                     → MISS: run action locally (or remotely)
  3. On success: PUT /ac/<tenant>/action_digest  → store result
                 PUT /v1/cas/<tenant>/<output_hash> → store each output
```

### Diagrama: CAS + AC juntos

```
        ┌─────────────────────────────────────────────────┐
        │                   Bazel client                   │
        └───┬─────────────────────────────────┬───────────┘
            │ 1. check AC                      │ 3. PUT outputs to CAS
            ▼                                  ▼
     ┌─────────────┐                   ┌────────────────┐
     │ Action Cache│   2. AC miss →    │ Build executor │
     │  (AC)       │   run action      │ (local or RE)  │
     └─────────────┘                   └────────────────┘
            │ 4. write result back              │
            └──────────────────────────────────►│
                                                │ 5. read outputs from CAS
                                                ▼
                                       ┌────────────────┐
                                       │      CAS        │
                                       └────────────────┘
```

## Comparação

| | CAS | Action Cache |
|---|---|---|
| Chave | SHA-256 do conteúdo | SHA-256 do proto Action |
| Valor | Bytes brutos | ActionResult (digests de saída, código de saída) |
| Imutável | Sim | Sim (entradas não são atualizadas, apenas gravadas uma vez) |
| Usado para | Blobs (arquivos, protos) | Memoização de ações de build |
| Disponível sem REAPI | Sim (API REST) | Somente via gRPC da REAPI |
| Turborepo | Não diretamente (o Turbo usa seu próprio formato de artefato) | O `TURBO_API` do Turborepo mapeia para este conceito |

## O que o Turborepo chama de "remote cache"

O Turborepo não expõe CAS/AC como conceitos distintos. Sua API de cache remoto é
um protocolo HTTP simplificado onde as saídas de tarefas são armazenadas por um
hash das entradas da tarefa. O CoreLink expõe um endpoint compatível com o
Turborepo — consulte [Integração com o Turborepo](../integrations/turborepo.md).
