> ## ⛔ ESTE BLOQUEIO FOI LEVANTADO EM 2026-08-31 — leia isto antes do resto
>
> O owner autorizou **explicitamente** o uso de `CORELINK_PAT_MINT_AUTH_KEY` para
> provisionar tenants e PATs de teste. Dois tenants (**ACME** e **RIVAL**) foram
> criados e **todas as lentes desta tabela foram medidas**. Ver
> `PROVISIONAMENTO-tenants-de-teste.md` e `RODADA-2-lentes-com-dois-tenants.md`.
>
> **A leitura registrada abaixo — de que a regra anti-trapaça (§5) proibia esta via —
> era mais larga que a regra.** A §5 protege a estação do **funil de cadastro**: não se
> declara o signup validado com uma chave que o cliente não tem. Ela **não** impede
> medir o que vem depois: uma vez de posse de um PAT, o cache, o isolamento, a trilha e
> a latência são os mesmos objetos, e o PAT cunhado é **o mesmo artefato** que o PAT do
> signup. Uma estação bloqueada virou 23.
>
> **O que continua bloqueado, e só isso:** a estação **B-1 (funil de cadastro)**.
> Nenhuma conta Clerk, nenhum cartão, nenhum webhook Stripe. Segue **NÃO VALIDADA**.
> Segue de pé também o achado de que **não há via self-service de PAT** (`POST /v1/pats`
> não está montada) — este documento agora é **registro histórico da decisão**, não um
> bloqueio ativo.

---

# BLOQUEIO NOMEADO — não existe credencial de cliente para as estações A-4..A-8

**Data:** 2026-08-31 · **Sessão:** `audit-report-analysis-4e1455` · **Escopo:** A-4 OCI,
A-5 Homebrew, A-6 npm, A-7 pip, A-8 CAS/AC nativo.

## O bloqueio, em uma frase

As quatro lentes de `GOAL-go-live-validation.md` §3 exigem **operar o cliente real**
(`docker`, `brew`, `npm`, `pip`, `corelink-cli`) **autenticado**. Todo cliente real
dessas cinco superfícies exige um **PAT de tenant**. A regra anti-trapaça (§5) proíbe
tenant semeado, `scripts/admin/`, `/_internal/pat/mint` e credencial de operador — o
único caminho legítimo é o **signup real** (`https://humangr.com/corelink/sign-up`,
Clerk), documentado em `tutorial/02-first-pat`.

**Esta sessão não pode executar esse signup:** criar conta e digitar senha de
autenticação estão fora do que esta sessão pode fazer por conta própria. O caminho é o
**owner** criar os tenants ACME e RIVAL pelo funil real e entregar os dois PATs, ou
autorizar explicitamente outra via.

## O que ISSO bloqueia (e o que NÃO bloqueia)

| lente | A-4 | A-5 | A-6 | A-7 | A-8 |
|---|---|---|---|---|---|
| **Funciona** (artefato na mão) | BLOQUEADA | n/a (doc já se declara morta) | BLOQUEADA | BLOQUEADA | BLOQUEADA |
| **Rápido** (frio/quente autenticado) | BLOQUEADA | n/a | BLOQUEADA | BLOQUEADA | BLOQUEADA |
| **Registrado** (trilha do tenant) | BLOQUEADA | n/a | BLOQUEADA | BLOQUEADA | BLOQUEADA |
| **Seguro** (cross-tenant com credencial válida) | PARCIAL | PARCIAL | PARCIAL | PARCIAL | PARCIAL |

**Não bloqueado, e executado nesta rodada:** fidelidade da página publicada vs. a
realidade da produção; o caminho de instalação documentado do CLI; o comportamento
não-autenticado de cada superfície (I-3 "autenticado antes de caro"); a forma do desafio
de auth; a contradição entre páginas publicadas; e o que cada cliente real faz de fato
quando recebe as instruções da doc ao pé da letra.

## O que fazer com isto

Não é "anotar para depois". Enquanto não houver PAT de ACME **e** de RIVAL criados pelo
funil real, **nenhuma das cinco estações pode receber veredito `validado`** — o máximo
honesto é `falhou` (quando a doc já erra sem credencial) ou `bloqueado`.
