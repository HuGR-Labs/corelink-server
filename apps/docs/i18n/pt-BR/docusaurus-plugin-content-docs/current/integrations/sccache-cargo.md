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

O CoreLink é a camada **compartilhada** — a que permite que outra máquina, ou um runner de
CI recém-criado, reaproveite o que alguém já compilou. Ele foi feito para ficar *atrás* de
uma camada local em disco, não para substituí-la. A configuração abaixo monta as duas.

## Pré-requisitos

- `sccache` **0.15 ou mais novo** (`cargo install sccache` ou o pacote da sua
  distribuição). Confira com `sccache --version` — a cadeia de armazenamento multinível
  que lhe dá uma camada local chegou na 0.15.0.
- Um PAT do CoreLink (`corelink_pat_...`) com escopo de leitura + escrita de cache.
- O UUID do seu tenant.

## Configurar

Aponte o backend WebDAV do sccache para o caminho do seu tenant, passe o PAT como um token
bearer e encadeie uma camada local em disco na frente dele:

```bash
export SCCACHE_WEBDAV_ENDPOINT="https://corelink-api.humangr.com/cargo/<your-tenant-id>"
export SCCACHE_WEBDAV_TOKEN="corelink_pat_XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"
export SCCACHE_MULTILEVEL_CHAIN="disk,webdav"   # requer sccache >= 0.15
export SCCACHE_DIR="$HOME/.cache/sccache"       # onde fica a camada local
export RUSTC_WRAPPER=sccache
```

O que cada variável faz:

| Variável | Papel |
|---|---|
| `SCCACHE_WEBDAV_ENDPOINT` | O backend do CoreLink — a camada **compartilhada**, no caminho do seu tenant |
| `SCCACHE_WEBDAV_TOKEN` | O PAT que o sccache envia como `Authorization: Bearer` |
| `SCCACHE_MULTILEVEL_CHAIN` | A cadeia de armazenamento, da camada mais próxima para a mais distante. `disk,webdav` = disco local na frente do CoreLink |
| `SCCACHE_DIR` | Caminho no sistema de arquivos da camada local em disco. Só tem efeito quando a cadeia inclui `disk` |
| `RUSTC_WRAPPER` | Faz o cargo rotear cada invocação do `rustc` pelo sccache |

:::caution Não omita o `SCCACHE_MULTILEVEL_CHAIN`
O sccache seleciona **exatamente um** backend de armazenamento. O `storage_from_config`
dele só cai no backend de disco quando *nenhum* backend remoto está configurado — ou seja,
com o `SCCACHE_WEBDAV_ENDPOINT` definido e a cadeia não definida, o sccache fica
somente-remoto: toda leitura de cache é uma ida e volta HTTPS, e o `SCCACHE_DIR` é inerte.

Já o `SCCACHE_MULTILEVEL_CHAIN="disk,webdav"` monta uma cadeia de dois níveis — disco local
primeiro, CoreLink atrás — e um acerto encontrado no CoreLink é copiado para a camada local
em segundo plano. Cadeias multinível exigem o sccache **0.15.0 ou mais novo**; em um sccache
mais antigo a variável é ignorada e você volta silenciosamente ao modo somente-remoto, então
verifique o `sccache --version`.
:::

Depois, compile normalmente:

```bash
cargo build --release
```

Quando uma busca não encontra nada na camada local, o sccache emite requisições
`GET`, `PUT` e `HEAD` para
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

Com a cadeia configurada, um segundo build na mesma máquina normalmente é atendido pela
camada *local* — que é justamente o objetivo, mas isso não prova nada sobre o CoreLink.
Para confirmar que o próprio CoreLink está servindo, descarte a camada local entre os dois
builds:

```bash
cargo clean
cargo build --release        # frio — compila e grava nas duas camadas
cargo clean
sccache --stop-server        # pare o daemon antes de mexer na camada em disco dele
rm -rf "$SCCACHE_DIR"        # descarte a camada local para a leitura ter de chegar ao CoreLink
cargo build --release        # quente — o acerto só pode vir do CoreLink
sccache --show-stats
```

O `sccache --show-stats` reporta as contagens de acertos de cache e o armazenamento
configurado. "Cache hits" diferente de zero no segundo build, com a camada local esvaziada,
confirma que o CoreLink serviu os artefatos. Faça o `rm -rf` apenas para esta verificação —
no uso normal você quer que a camada local persista.

## Exemplo de CI (GitHub Actions)

```yaml
- name: Build with sccache + CoreLink
  env:
    RUSTC_WRAPPER: sccache
    SCCACHE_WEBDAV_ENDPOINT: https://corelink-api.humangr.com/cargo/acme-prod
    SCCACHE_WEBDAV_TOKEN: ${{ secrets.CORELINK_PAT }}
    SCCACHE_MULTILEVEL_CHAIN: disk,webdav
    SCCACHE_DIR: ${{ runner.temp }}/sccache
  run: cargo build --release
```

Armazene o PAT como um secret de repositório (**Settings → Secrets and variables →
Actions**).

Em um runner efêmero a camada local começa vazia e é descartada quando o job termina, então
ela só ajuda *dentro* de um mesmo job — o que ainda faz diferença em um job que roda várias
invocações do cargo. Mantenha a cadeia configurada de qualquer forma; você também pode
persistir o `SCCACHE_DIR` entre execuções com a action de cache do seu runner.

## Resolução de problemas

| Sintoma | Causa provável | Correção |
|---|---|---|
| Todo build recompila | `RUSTC_WRAPPER` não definido | Exporte `RUSTC_WRAPPER=sccache` no mesmo shell |
| `401 Unauthorized` nos logs do sccache | `SCCACHE_WEBDAV_TOKEN` ausente ou incorreto | Defina-o como seu PAT `corelink_pat_...` |
| `403 Forbidden` | PAT com escopo para um tenant diferente | Confirme que o `<tenant>` no endpoint corresponde ao tenant do seu PAT |
| Falhas de cache persistem | Entradas de build não determinísticas | Fixe o toolchain + `CARGO_INCREMENTAL=0`; execute `sccache --show-stats` para inspecionar |
| Os acertos são contabilizados, mas cada um é uma requisição de rede | `SCCACHE_MULTILEVEL_CHAIN` não definido — o sccache está somente-remoto | Defina `SCCACHE_MULTILEVEL_CHAIN="disk,webdav"` **e** o `SCCACHE_DIR` |
| O `SCCACHE_DIR` parece ser ignorado | Mesma causa, ou um sccache anterior à 0.15 (a variável da cadeia é ignorada) | Defina a cadeia; confira o `sccache --version` e atualize para ≥ 0.15 |

Referência completa de erros: [Resolução de problemas](../troubleshooting.md).
