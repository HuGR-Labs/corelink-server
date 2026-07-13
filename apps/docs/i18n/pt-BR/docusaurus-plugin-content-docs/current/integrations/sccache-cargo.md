---
id: sccache-cargo
title: Integração com sccache (Rust / cargo)
sidebar_position: 4
description: Use o CoreLink como um cache de build WebDAV do sccache para que builds do cargo compartilhem artefatos compilados entre máquinas e na CI.
---

<!-- i18n:MT (pt-BR) — TMX-seeded machine-translation stub; replace with native-speaker translation before GA -->

> MT: Esta página está em tradução. A versão canônica em inglês é a fonte de verdade até a revisão por falante nativo (D+10).
>
> Canonical EN source: `docs/integrations/sccache-cargo.md`

# Integração com sccache (Rust / cargo)

O [sccache](https://github.com/mozilla/sccache) é um cache de compilador. Quando você define
`RUSTC_WRAPPER=sccache`, cada invocação do `rustc` é cacheada — de modo que um `cargo build`
reutiliza artefatos compilados produzidos em outra máquina ou em uma execução de CI anterior.

O CoreLink expõe um backend de armazenamento **WebDAV** do sccache em
`/cargo/<tenant>`, apoiado pelo mesmo armazenamento endereçável por conteúdo por tenant que o
CAS nativo usa. Tanto o sccache quanto o CoreLink chaveiam artefatos por **BLAKE3**, então
não há tradução de digest.

## Pré-requisitos

- `sccache` instalado (`cargo install sccache` ou o pacote da sua distribuição).
- Um PAT do CoreLink (`corelink_pat_...`) com escopo de leitura + escrita de cache.
- O UUID do seu tenant.

## Configurar

Aponte o backend WebDAV do sccache para o caminho do seu tenant e passe o PAT como um token
bearer:

```bash
export SCCACHE_WEBDAV_ENDPOINT="https://corelink-api.humangr.com/cargo/<your-tenant-id>"
export SCCACHE_WEBDAV_TOKEN="corelink_pat_XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"
export RUSTC_WRAPPER=sccache
```

Depois, compile normalmente:

```bash
cargo build --release
```

O sccache emite requisições `GET`, `PUT` e `HEAD` para
`<SCCACHE_WEBDAV_ENDPOINT>/<key>`; o CoreLink autentica o PAT bearer,
resolve o seu tenant a partir dele e serve ou armazena cada artefato no CAS do seu
tenant. Uma falha de `GET`/`HEAD` retorna 404 e o sccache recorre à compilação
local (e então faz `PUT` do resultado).

:::note O tenant vem do PAT
O `<tenant>` no endpoint é usado apenas para o roteamento de requisições; o
tenant autoritativo é resolvido a partir do PAT e reverificado no servidor. Um PAT
só pode ler e escrever no cache do seu próprio tenant.
:::

## Verificar se funcionou

Execute um build duas vezes (limpe o sccache local primeiro para que o segundo build precise acessar
o CoreLink):

```bash
cargo clean
cargo build --release        # cold — compiles and PUTs artifacts
cargo clean
cargo build --release        # warm — should read from CoreLink
sccache --show-stats
```

O `sccache --show-stats` reporta a taxa de acertos de cache e a URL do backend WebDAV.
"Cache hits" diferente de zero no segundo build confirma que o CoreLink serviu os
artefatos.

## Exemplo de CI (GitHub Actions)

```yaml
- name: Build with sccache + CoreLink
  env:
    RUSTC_WRAPPER: sccache
    SCCACHE_WEBDAV_ENDPOINT: https://corelink-api.humangr.com/cargo/acme-prod
    SCCACHE_WEBDAV_TOKEN: ${{ secrets.CORELINK_PAT }}
  run: cargo build --release
```

Armazene o PAT como um secret de repositório (**Settings → Secrets and variables →
Actions**).

## Resolução de problemas

| Sintoma | Causa provável | Correção |
|---|---|---|
| Todo build recompila | `RUSTC_WRAPPER` não definido | Exporte `RUSTC_WRAPPER=sccache` no mesmo shell |
| `401 Unauthorized` nos logs do sccache | `SCCACHE_WEBDAV_TOKEN` ausente ou incorreto | Defina-o como seu PAT `corelink_pat_...` |
| `403 Forbidden` | PAT com escopo para um tenant diferente | Confirme que o `<tenant>` no endpoint corresponde ao tenant do seu PAT |
| Falhas de cache persistem | Entradas de build não determinísticas | Fixe o toolchain + `CARGO_INCREMENTAL=0`; execute `sccache --show-stats` para inspecionar |

Referência completa de erros: [Resolução de problemas](../troubleshooting.md).
