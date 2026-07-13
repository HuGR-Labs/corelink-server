---
id: intro
title: O que é o CoreLink?
sidebar_position: 1
description: O CoreLink é um cache multi-inquilino endereçável por conteúdo para artefatos de build, pacotes, camadas de contêiner e pesos de modelos de ML — hospedado na Cloudflare.
---

<!-- i18n:MT (pt-BR) — TMX-seeded machine-translation stub; replace with native-speaker translation before GA -->

> MT: Esta página está em tradução. A versão canônica em inglês é a fonte de verdade até a revisão por falante nativo (D+10).
>
> Canonical EN source: `docs/intro.md`

# O que é o CoreLink?

O CoreLink é um **cache endereçável por conteúdo** hospedado e multi-inquilino para artefatos de build. Ele armazena qualquer blob exatamente uma vez pelo seu digest SHA-256 e o entrega a partir da borda da Cloudflare mais próxima de cada cliente.

Ferramentas de build que oferecem suporte à [Remote Execution API (REAPI)](https://github.com/bazelbuild/remote-apis) — Bazel, Buck2, NativeLink e outras — podem apontar diretamente para o CoreLink sem nenhuma alteração de código. O Turborepo conecta-se por meio de uma única variável de ambiente. Clientes HTTP brutos usam os endpoints REST.

## Para quem é

- **Equipes que executam Bazel ou Buck2** e que querem um cache remoto gerenciado sem operar buckets S3, Redis ou o `bazel-remote` por conta própria.
- **Monorepos do Turborepo** que querem um cache remoto personalizado fora da oferta hospedada da Vercel.
- **Equipes de engenharia de plataforma** que querem isolamento de inquilinos, logs de auditoria e criptografia BYOK em um único serviço.

## O que o CoreLink não é

O CoreLink não é um mecanismo de execução remota. Ele armazena e recupera conteúdo por hash; ele não agenda nem executa ações de build. Use-o junto com o [BuildBarn](https://github.com/buildbarn/bb-storage) ou o [EngFlow](https://www.engflow.com) se você precisar de execução remota.

## Como funciona

```
build tool                CoreLink API (Cloudflare Worker)       R2 / KV
─────────────────────     ─────────────────────────────────     ─────────
PUT /v1/cas/<t>/<hash> ─► auth (PAT) → tenant isolation        → stored once
GET /v1/cas/<t>/<hash> ◄─ cache-hit lookup                     ← returned
```

Todo blob é endereçado pelo seu digest SHA-256. Se dois inquilinos enviarem os mesmos bytes, cada inquilino paga por uma cópia e tem controle de acesso independente — o conteúdo é compartilhado na camada de armazenamento, o acesso não é.

## Principais recursos

| Recurso | Detalhes |
|---|---|
| Armazenamento endereçável por conteúdo (CAS) | Armazenamento de blobs chaveado por SHA-256. Deduplica automaticamente. |
| Action cache (AC) | Mapeia `(action_digest) → (output_digest)` para que o Bazel pule ações idênticas. |
| Multi-inquilino | Cada inquilino é isolado no nível do PAT. Leituras entre inquilinos nunca são possíveis. |
| Criptografia BYOK | Inquilinos no plano Enterprise podem fornecer sua própria chave AES-256. |
| Log de auditoria | Toda leitura e escrita é anexada a um log imutável e restrito ao inquilino. |
| REAPI v2 | Serviços gRPC completos `ContentAddressableStorage` + `ActionCache` + `ByteStream`. |

## Próximo passo

O caminho mais rápido para o seu primeiro cache hit é o [quickstart de 5 minutos](./quickstart.md).
