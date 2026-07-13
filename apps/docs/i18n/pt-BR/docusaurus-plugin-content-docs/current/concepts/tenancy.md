---
id: tenancy
title: Modelo de tenant e escopo de PAT
sidebar_position: 2
description: "Como o CoreLink isola tenants, como os PATs têm escopo definido e como é o acesso entre tenants (resposta: impossível)."
---

<!-- i18n:MT (pt-BR) — TMX-seeded machine-translation stub; replace with native-speaker translation before GA -->

> MT: Esta página está em tradução. A versão canônica em inglês é a fonte de verdade até a revisão por falante nativo (D+10).
>
> Canonical EN source: `docs/concepts/tenancy.md`

# Modelo de tenant e escopo de PAT

## O que é um tenant?

Um **tenant** é o limite de isolamento de nível mais alto no CoreLink. Cada pedaço
de dado — blobs do CAS, entradas do AC, eventos de auditoria, registros de
cobrança — pertence a exatamente um tenant. Nenhum dado é jamais legível ou
gravável entre tenants.

Você recebe um tenant ID durante o cadastro. Ele parece um identificador curto e
seguro para URL:

```text
acme-prod
```

O tenant ID aparece em todos os caminhos de API:

```
/v1/cas/acme-prod/<sha256>
/v1/ac/acme-prod/<action_digest>
```

## Personal Access Tokens (PATs)

Os PATs são o único tipo de credencial que o CoreLink aceita para chamadas de API.
Não há chaves de API, tokens OAuth ou contas de serviço — um PAT *é* a conta de
serviço.

### Propriedades do PAT

| Propriedade | Detalhes |
|---|---|
| Prefixo | `clk_live_` (produção) ou `clk_test_` (ambiente de teste) |
| Escopo | Exatamente um tenant no momento da emissão |
| Exibido uma vez | Exibido em texto puro apenas na criação; o hash é armazenado no servidor |
| Revogável | A qualquer momento pelo painel de administração ou `DELETE /v1/pats/:pat_id` |
| Expiração | Opcional; definida no momento da criação; por padrão não expira |

### Escopos de PAT

Ao criar um PAT, você pode restringi-lo a um subconjunto de operações:

| Escopo | Concede |
|---|---|
| `cas:read` | `GET /v1/cas/*` |
| `cas:write` | `PUT /v1/cas/*` |
| `ac:read` | Ler entradas do action cache |
| `ac:write` | Gravar entradas do action cache |
| `admin` | Gerenciamento de usuários, gerenciamento de PAT, exportação de log de auditoria |

Omitir um escopo significa que o PAT não pode executar aquela operação. O PAT
inicial emitido durante o cadastro tem `cas:read cas:write ac:read ac:write` —
suficiente para todas as integrações de ferramentas de build.

### Boas práticas de CI/CD

Não use seu PAT inicial pessoal em CI. Crie um PAT de CI dedicado com os escopos
mínimos necessários:

```bash
# Create a CI PAT with CAS + AC access only (no admin)
curl -s -X POST \
  -H "Authorization: Bearer $CORELINK_ADMIN_PAT" \
  -H "Content-Type: application/json" \
  -d '{"label": "ci-bazel", "scopes": ["cas:read", "cas:write", "ac:read", "ac:write"]}' \
  https://corelink-api.humangr.com/v1/pats
```

Armazene o valor do token retornado nos secrets do GitHub Actions, no Vault ou no
gerenciador de segredos de sua escolha.

## Isolamento entre tenants

O CoreLink impõe o isolamento de tenants em todas as camadas:

1. **Roteamento de API**: toda requisição de CAS e AC carrega o tenant ID no
   caminho da URL. O worker valida que o tenant do PAT corresponde ao tenant do
   caminho antes de tocar o armazenamento.
2. **Camada de armazenamento**: as chaves de objeto do R2 são prefixadas pelo
   tenant ID. Um bug que omita a verificação de prefixo não pode produzir uma
   colisão de chaves que vaze dados de outro tenant, porque o prefixo é
   obrigatório, não opcional.
3. **Log de auditoria**: cada evento carrega o tenant ID e é armazenado em um
   namespace KV específico do tenant. As consultas de nível de administrador têm
   escopo no namespace do tenant que faz a chamada.

Leituras entre tenants **não são possíveis** — nem como opção de configuração, nem
sob solicitação, nem pela API de administração. Se você precisa compartilhar
artefatos entre dois tenants (por exemplo, uma biblioteca compartilhada usada por
duas equipes de produto), envie o blob sob ambos os tenants ou use um único tenant
compartilhado com vários PATs com escopo por equipe.

Essa garantia de isolamento está documentada como a invariante
**INV-TENANT-ISOLATION** no modelo de segurança. Consulte
[Segurança](../security.md) para a lista completa de invariantes.

## Organização vs. tenant

Hoje, o CoreLink tem um mapeamento 1:1 entre organização (unidade de cadastro) e
tenant. Organizações multi-tenant (onde uma entidade de cobrança gerencia
sub-tenants para diferentes equipes ou ambientes) estão no roadmap, mas ainda não
estão disponíveis.

Um padrão comum nesse meio-tempo: crie contas separadas para `acme-prod` e
`acme-staging`, cada uma com seus próprios PATs. Os pipelines de build são
configurados por ambiente.

## Listar e revogar PATs

```bash
# List all PATs for your tenant
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/pats

# Revoke a PAT by ID
curl -s -X DELETE \
  -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/pats/<pat_id>
```

Após a revogação, quaisquer requisições em andamento que usem aquele PAT receberão
`401 Unauthorized`.
