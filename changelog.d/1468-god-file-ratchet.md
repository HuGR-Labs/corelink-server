### Added

- **Catraca de tamanho de arquivo (`scripts/validate_file_size_ratchet.py` +
  lane `file-size ratchet`), a favor da decisão do owner de que todo arquivo
  acima de 1000 linhas é refatorado (B-126).** Arquivo **novo** acima do limite
  reprova; arquivo **já** na linha de base reprova se **crescer**; arquivo que
  cai para o limite **gradua** e passa a valer a regra dura. Não é teto seco de
  propósito: 81 arquivos já estão acima, e portão que nasce vermelho não é
  obedecido, é afrouxado — e o afrouxamento passa a ler como "revisado e
  aceito". A linha de base é versionada, então toda flexibilização é um diff
  revisável em vez de uma flag. Falha de medição sai com código **2**, nunca 0:
  "não achei violação" e "não consegui olhar" não podem compartilhar exit code.
