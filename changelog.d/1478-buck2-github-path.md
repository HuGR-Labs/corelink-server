### Fixed

- **`buck2-starter-ci.yml` passa do passo de instalação — 77 execuções e zero
  sucessos por uma linha.** O passo escrevia `$BUCK2_INSTALL_DIR` em
  `$GITHUB_PATH` e em seguida chamava `buck2 --version` no MESMO passo, mas
  `$GITHUB_PATH` só vale para os passos SEGUINTES. O resultado era
  `buck2: command not found` e exit 127, na última linha do próprio passo de
  instalação. Agora o binário é invocado por caminho absoluto ali; o
  `>> $GITHUB_PATH` continua servindo todos os passos posteriores.

  O cabeçalho do workflow registrava a causa como "não diagnosticada" e listava
  dois candidatos, ambos refutados pelo mesmo log: a frota `corelink` persiste
  `$GITHUB_PATH` normalmente, e o caminho apt/zstd não deu no-op (o log mostra
  `Setting up zstd` e a descompressão de 133 MB). O pin `BUCK2_VERSION` também
  está vivo — a tag resolve, não dá 404.
