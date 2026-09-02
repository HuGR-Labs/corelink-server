### Fixed

- **O guard de `~/.rustup` agora inspeciona `runs-on:` pela estrutura YAML (B-140).**
  As formas legais de escalar aspeado (`"corelink"`) e sequência em bloco (`- self-hosted`)
  deixavam jobs self-hosted fora da checagem baseada em regex. O parser cobre também listas
  inline, comentários e expressões, falha alto em YAML inválido e mantém fallback stdlib-only
  para a imagem mínima da frota; o fallback agora rejeita itens, chaves e valores vazios em
  flow-maps (incluindo chaves que são flow-collections) com a mesma decisão do PyYAML,
  preservando vírgulas finais válidas e escapes dentro de valores.
  Sufixos após o fechamento de uma flow-collection também são rejeitados, exceto quando
  formam um limite YAML estrutural válido, comentário ou fim de valor.
  Dois-pontos em posição de chave vazia também falham imediatamente, inclusive sem espaço
  (`{:b}`), mantendo chaves compactas válidas como `{a:b}`.
  Scalars duplamente aspeados agora decodificam escapes YAML suportados antes de classificar a
  label (e falham alto no que o fallback não suporta), enquanto `uses` é lido da estrutura de
  cada step, incluindo sequências flow como `steps: [{uses: …}]`; portanto uma ação que muta
  rustup não consegue mais se esconder numa grafia YAML alternativa.
