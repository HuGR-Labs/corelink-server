### Fixed

- **O diagnóstico de citações abreviadas do OKF agora tem contrato executado no
  gate oficial (B-059).** `okf_wiki.yml` executa a suíte stdlib do resolvedor,
  separada do diagnóstico que permanece `warning` enquanto os 38 achados
  existentes são reescritos. A suíte cobre herança de âncora completa e
  path-only, inclusive fonte na raiz como `README.md`, limites/ranges e
  traversal. A âncora path-only usa a mesma gramática do `CITE_RE`, portanto
  inclui `.gitignore` e `.github/workflows/*.yml`, mas só herda quando está em
  `source_files` do conceito e abre como arquivo do repositório: identificadores
  pontilhados como eventos não podem mais virar contexto. `SourceCache` continua
  recusando qualquer caminho que escape a raiz. O `verify` open-polarity de B-059 agora falha fechado se `CITE_RE` sumir
  e prova por mutação que removê-lo torna o validador vermelho. O item continua
  aberto: isto endurece a medição e a integração, não transforma os 38 de 107
  resíduos em citações corretas.
