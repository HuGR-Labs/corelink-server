### Fixed

- **O diagnóstico de citações abreviadas do OKF agora tem contrato executado no
  gate oficial (B-059).** `okf_wiki.yml` executa a suíte stdlib do resolvedor,
  separada do diagnóstico. A suíte cobre herança de âncora completa e
  path-only, inclusive fonte na raiz como `README.md`, limites/ranges e
  traversal. A âncora path-only usa a mesma gramática do `CITE_RE`, portanto
  inclui `.gitignore` e `.github/workflows/*.yml`, mas só herda quando está em
  `source_files` do conceito e abre como arquivo do repositório: identificadores
  pontilhados como eventos não podem mais virar contexto. `SourceCache` continua
  recusando qualquer caminho que escape a raiz. O `verify` open-polarity de B-059 agora falha fechado se `CITE_RE` sumir
  e prova por mutação que removê-lo torna o coletor vermelho. O item está
  fechado: o resolvedor encontra zero resíduos nas 105 citações abreviadas e o
  `validate_okf.py` completo passa. Números de linha com milhares de dígitos agora
  viram `malformed-range` (sem `ValueError`/traceback), e o `verify` instala o
  cleanup imediatamente após o primeiro `mktemp`, inclusive se o segundo falhar.
  A reancoragem posterior conferiu os 105 atalhos restantes e o resolvedor agora
  sai `0` (`OK`, zero resíduos); a mutação focal da remoção de `CITE_RE` falha no
  coletor, sem duplicar a execução do gate completo.
