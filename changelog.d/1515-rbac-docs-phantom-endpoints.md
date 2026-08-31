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

  **O censo foi então refeito sobre a superfície RBAC inteira** — os 24
  arquivos `apps/docs/**/rbac/*.mdx` (EN + 3 locales), não só os 8 tocados
  antes. Ele achou um fantasma sobrevivente: `/v1/admin/ops` dentro de uma
  cerca de código ilustrativa em `explanation/rbac/role-catalog.mdx` (× 4
  locales), que os censos por tabela não olhavam. Aquele fluxo era ficção em
  três eixos: a rota não existe, o campo chama-se `op_kind` (não `op_type`,
  `admin.rs:719`), e `ConfigRollback` é variante de `AdminOpType` do plano
  interno (`corelink-ops` / `corelink-dual-approval`), do qual o contêiner
  servido **não depende**. O fluxo foi reescrito contra o que `admin.rs`
  serve de fato: `POST /v1/admin/approve` primeiro (rota própria,
  `admin.rs:129`, com credencial DIFERENTE da do mutate, `admin.rs:1079-1081`),
  depois `POST /v1/admin/mutate` apresentando o mesmo `approval_id`. Censo
  final: 39 caminhos `/v1/` distintos na superfície, **0 fantasmas**;
  controle negativo `/v1/zzz-nonexistent` marcado como fantasma, controle
  positivo `/v1/admin/tenants/{tenant_id}/usage` resolvido.

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
  rota **servida pelo contêiner** — a autorização vem do header
  `x-corelink-scope`. O admonition publicado dizia "there is no production
  call site", o que é falso ao pé da letra: o crate `corelink-worker`
  (superfície REAPI wasm) lê o bitset em `middleware/auth_ctx.rs:190-192`,
  `reapi/ac/handler/handler_impl.rs:82` e
  `reapi/cas/split_splice/handler.rs:116,126`. Ele só não está no caminho
  servido — `corelink-container` não depende dele e o Worker deployado é
  TypeScript (`wrangler.toml:15`). A frase foi corrigida para dizer isso.
