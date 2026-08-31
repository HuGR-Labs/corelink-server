### Fixed

- **`B-147` fechado: `status ∈ {done, parked}` com `owner: owner` agora é `BROKEN`.**
  `validate_schema()` validava `status` e `owner` **independentemente** e nunca cruzava os
  dois, então um item fechado podia continuar declarando que estava bloqueado no owner. Não
  era hipótese: **cinco** itens (`B-031`, `B-041`, `B-042`, `B-043`, `B-085`) carregaram a
  contradição com o `backlog_verify` **verde o tempo todo**. São 14 linhas em
  `scripts/backlog_verify.py` e 4 células em `scripts/test_backlog_verify.sh` — duas de
  **dente** e duas de **não-sobre-alcance** (`open`+`owner` e `done`+`tl` continuam
  `CONFIRMED`), porque sem os controles negativos um portão que reprovasse **tudo** passaria
  nas células de dente e pareceria provado. A pergunta que o item deixava explicitamente em
  aberto — se `parked` conta junto com `done` — ficou decidida pela lista explícita
  `{done, parked}` em vez de `!= open`, e o teste fixa a escolha.
- **O `verify` do próprio `B-147` era o segundo espécime do defeito que ele descrevia.** Ele
  montava uma sonda sintética num `mktemp -d` e perguntava se o *script* recusava a
  combinação — **nunca abria o `BACKLOG.md` vivo**, e portanto era estruturalmente incapaz de
  contar violações. Foi assim que ficou verde durante os cinco. O `verify` de fechamento tem
  **polaridade invertida** e mede os dois: exige que as sondas `done`/`parked` × `owner` saiam
  `BROKEN` **e** que os controles `open`×`owner` e `done`×`tl` saiam `CONFIRMED`, **e** conta
  as violações vivas no arquivo real, recusando-se a acreditar em "zero" antes de ver ≥ 100
  itens parseados.
