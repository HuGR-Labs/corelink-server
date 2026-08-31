### Fixed

- **O comando de renumeração que o portão de merge imprime passou a cobrir os
  três arquivos onde um id vive, não só o `BACKLOG.md`.** O mesmo `B-NNN` é
  citado pelo dossiê do item em `reports/` e pelo seu fragmento em
  `changelog.d/`, e **nenhum portão confere que os três concordam** — então
  renumerar só o backlog deixava a documentação do próprio item apontando para o
  número antigo, em silêncio. Portão que dá instrução incompleta é pior que
  portão que não dá nenhuma: a incompleta parece autoritativa. Acrescentado
  também o aviso de nunca renumerar com rebase em andamento — a substituição
  passa por cima dos marcadores `<<<<<<<` e o estrago fica dentro do que parece
  um conserto.
