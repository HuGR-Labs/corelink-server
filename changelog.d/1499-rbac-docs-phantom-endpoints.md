### Fixed

- **A documentação RBAC publicada nomeava endpoints que nunca foram servidos, e
  a matriz dava ✅ ao `Viewer` numa rota que lhe devolve 403.** As duas páginas
  (`reference/rbac/permissions.mdx`, `explanation/rbac/permission-matrix.mdx`,
  mais os três locales de cada) citavam `/v1/admin/ops`, `/v1/cas/blobs/{digest}`
  e `/v1/ac/{action_digest}` — nenhum resolve para rota registrada. Foram
  substituídos pelas rotas reais (`/v1/admin/mutate` + `/v1/admin/approve`,
  `/v1/cas/{tenant}/{hash}`, `/v1/ac/{tenant}/{action_digest}`), e cada linha das
  tabelas passa a nomear a rota servida e o predicado que a gateia, com
  `file:line`. Onde não há rota, a linha diz isso em vez de carregar um ✅/❌ que
  nada honra.

  A seção *Audit* fundia duas superfícies com gates diferentes. `GET
  /v1/customer/audit` é gateado por `requires_billing_admin`
  (`scope.rs:229`), que aceita só `billing｜admin｜owner`; o Worker dá exatamente
  `read-only` a uma sessão `viewer` (`worker/src/index.ts:3100-3107`), logo o
  `Viewer` recebe **403** — já cravado por `read_only_caller_cannot_read_audit`
  (`customer.rs:1900-1915`). Já export e analytics usam `requires_audit_read`
  (`scope.rs:251`), que aceita qualquer credencial de leitura, e ali o `Viewer`
  **passa**. `admin_op_log` não tem rota de leitura alguma. As três linhas
  estavam erradas, cada uma por um motivo distinto.

  Também documentado: `cache:delete` não é entrada de enforcement — o `DELETE`
  existe (`cas.rs:709`, `ac.rs:451`) mas gateia em `can_write()`
  (`cas.rs:1584`, `ac.rs:707`); e o bitset `PatScopes` não é lido por nenhuma
  rota servida — a autorização vem do header `x-corelink-scope`.
