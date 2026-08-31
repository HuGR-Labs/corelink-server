### Fixed

- **O portão do OKF deixou de premiar o atalho: âncora de blob movida agora
  exige a renumeração que ela estava substituindo.** `cited_range_drifted` faz
  curto-circuito quando o blob da árvore de trabalho é igual à âncora
  `source_blobs`, então reapontar a âncora para o arquivo como ele está agora
  fazia o C5 comparar o arquivo **com ele mesmo** — toda citação àquele arquivo
  ficava fresca por construção, apontasse para a linha que apontasse. Não era
  falta de disciplina e sim defeito de incentivo: avançar a âncora custava um
  comando e ficava verde na hora, renumerar custava script mais verificação byte
  a byte, e as duas terminavam verdes. Em 2026-08-30 duas sessões independentes
  que conheciam a ressalva tomaram o atalho no mesmo dia (#1389, 17 de 18
  citações erradas; #1393, mais 16) com `validate_okf` dizendo `0 stale`. O novo
  check **C5c** dispara só quando uma âncora é adicionada ou alterada contra a
  versão-base do conceito e, para cada citação anterior daquele arquivo, localiza
  o conteúdo autoral na árvore atual: renumerado passa, conteúdo deslocado sem
  citação correspondente reprova nomeando as linhas exatas, conteúdo reescrito ou
  posição ambígua fica em silêncio.

- **A citação abreviada `` `:N-M` `` passou a ser cidadã de primeira classe.**
  `CITE_RE` exigia caminho não-vazio, então a forma de continuação que a wiki usa
  para não repetir um caminho longo era descartada por `_collect_cites`: **107
  citações em 20 conceitos** nunca checadas por C3, C5 ou C6. Era o amplificador
  do defeito acima — 7 das citações erradas do #1389 estavam nessa forma. As
  checagens bloco-locais de fundamentação (C6c) mantêm a forma estrita, então a
  mudança é um fortalecimento estrito.

- **`Git.merge_base` resolvia só a ref nua, e isso desligava duas checagens em
  silêncio.** Na CI (`actions/checkout` em HEAD destacado) `main` não existe como
  branch local, então a função devolvia `None` e tudo que precisa de versão
  anterior — C5b anti-fantasma, C4c catraca de blob — virava no-op sem avisar. Em
  worktree de desenvolvimento, o inverso: um `main` local obsoleto resolvia e as
  checagens comparavam contra uma base de semanas atrás. Agora resolve
  `origin/<ref>` primeiro.

### Added

- **`scripts/okf_shift_citations.py`** — deslocador de citações OKF dirigido por
  conteúdo. Nunca soma offset a número: lê o conteúdo que a citação nomeava na
  base e procura onde ele está agora, alargando com as linhas vizinhas até a
  posição ser única, e deixa intacto o que não conseguir provar. Reescreve por
  span de caracteres da direita para a esquerda, o que é a razão estrutural de
  não corromper o `file:N-M` vizinho ao reescrever `file:N`. Recusa arquivo com
  marcador de conflito e recusa o par (conceito, arquivo) cujas citações já
  diferem da base — a regra "um método por ARQUIVO", mecanizada, de modo que
  rodar duas vezes é seguro em vez de duplicar o deslocamento.
