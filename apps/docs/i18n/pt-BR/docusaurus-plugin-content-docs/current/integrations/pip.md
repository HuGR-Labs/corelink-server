---
id: pip
title: Espelho do pip (PyPI)
sidebar_position: 7
description: Aponte pip, uv, poetry ou pdm para o CoreLink como um espelho de cache na frente do PyPI.
---

<!-- i18n:MT (pt-BR) — TMX-seeded machine-translation stub; replace with native-speaker translation before GA -->

> MT: Esta página está em tradução. A versão canônica em inglês é a fonte de verdade até a revisão por falante nativo (D+10).
>
> Canonical EN source: `docs/integrations/pip.md`

# Espelho do pip (PyPI)

O CoreLink é um **espelho de cache** na frente do `pypi.org`. Ele implementa a
Simple Index API (PEP 691 JSON, com fallback para HTML PEP 503), de modo que `pip`, `uv`,
`poetry` e `pdm` possam usá-lo como seu índice de pacotes. Wheels e distribuições de
código-fonte são cacheados no CAS do seu tenant e têm a integridade verificada contra o
fragmento `#sha256=` do upstream antes de serem cacheados.

Este é um **espelho somente leitura** para instalar pacotes públicos. Ele não hospeda
índices privados e não aceita `twine upload`.

## Pré-requisitos

- `pip` (ou um cliente compatível) instalado.
- Um PAT do CoreLink (`corelink_pat_...`).
- O UUID do seu tenant.

## Configurar

O pip autentica com HTTP basic auth, em que o nome de usuário é `hugr` e a
senha é o seu PAT. Coloque a URL do índice Simple do seu tenant — com as credenciais
embutidas — no `pip.conf` (`~/.config/pip/pip.conf` no Linux,
`~/Library/Application Support/pip/pip.conf` no macOS):

```ini
[global]
index-url = https://hugr:corelink_pat_XXXXXXXXXXXX@corelink-api.humangr.com/pip/<your-tenant-id>/simple/
```

Depois, instale normalmente:

```bash
pip install requests
```

Ou passe inline para um uso pontual (e para a CI, onde o PAT vem de um secret):

```bash
pip install --index-url "https://hugr:${CORELINK_PAT}@corelink-api.humangr.com/pip/<your-tenant-id>/simple/" requests
```

O `uv` respeita a mesma `index-url`:

```bash
uv pip install --index-url "https://hugr:${CORELINK_PAT}@corelink-api.humangr.com/pip/<your-tenant-id>/simple/" requests
```

## Verificar se funcionou

```bash
pip install --force-reinstall --no-cache-dir -v requests 2>&1 \
  | grep corelink-api.humangr.com | head
```

Ver `corelink-api.humangr.com/pip/.../simple/` no log verboso confirma que o
pip está resolvendo através do espelho.

## Resolução de problemas

| Sintoma | Causa provável | Correção |
|---|---|---|
| `401 Unauthorized` | O nome de usuário não é `hugr`, ou a senha não é um PAT `corelink_pat_...` | Use `hugr` como nome de usuário e seu PAT como senha |
| `Could not find a version` | Projeto ainda não cacheado e upstream inacessível | Tente novamente; o CoreLink busca do PyPI na primeira requisição |
| Divergência de hash em uma wheel | O artefato do upstream mudou | O CoreLink verifica o fragmento `#sha256=` e falha de forma fechada em caso de divergência |

Referência completa de erros: [Resolução de problemas](../troubleshooting.md).
