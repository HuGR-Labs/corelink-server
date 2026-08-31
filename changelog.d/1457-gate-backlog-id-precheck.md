### Fixed

- **`pre-merge-gate-check.sh` recusa o merge ANTES de uma colisão de id no
  `BACKLOG.md`, com o comando de renumeração pronto.** `backlog_verify.py`
  exige ids **densos**, e essa regra sozinha torna a colisão entre PRs
  concorrentes inevitável — de um jeito que pune quem tenta ser cuidadoso: a
  sessão que deixa lacuna para outra é reprovada pelo gate *por tentar*, porque
  a lacuna é ela mesma violação de densidade. Marcador também não resolve
  (`B-NNN` não é id válido, o item nasce `BROKEN`, e o PR fica vermelho antes de
  poder ser gateado). Sem este precheck, a colisão é descoberta pelo **segundo**
  merge — quando a `main` já está vermelha e todo PR aberto herda a falha; foram
  cinco ocorrências em um dia. A ordem dos renames emitidos é derivada da
  direção (subir é seguro em ordem decrescente, descer em ordem crescente), e o
  comando carrega o limite de token `(?![0-9])` mais o aviso sobre o limite de
  região. O bloco não reescreve o branch e, se não conseguir medir, avisa em
  `stderr` e segue — é uma cortesia, não o portão de verdade.
