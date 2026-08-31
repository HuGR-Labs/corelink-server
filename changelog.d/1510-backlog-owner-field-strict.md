### Fixed

- **O campo `owner:` do `BACKLOG.md` marcava "cheiro de decisão" em vez de bloqueio
  real, e com isso tirou 23 itens da fila de trabalho.** O campo carregava dois
  significados ao mesmo tempo — "está bloqueado no dono agora" e "foi decisão dele um
  dia" — e um campo com dois significados não decide nada quando alguém o lê. A
  consequência não foi cosmética: 19 itens abertos, todos trabalho de engenharia
  comum, ficaram parados esperando uma pessoa que não sabia que estava sendo
  esperada. O critério agora é único e é o presente: `owner: owner` só quando o
  próximo passo é literalmente impossível sem o dono — a credencial dele, o dinheiro
  dele, a assinatura dele, a máquina dele; investigar, medir e deixar o conserto
  pronto nunca conta, mesmo que a última tecla seja dele. As linhas `owner: owner`
  caíram de **28 para 5** (`B-013`, `B-111`, `B-065`, `B-110`, `B-046`). Quatro dos
  23 que saíram já estavam `done` (`B-031`, `B-041`, `B-042`, `B-043`) e entraram por
  ambiguidade de campo, não por bloqueio: neles `owner:` era procedência histórica, e
  essa procedência já está escrita no corpo de cada item, com data e com a fala do
  owner. `B-046` mantém o campo mas ganhou anotação explícita de que o bloqueio é da
  **Cloudflare** (R2 responde `NotImplemented` a Object Lock), não uma decisão
  pendente na mesa dele. No mesmo passo, `B-062` fechou: o `GET` por `{id}` na API de
  Containers — nunca pela LISTA, que serve visão defasada — mostra as cinco
  aplicações de produção convergidas em `ddd95560-r1`, cujo commit é ancestral de
  `main`, com os quatro commits que o item dava como presos fora (Art.17 do GDPR,
  leitura do CAS sem limite, assinatura do Turborepo, coleções sem limite) já em
  produção; seu `verify` continua `manual` — um check de `wrangler.toml` vs `HEAD`
  seria portão dominado, pois mediria a configuração e não o que executa — e foi
  reescrito com polaridade invertida, como `done` exige.
