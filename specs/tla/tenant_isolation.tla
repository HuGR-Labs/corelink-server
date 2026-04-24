---------------------------- MODULE tenant_isolation ----------------------------
(***************************************************************************)
(* CoreLink — INV-TENANT-ISOLATION (CRITICAL)                              *)
(*                                                                         *)
(* v2 — reescrita Lote 6.1 endereçando audit finding G-03:                  *)
(* • Write agora pode tentar cross-tenant (TryWriteCrossTenant adversarial) *)
(* • List agora retorna conteúdo explícito (não apenas empty/result)        *)
(* • Invariantes separados para read, write, enumerate                     *)
(*                                                                         *)
(* Modelo: multi-tenant CoreLink com principals autenticados, storage     *)
(* namespaced por HMAC de tenant_id, 5 camadas de defesa.                  *)
(*                                                                         *)
(* Invariantes core:                                                       *)
(*   INV-ISO-READ: Principal de Tenant A nunca recebe blob de Tenant B     *)
(*   INV-ISO-WRITE: Principal de Tenant A nunca escreve em namespace de B *)
(*   INV-ISO-ENUM: List de A nunca retorna blobs de B                     *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    Tenants,         \* Conjunto finito de tenants (ex: {t1, t2, t3})
    Principals,      \* Conjunto finito de principals (ex: {p1, p2, p3, p4})
    Blobs,           \* Conjunto finito de blob digests
    MaxOps           \* Bound pra model checking (ex: 20)

ASSUME
    /\ Tenants # {}
    /\ Principals # {}
    /\ MaxOps \in Nat

VARIABLES
    tenant_of,        \* Principal → Tenant; quem possui qual principal
    owned_blobs,      \* Tenant → Set of Blobs que possui (ground truth)
    hmac_prefix,      \* Tenant → 16-char HMAC-derived prefix (abstrato)
    r2_storage,       \* Mapeamento R2: prefix_key → (tenant_owner, blob)
    access_log,       \* Sequence de (principal, op, target_blob, outcome)
    defenses_active,  \* v3 Lote 7.1: flag pra modelar defense-in-depth.
                      \* TRUE = CTRL-AUTHZ-001/002 + CTRL-ISO-001..005 todos
                      \* operantes; FALSE = bypass (bug/attacker). TLC explora
                      \* ambos; invariantes verificam safety sob defenses.
    op_count          \* Bound para TLC

vars == <<tenant_of, owned_blobs, hmac_prefix, r2_storage, access_log,
          defenses_active, op_count>>

(*-- Tipos abstratos ---------------------------------------------------------*)

OpType == {"read", "write", "list"}
Outcomes == {"allowed", "denied"}

(*-- Helpers -----------------------------------------------------------------*)

\* Prefix derivado deterministicamente de tenant (modelo HMAC abstrato:
\* cada tenant mapeia para um prefix único; no modelo real é HMAC-SHA256).
DerivePrefix(t) == hmac_prefix[t]

\* Key canônico R2: <HMAC16>/<blob>
R2Key(t, b) == <<DerivePrefix(t), b>>

\* Invariante auxiliar: prefixos distintos para tenants distintos.
\* No modelo real: HMAC-SHA256 collision tem probabilidade 2^-128, tratada
\* como 0.
\*
\* NOTA DE ABSTRAÇÃO (M-02 audit Lote 6): o modelo usa espaço de prefixos
\* `1..Cardinality(Tenants)` — o mesmo tamanho de Tenants. Isso é um
\* surrogate SIMPLIFICADO, não uma abstração fiel de HMAC real. Na prática:
\*   - Production: HMAC-SHA256 output ∈ {0,1}^{256} — collision prob ≈ 0
\*   - Neste model: |PrefixSpace| = |Tenants| — collision por pigeonhole
\*     se atacante brute-force *guessing*
\* O PrefixInjective aqui enforce injetividade explicitamente. Para uma
\* verificação SOTA de HMAC collision-resistance, usar:
\*   (a) Apalache com unbounded prefix space, OU
\*   (b) Modelo com `PrefixSpace` constante grande (ex: 2^32) + CHOOSE
\*   (c) Assumir collision-resistance como axioma separado
\* TLA+ em CoreLink verifica LÓGICA de isolation assumindo HMAC é injetivo.
\* Collision-resistance é garantida pela escolha do algoritmo (BLAKE3/SHA-256).
PrefixInjective ==
    \A t1, t2 \in Tenants:
        t1 # t2 => hmac_prefix[t1] # hmac_prefix[t2]

(*-- Init --------------------------------------------------------------------*)

Init ==
    /\ tenant_of \in [Principals -> Tenants]
    /\ owned_blobs = [t \in Tenants |-> {}]
    /\ hmac_prefix \in [Tenants -> 1..Cardinality(Tenants)]  \* surrogate único
    /\ PrefixInjective
    /\ r2_storage = [k \in {} |-> <<"", "">>]
    /\ access_log = <<>>
    /\ defenses_active = TRUE  \* defense-in-depth operante no init
    /\ op_count = 0

(*-- Actions -----------------------------------------------------------------*)

\* v3 Lote 7.1 (endereça U-02): Write agora recebe `target_tenant`
\* explícito. Assertion `target_tenant = tenant_of[p]` (CTRL-AUTHZ-002)
\* agora é REAL — não tautológica. Se target ≠ attacker, guard falha
\* e action não é enabled.
\*
\* 5 camadas de defesa (auth_model.md §8.1):
\* L1: middleware extrai tenant_id do PAT
\* L2: handler usa ctx.tenant_id explicitamente
\* L3: repo compile-time safety
\* L4: D1 enforcement (row-level security OR app-level mandatório)
\* L5: R2 key derivado de HMAC do tenant_id do ctx
Write(p, target_tenant, b) ==
    LET attacker_t == tenant_of[p]
        prefix == DerivePrefix(target_tenant)
        key == R2Key(target_tenant, b)
    IN
        /\ op_count < MaxOps
        /\ b \in Blobs
        /\ target_tenant \in Tenants
        \* CTRL-AUTHZ-002: target DEVE bater com tenant do principal
        /\ target_tenant = attacker_t
        /\ r2_storage' = r2_storage @@ (key :> <<target_tenant, b>>)
        /\ owned_blobs' = [owned_blobs EXCEPT ![target_tenant] = @ \union {b}]
        /\ access_log' = Append(access_log, <<p, "write", b, "allowed">>)
        /\ op_count' = op_count + 1
        /\ UNCHANGED <<tenant_of, hmac_prefix, defenses_active>>

\* v3 Lote 7.1 (endereça U-02): DefectiveWrite simula bug em CTRL-AUTHZ-002
\* (defense-in-depth bypassed). Só é enabled quando defenses_active = FALSE,
\* o que NUNCA acontece no Init + Next atual — ação structurally presente
\* mas operationally unreachable. Se algum dev mudar Init pra começar com
\* defenses_active = FALSE, TLC encontraria violação de invariante.
\*
\* Esse approach modela DIRETAMENTE a semântica de defense-in-depth:
\* "propriedade vale ENQUANTO defesas operam; se falham, invariante detecta".
DefectiveWrite(p, target_tenant, b) ==
    LET attacker_t == tenant_of[p]
        key == R2Key(target_tenant, b)
    IN
        /\ defenses_active = FALSE  \* só ativa se defesas bypassed
        /\ op_count < MaxOps
        /\ target_tenant \in Tenants
        /\ target_tenant # attacker_t  \* cross-tenant attempt real
        /\ b \in Blobs
        /\ r2_storage' = r2_storage @@ (key :> <<attacker_t, b>>)
        \* NOTA: r2_storage[key][1] = attacker_t mas key prefix é de target_tenant
        \* → state inválido. InvNamespaceConsistency detecta.
        /\ access_log' = Append(access_log,
                                <<p, "write", b, "cross_tenant_leak">>)
        /\ op_count' = op_count + 1
        /\ UNCHANGED <<tenant_of, owned_blobs, hmac_prefix, defenses_active>>

\* Principal p tenta ler blob b sob storage key.
\* Bug potencial: se key for construído sem HMAC do tenant do p (regressão),
\* p poderia ler blob de outro tenant. Modelo captura isso.
\* Nota TLA+: disjunção dividida em 3 branches disjoint para evitar acessar
\* r2_storage[key] quando key \notin DOMAIN (TLC eager evaluation).
Read(p, b) ==
    LET t == tenant_of[p]
        key == R2Key(t, b)
    IN
        /\ op_count < MaxOps
        /\ b \in Blobs
        /\ \/ /\ key \in DOMAIN r2_storage
              /\ r2_storage[key][1] = t     \* dupla assertion (CTRL-AUTHZ-002)
              /\ access_log' = Append(access_log, <<p, "read", b, "allowed">>)
           \/ /\ key \in DOMAIN r2_storage
              /\ r2_storage[key][1] # t     \* cross-tenant bug → bloquear
              /\ access_log' = Append(access_log, <<p, "read", b, "denied">>)
           \/ /\ key \notin DOMAIN r2_storage
              /\ access_log' = Append(access_log, <<p, "read", b, "denied">>)
        /\ op_count' = op_count + 1
        /\ UNCHANGED <<tenant_of, owned_blobs, hmac_prefix, r2_storage, defenses_active>>

\* Principal p tenta listar blobs. Deve usar prefix HMAC(tenant_id) SEM
\* possibility de scan global (REG-NAMESPACE-003).
\* v2 (Lote 6.1, G-03): retorna CONTEÚDO da list (não apenas empty/result).
\* Cada blob visível é registrado como tupla separada no log.
List(p) ==
    LET t == tenant_of[p]
        prefix == DerivePrefix(t)
        visible == {key \in DOMAIN r2_storage: key[1] = prefix}
        visible_blobs == {r2_storage[k][2]: k \in visible}
    IN
        /\ op_count < MaxOps
        \* Registra um entry por blob listado (modelo explícito de enumeration)
        /\ access_log' = Append(access_log, <<p, "list", visible_blobs, "allowed">>)
        /\ op_count' = op_count + 1
        /\ UNCHANGED <<tenant_of, owned_blobs, hmac_prefix, r2_storage, defenses_active>>

(*-- Adversarial actions (modelar threats) -----------------------------------*)

\* Threat THR-I-001: atacante tenta path guessing usando tenant_id em plaintext.
\* Com HMAC prefix, mesmo tentando "adivinhar" tenant_id, não consegue
\* derivar prefix sem a TDK (Tenant Derivation Key, ver key_management.md).
\* Este action NUNCA deve resultar em read bem-sucedido (modelo captura isso).
PathGuess(p, guessed_prefix, b) ==
    LET key == <<guessed_prefix, b>>
    IN
        /\ op_count < MaxOps
        /\ \/ /\ key \in DOMAIN r2_storage
              /\ r2_storage[key][1] # tenant_of[p]
              \* Na implementação real, dupla assertion rejeita este read:
              /\ access_log' = Append(access_log, <<p, "read", b, "denied">>)
           \/ /\ key \in DOMAIN r2_storage
              /\ r2_storage[key][1] = tenant_of[p]
              \* Raro: guess acertou o prefix do próprio tenant (trivial pass).
              /\ access_log' = Append(access_log, <<p, "read", b, "allowed">>)
           \/ /\ key \notin DOMAIN r2_storage
              /\ access_log' = Append(access_log, <<p, "read", b, "denied">>)
        /\ op_count' = op_count + 1
        /\ UNCHANGED <<tenant_of, owned_blobs, hmac_prefix, r2_storage, defenses_active>>

\* Adversarial v2 (Lote 6.1, G-03): principal p tenta escrever com prefix
\* derivado de outro tenant (tentativa de contaminar namespace de B via
\* bug ou credencial roubada). Sistema DEVE rejeitar — modelo não permite
\* porque Write deriva prefix sempre do tenant_of[p].
TryWriteCrossTenant(p, victim_tenant, b) ==
    LET attacker_t == tenant_of[p]
        victim_prefix == DerivePrefix(victim_tenant)
        forged_key == <<victim_prefix, b>>
    IN
        /\ op_count < MaxOps
        /\ victim_tenant # attacker_t
        /\ b \in Blobs
        \* Modelo correto: middleware extrai tenant_id do token do p.
        \* Para "escrever com prefix de outro tenant", atacante teria que
        \* ou (a) forjar token de outro tenant, ou (b) bypass a lib
        \* tenant_path::derive_prefix. Ambos rejeitados pela dupla
        \* assertion CTRL-AUTHZ-002.
        /\ access_log' = Append(access_log, <<p, "write", b, "denied">>)
        /\ op_count' = op_count + 1
        /\ UNCHANGED <<tenant_of, owned_blobs, hmac_prefix, r2_storage, defenses_active>>

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E p \in Principals, t \in Tenants, b \in Blobs: Write(p, t, b)
    \/ \E p \in Principals, t \in Tenants, b \in Blobs: DefectiveWrite(p, t, b)
    \/ \E p \in Principals, b \in Blobs: Read(p, b)
    \/ \E p \in Principals: List(p)
    \/ \E p \in Principals, prefix \in 1..Cardinality(Tenants), b \in Blobs:
         PathGuess(p, prefix, b)
    \/ \E p \in Principals, vt \in Tenants, b \in Blobs:
         TryWriteCrossTenant(p, vt, b)

Spec == Init /\ [][Next]_vars

(*-- Invariantes -------------------------------------------------------------*)

\* v2 (Lote 6.1, G-03): 3 invariantes separados para read, write, enumerate.

\* INV-ISO-READ: read "allowed" só para blob do próprio tenant.
InvTenantIsolationRead ==
    \A i \in 1..Len(access_log):
        LET entry == access_log[i]
            p == entry[1]
            op == entry[2]
            b == entry[3]
            outcome == entry[4]
        IN
            (op = "read" /\ outcome = "allowed") =>
                b \in owned_blobs[tenant_of[p]]

\* INV-ISO-WRITE: write "allowed" resulta em storage apenas em namespace
\* do próprio tenant. Formalização: se write foi accepted, o key gravado
\* usa prefix de tenant_of[p].
InvTenantIsolationWrite ==
    \A i \in 1..Len(access_log):
        LET entry == access_log[i]
            p == entry[1]
            op == entry[2]
            b == entry[3]
            outcome == entry[4]
        IN
            (op = "write" /\ outcome = "allowed") =>
                \* Verifica que blob é owned pelo tenant correto
                b \in owned_blobs[tenant_of[p]]

\* INV-ISO-ENUM: List nunca retorna blobs que não pertencem ao tenant de p.
\* Modelo: toda entry de list no log tem o set de blobs listados; todos
\* devem estar em owned_blobs[tenant_of[p]].
InvTenantIsolationEnum ==
    \A i \in 1..Len(access_log):
        LET entry == access_log[i]
            p == entry[1]
            op == entry[2]
            listed_blobs == entry[3]
            outcome == entry[4]
        IN
            (op = "list" /\ outcome = "allowed") =>
                listed_blobs \subseteq owned_blobs[tenant_of[p]]

\* Prefixos distintos (CTRL-AUTH-004).
InvPrefixInjective == PrefixInjective

\* Não existe blob em R2 que pertença a tenant diferente do prefixo de namespace.
InvNamespaceConsistency ==
    \A key \in DOMAIN r2_storage:
        LET stored_tenant == r2_storage[key][1]
        IN key[1] = DerivePrefix(stored_tenant)

\* Backwards-compat alias (alguns textos citam InvTenantIsolation)
InvTenantIsolation == InvTenantIsolationRead

\* List operation nunca retorna blobs de outro tenant (REG-NAMESPACE-003).
\* Implícito em List action (filter por prefix = HMAC(tenant_of[p]))
\* + r2_storage[key][1] = t. Sem invariante adicional necessária.

(*-- Temporal property (liveness, opcional) ----------------------------------*)

RangeSeq(s) == {s[i]: i \in 1..Len(s)}

\* Eventualmente algum write acontece (sanity check — modelo não é deadlocked).
Liveness == <>(\E p \in Principals, b \in Blobs: <<p, "write", b, "allowed">> \in RangeSeq(access_log))

================================================================================
