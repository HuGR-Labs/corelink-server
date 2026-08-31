### Fixed

- **`backlog_verify.py` recusa um item cujo bloco cercado perdeu a abertura da
  cerca.** A checagem existente caminhava pelos **blocos** e procurava o título
  acima — logo era estruturalmente cega para a falha inversa: um título cujo
  bloco foi comido por uma resolução de conflito.

  **A janela cega é o item de id MÁXIMO.** Um item perdido no meio do arquivo já
  era pego pela regra de densidade (`missing B-060 — ids must be dense`); só o
  último não deixa buraco e por isso passava despercebido. É justamente o
  recém-adicionado — o mais provável de nascer de um conflito. Observado em
  2026-08-31: 129 títulos, 128 blocos, `confirmed=128, drifted=0, broken=0`.

  A detecção de título é **fence-aware**: `### B-NN` dentro de um bloco cercado é
  exemplo, não item. Sem a máscara, documentar o formato de um item dentro do
  próprio `BACKLOG.md` derrubaria o portão nomeando um id inexistente — falha
  fechada, mas pelo motivo errado.

  A máscara alimenta **só** esta checagem nova; o laço de divergência
  título↔bloco continua lendo os títulos **sem máscara**, como antes. O
  emparelhamento de cercas é sequencial a partir do topo do arquivo, então uma
  abertura perdida deixa a contagem ímpar e **desloca todo emparelhamento
  seguinte em um**: cada `### B-NN` posterior cai numa região tida como cercada e
  some da lista. Medido no `BACKLOG.md` real (129 itens) com a abertura do
  **B-062** removida — títulos mascarados produziam **64 linhas espúrias**
  `heading says B-062, block says id: B-0NN` e retornavam **antes** da regra de
  densidade; sem máscara sai `FATAL: BACKLOG.md is missing B-062 — ids must be
  dense.`, uma linha exata, idêntica à da `main`. Perder a cerca do **último**
  item — o caso que esta checagem existe para pegar — não desalinha bloco nenhum
  do seu título, então o laço fica calado e a checagem de órfão é alcançada
  intacta nos dois casos.

  A checagem de órfão roda **depois** do relatório de divergência, para não
  mascarar o diagnóstico mais preciso.

  A suíte `scripts/test_backlog_verify.sh` ganhou a célula que faltava: **três
  itens, cerca do do meio removida**, esperando a mensagem de densidade e
  proibindo a cascata. Nenhuma das 16 células anteriores podia enxergar isto —
  todas usam fixtures de 1–2 itens, e com só o último danificado não há "depois"
  para deslocar.
