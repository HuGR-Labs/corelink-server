### Fixed

- **O caminho do dinheiro passa a exigir o mesmo piso de entropia das demais superfícies
  internas (B-074).** `/v1/onboarding/tier-select` (checkout pago) e
  `/v1/onboarding/dpa-accept` (o recibo de consentimento juridicamente vinculante) liam
  `CORELINK_INTERNAL_AUTH_KEY` cru com piso de **16** caracteres, enquanto toda outra
  superfície interna já resolvia por `resolve_internal_auth_key` com piso de **32**. Eram
  os dois únicos arquivos de fora — e justamente os dois por onde passam dinheiro e
  consentimento. Ambos agora resolvem pelo helper, com variável dedicada
  (`CORELINK_TIER_SELECT_AUTH_KEY`, `CORELINK_DPA_ACCEPT_AUTH_KEY`) e fallback
  documentado para a compartilhada, fechando as duas metades do defeito: a entropia e a
  ausência de caminho de rotação para credencial própria.

  **Nota de operação:** se o segredo compartilhado vivo em produção tiver menos de 32
  caracteres, as duas rotas passam a **não montar** (404) até que um segredo bem
  dimensionado seja provisionado (`openssl rand -hex 32`).
