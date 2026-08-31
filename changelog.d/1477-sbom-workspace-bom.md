### Fixed

- **`sbom.yml` produz um SBOM de workspace de verdade — antes eram 16 execuções e
  zero sucessos por um `ls` que nunca achava o arquivo.** `cargo cyclonedx --all`
  emite um SBOM por pacote, cada um no diretório do próprio manifesto, e nunca um
  documento na raiz; com 70+ crates isso espalhava 70+ arquivos `sbom.cdx.json`
  por `crates/*/`. A geração passa a usar `scripts/sbom-aggregate.sh`, o caminho
  já comprovado pela lane irmã `sbom-consolidated.yml`.

  Três defeitos latentes atrás desse — nunca alcançados, portanto nunca
  observados — foram corrigidos junto: o job de validação baixava
  `cyclonedx-linux-x64` e o executava na frota macOS (agora selecionado por
  `uname -m`); o pin `CYCLONEDX_CLI_LINUX_SHA256` era o literal
  `"placeholder-pin-at-first-use"` e nenhum passo o lia (agora são digests reais,
  verificados antes do `chmod +x`); e `cargo-cyclonedx` estava em `0.5.4`, fora do
  pin canônico `0.5.9` do ADR-0044.

  A ingestão no Dependency-Track ficou condicional a `vars.DT_ENDPOINT`: o
  fallback embutido é NXDOMAIN, então sem a variável o job só podia avisar depois
  de compilar `sbom-publish` na máquina do owner. `sbom-release-upload` deixou de
  depender dele, senão o upload de assets seria pulado junto.

### Removed

- **O cron semanal da `sbom.yml`.** O SBOM deriva de `Cargo.lock` e dos
  manifestos, logo não pode driftar sem um commit — e a regra do repo é que cron
  só se justifica para o que muda sem commit. Restam `release: published` e
  `workflow_dispatch`.
