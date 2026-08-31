### Fixed

- **`backlog_verify.py` recusa um item cujo bloco cercado perdeu a abertura da
  cerca.** A checagem existente caminhava pelos **blocos** e procurava o título
  acima — logo era estruturalmente cega para a falha inversa: um título cujo
  bloco foi comido por uma resolução de conflito. Esse item deixa de existir
  para **todas** as verificações do arquivo, e o portão reporta verde sobre os
  sobreviventes. Observado em 2026-08-31: 129 títulos, 128 blocos,
  `confirmed=128, drifted=0, broken=0` — o item teria mergeado verde e sumido do
  registro. Item ausente é indistinguível de item malformado a menos que algo
  compare as duas populações; agora algo compara, e a mensagem imprime os dois
  números.
