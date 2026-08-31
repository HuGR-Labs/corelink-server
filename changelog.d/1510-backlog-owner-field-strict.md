### Fixed

- **O campo `owner:` do `BACKLOG.md` marcava "cheiro de decisão" em vez de bloqueio
  real, e com isso tirou itens de engenharia da fila de trabalho.** O campo carregava
  dois significados ao mesmo tempo — "está bloqueado no dono agora" e "foi decisão
  dele um dia" — e um campo com dois significados não decide nada quando alguém o lê.
  A consequência não foi cosmética: itens que eram trabalho de engenharia comum
  ficaram parados esperando uma pessoa que não sabia que estava sendo esperada. O
  critério agora é único e é o presente: `owner: owner` só quando o próximo passo é
  literalmente impossível sem o dono — a credencial dele, o dinheiro dele, a
  assinatura dele, a máquina dele; investigar, medir e deixar o conserto pronto nunca
  conta, mesmo que a última tecla seja dele. As linhas `owner: owner` caíram de **28
  para 9**, e nenhum item `done` carrega mais o campo: ele agora é estritamente
  presente **e** exclusivo de itens abertos. Uma revisão fria refutou quatro
  rebaixamentos da primeira passada e eles foram revertidos, cada um por credencial
  ou assinatura ausente: `B-008` (não existe chave de LEITURA da PagerDuty — só a
  `PAGERDUTY_ROUTING_KEY` de escrita; a `PAGERDUTY_API_KEY` do doc-comment nunca foi
  provisionada), `B-032` (nenhuma credencial do Drata em lugar nenhum), `B-035`
  (emenda de instrumento assinado que o próprio item diz não poder ser auto-aprovada)
  e `B-089` (emenda de SLA, ou comprometer-se a devolver dinheiro contra fatura).
  Sobre os `done`: `B-031`, `B-041` e `B-042` registram no corpo uma decisão datada do
  owner, então a procedência não se perde ao mover o campo; `B-043` **não registrava
  nenhuma** — apurado contra o brief de decisões e por varredura do item — e isso foi
  escrito no corpo em vez de se inventar uma decisão que não houve. `B-046` mantém o
  campo, com o corpo reconciliado para dizer que o bloqueio é da **Cloudflare** (R2
  responde `NotImplemented` a Object Lock) e que o único resíduo de owner é pagar por
  um segundo backend. No mesmo passo, `B-062` fechou: o `GET` por `{id}` na API de
  Containers — nunca pela LISTA, que serve visão defasada — mostra as cinco
  aplicações de produção convergidas em `ddd95560-r1`, cujo commit é ancestral de
  `main`, com os quatro commits que o item dava como presos fora (Art.17 do GDPR,
  leitura do CAS sem limite, assinatura do Turborepo, coleções sem limite) já em
  produção; seu `verify` continua `manual` — um check de `wrangler.toml` vs `HEAD`
  seria portão dominado, pois mediria a configuração e não o que executa — e foi
  reescrito com polaridade invertida, como `done` exige.
