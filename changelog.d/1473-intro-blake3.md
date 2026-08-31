### Fixed

- **A página de entrada da documentação ensinava SHA-256; o CAS é BLAKE3 (B-092).**
  `apps/docs/docs/intro.md` — a primeira página que um cliente novo lê — instruía
  SHA-256 no parágrafo de abertura, no parágrafo do endereçamento e na tabela de
  capacidades. O CAS nativo é BLAKE3 (`crates/corelink-hash/src/lib.rs`), e um digest
  do algoritmo errado é rejeitado com `422 content hash mismatch`, de modo que o
  primeiro PUT de todo cliente que seguisse a página falhava. As três posições passam a
  ensinar BLAKE3, propagando a frase que já existia em `apps/docs/docs/api/http.md`
  (*compute it with `b3sum`, **not** `sha256sum`*). No mesmo arquivo saíram as duas
  linhas que prometiam Buck2 e NativeLink conectando "with zero code changes" — a
  superfície REAPI v2 é servida sobre HTTP/REST e não há ingresso gRPC. Nenhum código
  foi alterado.
