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

  A checagem roda **depois** do relatório de título↔bloco divergente, para não
  mascarar o diagnóstico mais preciso.
