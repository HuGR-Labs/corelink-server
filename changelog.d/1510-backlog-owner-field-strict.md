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
  para 12**. Duas revisões frias refutaram sete rebaixamentos da primeira passada,
  cada um por credencial ou assinatura verificadamente ausente: `B-008` (não existe
  chave de LEITURA da PagerDuty — só a `PAGERDUTY_ROUTING_KEY` de escrita), `B-032`
  (nenhuma credencial do Drata, embora `corelink-ops/src/drata/drata.rs:132` faça
  `env::var` dela de verdade), `B-035` e `B-089` (emenda de instrumento assinado),
  `B-086` (emenda do adendo de residência), `B-097` (ticket na conta comercial) e
  `B-012` (o GitHub não expõe API para cunhar fine-grained PAT — é UI na conta dele).
  Os seis que permanecem `tl` de mérito tiveram a **prosa reconciliada**: deixar
  "Owner, não tl" de pé sob `owner: tl` apenas trocaria "o campo mente" por "a prosa
  mente", então cada um agora distingue a decisão a jusante, que é do owner, do
  próximo passo, que é medir e deixar pronto.
- **`scripts/backlog_verify.py` agora recusa `status: done|parked` combinado com
  `owner: owner`.** O critério acima era uma fotografia: valia no dia em que foi
  escrito e voltaria a quebrar no instante em que qualquer um dos itens fechasse sem
  que alguém lembrasse de tirar o campo à mão. O gate validava `owner ∈ {tl, owner}`
  e não tinha opinião nenhuma sobre a combinação com `status`. Agora tem, e o
  auto-teste `scripts/test_backlog_verify.sh` ganhou quatro células: as duas que
  provam o vermelho (`done`×`owner` e `parked`×`owner` viram BROKEN) e as duas de
  controle que impedem a regra de ser ampla demais (um item `open` bloqueado no dono
  continua válido; um `done` sob `tl` também).
- **`B-062` fechou** — o `GET` por `{id}` na API de Containers, nunca pela LISTA que
  serve visão defasada, mostra as cinco aplicações de produção convergidas em
  `ddd95560-r1`, cujo commit é ancestral de `main`, com os quatro commits que o item
  dava como presos fora (Art.17 do GDPR, leitura do CAS sem limite, assinatura do
  Turborepo, coleções sem limite) já em produção. Seu `verify` continua `manual` — um
  check de `wrangler.toml` vs `HEAD` seria portão dominado, pois mediria a
  configuração e não o que executa — reescrito com polaridade invertida. Registre-se
  que um `verify: manual` recém-datado é um cronômetro de 14 dias: o que sustenta
  este fechamento é a reverificação por região registrada no corpo, não o verde do
  gate.
