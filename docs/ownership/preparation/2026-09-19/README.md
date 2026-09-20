# Ownership — preparação executada do censo Cargo

**Estado:** candidato de preparação; não aprova o padrão nem libera issues por crate.
**Fonte:** `HuGR-Labs/corelink-server@cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.
**Destino:** trabalho relacionado ao épico de documentação #1699.

## Navegação

[105 pacotes de entrada](index.md) · [Censo](census.json) ·
[Fontes por package](source-packets.json) · [Seeds](seeds.json) ·
[Resumo medido](summary.json) · [Comandos executados](metadata-commands.json) ·
[Classificação dos manifestos](tracked-manifests.json) ·
[Deduplicação](deduplication.json) · [Checks locais](verification-tests.json).

## Resultado desta captura

| População | Quantidade | Natureza da prova |
|---|---:|---|
| Packages do workspace principal | 95 | Cargo metadata executado |
| Packages independentes de fuzz | 10 | Dez invocações próprias de Cargo metadata |
| Packages ativos elegíveis | 105 | União conciliada, sem duplicatas |
| Targets Cargo | 631 | 608 principais e 23 de fuzz |
| Manifestos rastreados | 107 | 105 packages, raiz virtual e arquivo histórico |
| Relações internas declaradas | 235 | 222 principais; não são chamadas de runtime |
| Arquivos Rust próprios | 2.111 | Arquivos rastreados; raízes aninhadas excluídas |
| Packages com fonte declarada no OKF | 60 | Correspondência com `source_files` |

Os 45 sem correspondência direta não são declarados sem documentação: a busca
por conceito e contexto herdado continua necessária. As contagens pertencem a
esta baseline, não são constantes a copiar em skills ou em políticas do produto.

## Exclusões justificadas

`Cargo.toml` é a raiz virtual, sem `[package]`. Não recebe issue própria.
`_archive/wi-s11-002-partial/Cargo.toml` é a implementação parcial histórica
`corelink-erasure`, não consumida por qualquer dos 105 packages ativos no censo.
A substituição é documentada em `specs/04_sprints/S11/_spec_contract.md:485`:
os sucessores são `corelink-privacy-erasure-worker` e `corelink-privacy-pseudonymize`.
A exclusão não transforma código arquivado em código apagado ou sem valor histórico.

## O que os pacotes de entrada entregam

Cada Markdown em `packets/` identifica manifesto, entradas, targets, consumidores
Cargo declarados, conceitos OKF encontrados, pontos concretos para investigação,
comandos com estado de execução e links imutáveis para fontes. São preparação
para autoria; não substituem skill, referência, blast radius e manutenção finais.
O inventário completo de targets e relações está em `census.json`, identificado
por `manifest`; o Markdown não corta a população sem apontar onde ela está.

`source-packets.json` contém nomes de declarações localizados por regex nos
entrypoints. São candidatos de navegação, não uma AST, inventário completo de API
ou prova de uso em produção. `specific_risks` nos seeds são perguntas de investigação,
não bugs confirmados. O comando Cargo tree Linux é explicitamente não executado
e serve à análise: não afirma que essa seleção é o artefato implantado da crate.

## Evidência e segurança da execução

Onze invocações `cargo metadata --locked --offline --no-deps --format-version=1`
passaram. A cópia de fonte estava limpa antes e depois, no mesmo commit; Cargo.lock
permaneceu byte-idêntico. Não houve compilação Rust, teste do produto, rede de
produção, leitura de segredos, modificação do checkout original ou implantação.

## Reproduzir e interpretar os checks

Em checkout limpo na revisão de fonte, com o toolchain do projeto no PATH:

```sh
cargo metadata --locked --offline --no-deps --format-version=1
cargo metadata --locked --offline --no-deps --format-version=1 \
  --manifest-path tools/cli/fuzz/Cargo.toml
```

`metadata-commands.json` registra as onze seleções exatas. O resultado não contém
um `resolve` completo: inversas são reconstruídas das declarações de path, com
kind/cfg/optional/aliases preservados. A união de workspaces não é um único build.

A partir da raiz do checkout do PR, o verificador desta captura compara populações, inversas, contagens, estados e
atribuição de comandos executados. Não aprova semântica, autonomia ou produção:

```sh
python3 docs/ownership/preparation/2026-09-19/verify_preparation.py --self-test
```

Resultado obtido: um controle positivo e onze entradas alteradas corretamente
rejeitadas. Isso não é reexecução da suíte de 126 testes do pacote candidato v1.2.

## Duplicatas e integração

A coleta paginou todas as 194 issues abertas/fechadas presentes na captura. A
busca nos títulos/corpos não encontrou uma campanha equivalente de ownership.
#1699 é o épico relacionado; #1702 trata de migração de organização e não é duplicata.
O BACKLOG inteiro foi varrido como texto; a única ocorrência de blast radius é
sobre um teste de cobrança, não esta campanha. Isso não afirma leitura semântica
de cada linha histórica. Rechecar nomes, aliases e marcadores imediatamente antes
da emissão: um snapshot não bloqueia criações concorrentes.

## Revisão independente

[Cold review estrutural — PASS](census-cold-review.md) ·
[Registro da execução](census-cold-review-execution.json).

Um contexto novo, read-only, refez as onze invocações Cargo e comparou todas as
identidades, targets, features, dependências e registros inversos. A aprovação
está vinculada aos hashes de `census.json` e `summary.json`. Não cobre os seeds,
a semântica dos source packets, o padrão CO-1 ou os quatro artefatos finais.
Uma tentativa anterior terminou por timeout, sem veredito; não recebeu aprovação.

## Fronteira de entrega

Esta captura encerra a incerteza estrutural de população na baseline indicada.
Não presume que o candidato documental v1.2 já está congelado ou incorporado ao repo.
O pacote de referência usado é `corelink-ownership-v1.2.zip`, SHA-256
`55fe561159735b58bcb9e1226d11872d58fd9e29629133a5f85021e24e8d595a`.

Antes de emitir as issues por crate, continuam necessários os cinco pilotos
completos, a calibração dos tetos, a revisão independente do padrão e a publicação
de sua versão imutável. A revisão semântica dos seeds e a conciliação de aliases
precisam ser registradas. O estudo profundo das crates será o trabalho das issues,
não uma exigência para escrever antecipadamente 420 documentos.

A integração deve também conciliar o candidato anterior de ownership encontrado
em uma branch local não publicada, preservada sem alterações. Não se instalaram
políticas concorrentes. O backlog e os gates de integração existentes precisam
ser reconciliados antes do merge; este material é apresentado em PR de rascunho.
