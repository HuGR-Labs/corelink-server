### Fixed

- **O `verify` do B-041 decidia pela ausência do `dig`, não pela do DNS.** O
  predicado era `[ -z "$(dig +short <host>)" ] && curl …`: com `dig` ausente a
  substituição sai vazia, `-z` é verdadeiro, e o portão passa **sem ter
  resolvido nada**. Não é hipotético — o runner da frota `corelink` não tem
  `dig` e o Mac tem, então o mesmo portão decidia coisas diferentes conforme o
  runner que o pegasse, e a resposta em CI era o falso verde. Trocado por
  `python3` + `socket.getaddrinfo` (dependência dura do próprio
  `backlog_verify.py`, logo garantida nos dois ambientes), com controle
  positivo e três estados em vez de dois: host morto → exit 0 (CONFIRMED,
  polaridade invertida do item `done`), host resolvendo → exit 1 (DRIFTED),
  instrumento indisponível → exit 124, que `run_verify` reporta como BROKEN
  nomeando o instrumento. Contraste medido: o predicado antigo sem `dig` sai
  `exit 0` com saída vazia; o novo sem `python3` sai `exit 124` com
  "FALHA: nao consigo decidir DNS — instrumento python3, nao achado."
