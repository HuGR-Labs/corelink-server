### Fixed

- **O `verify` do `B-147` provava que o portão existe e nunca olhava para o arquivo que o
  portão guarda.** Ele montava sondas sintéticas num `mktemp -d` e perguntava se o *script*
  recusava `done`+`owner: owner` — **nunca abria o `BACKLOG.md` vivo**, e portanto era
  estruturalmente incapaz de contar violações. Foi assim que ficou verde enquanto **cinco**
  itens (`B-031`, `B-041`, `B-042`, `B-043`, `B-085`) as carregavam. O cruzamento que o #1523
  landou impede violação **nova**; nada olhava para as **existentes** — são coisas diferentes,
  e só uma estava consertada. O predicado agora exige **as duas**: as sondas **e** zero
  violações no arquivo real.
- **A contagem viva usa o parser do próprio `backlog_verify` (`bv.parse`), não um segundo
  lexer.** Dois parsers podem discordar sobre o que é um bloco; um só não pode. O escopo
  espelha exatamente o do portão (`done`, não `!= open`), senão a contagem vermelharia um
  `parked`+`owner` legítimo.
- **Anti-vacuidade em três cláusulas, porque "não achei violação" e "não sei ler o arquivo"
  são a mesma saída de um parser.** (1) Sonda não parseada ⇒ falha alta com a saída recortada.
  (2) O contador exige ver **≥ 100 itens** antes de acreditar em "zero" — é o que impede o
  portão de ficar verde por ter lido **zero** item. (3) Controle positivo nominal: o parser
  tem de achar o próprio `B-147`.
- **A exclusão deliberada de `parked` virou predicado.** A decisão do #1523 — `parked` **é** o
  estado que significa "esperando alguém de fora", então cruzá-lo com `owner:` puniria o uso
  correto — passa a ter uma quarta sonda (`parked`+`owner` ⇒ **passa**) que a pina. Quem
  quiser mudá-la tem de mudar a sonda, e aí é escolha, não deslize.
- **Dependências reduzidas a `sh` + `python3`.** A versão anterior usava `bash -c` com
  `mktemp`, `date`, `tr` e `cut`; esta não usa nenhum. O runner self-hosted já provou não ter
  `dig` (`B-041`), e `python3` é dependência dura do próprio `backlog_verify.py` — é o único
  interpretador garantido nos dois ambientes. Rodado sob `/bin/sh` e sob `dash`, não só sob o
  shell local.
