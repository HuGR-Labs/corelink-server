### Fixed

- **B-080 — metade do catálogo canônico de escopos de PAT era decorativa; removida em vez
  de fingida.** `crates/corelink-pat/src/scopes.rs` publicava doze escopos canônicos, dos
  quais seis (`admin:tenant-read`, `admin:tenant-write`, `admin:tokens`, `admin:billing`,
  `admin:audit`, `admin:users`) não eram consultados por **nenhum** ponto de aplicação: um
  token com `admin:audit` recebia exatamente o privilégio de um sem ele, e qualquer plano
  de menor-privilégio escrito sobre esses nomes era decorativo. Os seis foram **removidos**
  do catálogo e colapsados no único nome que é de facto cunhável, persistível e aplicado —
  `admin`. A evidência que escolheu remover em vez de aplicar: (i) **não eram cunháveis** —
  o classificador self-serve (`scope::classify_requested_scopes`) devolve `Err` para
  qualquer token `admin:*`, e o mint interno (`routes::internal_pat::scope_label_to_bits`)
  só aceita os rótulos `admin`/`cas:rw`/`read-write`/`read-only`, ligando os seis bits
  apenas **em bloco** via `SCOPE_ADMIN_ALL`; (ii) **não eram persistíveis** — `pat.scope` é
  `TEXT CHECK (scope IN ('read-write','read-only','admin'))` (`migrations/d1/0037`) e não
  existe coluna de bitset, de modo que o `PatScopes` u64 nunca sai da memória do mint;
  (iii) **o consumidor natural já é aplicado sob outro vocabulário** —
  `scope::requires_billing_admin` (`billing`/`admin`/`owner`) gateia
  `/v1/customer/billing*` e `/v1/customer/audit`, e o dashboard admin autentica por sessão
  Clerk, não por PAT. Implementar o enforcement dos seis teria ensinado o ponto de
  aplicação a reconhecer strings que nenhum cunhador emite e que o D1 não sabe guardar —
  além de abrir um caminho **novo** para billing (`requires_billing_admin("admin:billing")`
  passaria a ser `true`), ou seja, aumento de superfície disfarçado de reparo.

### Added

- **Guarda de classe para nomes de escopo decorativos.**
  `crates/corelink-container/tests/scope_catalog_closure.rs` reprova qualquer nome
  publicado no catálogo canônico de `corelink-pat` que nenhum predicado de
  `corelink_server::scope` consulte — fechando a **classe**, não só a instância. Traz
  calibração explícita do reconhecedor (controles positivo e negativo, sem os quais um
  reconhecedor que devolvesse sempre `true` deixaria o teste vacuamente verde),
  anti-vacuidade (catálogo vazio reprova), a **prova de negação** que o item exigiu (cada
  nome removido é recusado em billing, cache read/write, find-missing e no cunhador, com
  controles provando que os predicados não recusam tudo) e uma ledger de exceções
  declaradas que não pode envelhecer em silêncio. A ledger registra `cache:delete`,
  `execute:action` e `report:result` — descobertos no mesmo levantamento e igualmente sem
  consumidor algum, o que faz **9 de 12** nomes decorativos, e não 6 — como capacidades não
  construídas, tornando a dívida visível em vez de invisível.

### Changed

- **`auth_model.md` §3.1 passa a distinguir escopo APLICADO de escopo apenas planejado**
  (v0.2.0 → v0.3.0). Nova §3.1.1 registra por que os seis `admin-*` granulares nunca
  saíram do papel e por que a granularidade que eles prometiam já existe sob outros nomes.
  Como a mudança **remove** alegação não implementada e não expande superfície de
  segurança, §3.4/FF-HR-005 (ADR + security review para **adicionar** escopo) não dispara;
  reintroduzir qualquer um dos seis continua sujeito a §3.4 na íntegra.
- **`marketing/corelink-feature-catalog.html`** deixa de vender treze escopos como
  enforcement de menor-privilégio entregue: o card agora nomeia o que é aplicado hoje
  (cache read/write/find-missing e admin) e diz explicitamente que os três reservados não
  concedem nada.
- **A referência RBAC publicada contradizia o novo significado do bit 4.**
  `apps/docs/docs/reference/rbac/permissions.mdx` documentava o bit 4 como
  `admin:tenant-read` — "List tenant config", concedido por padrão a `Owner`,
  `Admin` **e `Viewer`** — enquanto o bit 4 passou a ser `SCOPE_ADMIN`, o superset
  owner-grade. Ou seja, a superfície que o cliente lê anunciava que `Viewer` tinha
  o bit que hoje é admin. Corrigidos 5 documentos × 4 locales (20 arquivos):
  contagens 12→7, o bloco dos seis substituído pelo `admin` com aviso explícito de
  que os granulares nunca foram implementados, bits 5..=9 documentados como
  retirados e fail-closed, `cache:delete` marcado como NÃO aplicado, a coluna
  *Scope* da matriz deixando de citar escopos inexistentes (administrativo →
  `session role`, billing → `billing`), e o pré-requisito acionável **falso** do
  guia de auditoria ("You hold `admin:audit` scope") substituído pelo gate real
  (`requires_billing_admin`). Os locales de `audit-role-changes` são tradução
  genuína e foram reescritos em de/es/pt, não colados do inglês.
- **Correção de rota: `cache:delete` não é aplicado, mas a deleção de blob EXISTE
  — e é gateada por cache-WRITE.** A primeira versão desta mudança afirmou que
  "não existe rota de delete de blob". Falso: `DELETE /v1/cas/{tenant}/{hash}`
  existe (`routes/cas.rs:709`, handler em 1556) e é gateada por
  `scope.can_write()`, nunca por `cache:delete`. O escopo segue sem ponto de
  aplicação; a capacidade, não. Consequência de segurança na matriz publicada:
  ela marcava `Developer ❌` para deleção de blob, mas como o gate é cache-WRITE e
  o Developer tem write, o Developer **pode** deletar — a linha estava errada na
  direção permissiva e foi corrigida.
- **O guia de auditoria passou a exigir um token que o self-service nunca emite.**
  As rotas `/v1/audit/analytics/*` e `/v1/audit/{tenant}/export` — as três que o
  próprio guia demonstra com `curl` — são gateadas por `requires_audit_read`
  (= `requires_cache_read || "owner"`), satisfeito por qualquer token com leitura,
  incluindo o `read-write` padrão do painel. O texto que citava
  `requires_billing_admin` descrevia outra superfície
  (`GET /v1/customer/audit`, a página do painel) e tornava o guia mais restritivo
  que o produto. Corrigido em EN + de/es/pt.
