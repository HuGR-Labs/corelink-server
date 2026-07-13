---
id: oci-registry
title: Integração com registro OCI (Docker / Podman)
sidebar_position: 5
description: Envie e baixe imagens de contêiner e artefatos OCI para o CoreLink, um registro completo da OCI Distribution Spec v1.1.
---

<!-- i18n:MT (pt-BR) — TMX-seeded machine-translation stub; replace with native-speaker translation before GA -->

> MT: Esta página está em tradução. A versão canônica em inglês é a fonte de verdade até a revisão por falante nativo (D+10).
>
> Canonical EN source: `docs/integrations/oci-registry.md`

# Integração com registro OCI (Docker / Podman)

O CoreLink é um registro completo da **OCI Distribution Spec v1.1**. Qualquer cliente OCI
padrão — `docker`, `podman`, `buildah`, `crane`, `helm` (charts OCI), exportadores de
cache do BuildKit — pode fazer push e pull contra ele. Os manifests são armazenados em
chave-valor por tenant; os blobs (camadas e configs) ficam no CAS do tenant.

O host do registro é `corelink-api.humangr.com`. Ao contrário das outras superfícies de
cache, o caminho OCI **não tem segmento de tenant** na URL — seu tenant é
derivado do token com o qual você se autentica, usando o fluxo padrão de token bearer
de duas etapas do registro (`GET /token` e depois `Authorization: Bearer`).

## Pré-requisitos

- `docker` (ou `podman`) instalado.
- Um PAT do CoreLink (`corelink_pat_...`) com escopo de leitura + escrita de cache.

## Fazer login

Faça login com seu PAT como senha. O nome de usuário não é verificado — qualquer valor
(por exemplo, `corelink`) funciona:

```bash
echo "$CORELINK_PAT" | docker login corelink-api.humangr.com \
  --username corelink --password-stdin
```

O Docker realiza a troca de token automaticamente no próximo push ou pull.

## Enviar uma imagem

Marque a imagem com o host do CoreLink e faça o push. O primeiro segmento de caminho após o
host é o **nome do repositório** (não um tenant):

```bash
docker tag my-app:latest corelink-api.humangr.com/my-app:latest
docker push corelink-api.humangr.com/my-app:latest
```

## Baixar uma imagem

```bash
docker pull corelink-api.humangr.com/my-app:latest
```

O Podman usa a mesma referência:

```bash
podman pull corelink-api.humangr.com/my-app:latest
```

:::note O isolamento é por tenant, chaveado pelo PAT
Dois tenants podem ambos fazer push de `my-app:latest` sem colisão — cada repositório é
escopado ao tenant resolvido a partir do PAT autenticador. O endpoint `_catalog`
está desativado por padrão.
:::

## Resolução de problemas

| Sintoma | Causa provável | Correção |
|---|---|---|
| `401 Unauthorized` no push | Não está logado ou o PAT expirou | Execute `docker login` novamente com um PAT atual |
| `denied: requested access to the resource is denied` | O PAT não tem escopo de escrita | Use um PAT com escopo de escrita de cache |
| `manifest unknown` no pull | A imagem nunca foi enviada para este tenant | Faça o push dela primeiro ou verifique o host/nome da referência |

Referência completa de erros: [Resolução de problemas](../troubleshooting.md).
