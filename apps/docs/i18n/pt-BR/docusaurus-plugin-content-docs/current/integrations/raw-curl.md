---
id: raw-curl
title: Uso de HTTP bruto (curl)
sidebar_position: 3
description: Usar o CoreLink diretamente com curl — para scripting, depuração e pipelines de CI que não usam uma ferramenta de build.
---

<!-- i18n:MT (pt-BR) — TMX-seeded machine-translation stub; replace with native-speaker translation before GA -->

> MT: Esta página está em tradução. A versão canônica em inglês é a fonte de verdade até a revisão por falante nativo (D+10).
>
> Canonical EN source: `docs/integrations/raw-curl.md`

# Uso de HTTP bruto (curl)

Esta página aborda o uso da API do CoreLink diretamente com `curl`. É útil para:

- Fazer scripting de uploads de artefatos em pipelines de release.
- Depurar problemas de autenticação ou de rede antes de configurar uma ferramenta de build.
- Qualquer toolchain que fale HTTP mas não use REAPI.

## Configuração de autenticação

```bash
export CORELINK_PAT="corelink_pat_XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"
export CORELINK_TENANT="acme-prod"
export CORELINK_BASE="https://corelink-api.humangr.com"
```

## Enviar um arquivo

O CAS nativo faz o endereçamento por conteúdo de cada blob pelo seu digest **BLAKE3** (hex
minúsculo), então calcule o digest com `b3sum` — não `sha256sum`. Instale-o com
`brew install b3sum` (macOS) ou `cargo install b3sum` / o pacote da sua distribuição
(Linux).

```bash
# 1. Compute the BLAKE3 digest
DIGEST=$(b3sum ./artifact.tar.gz | awk '{print $1}')
echo "Digest: $DIGEST"

# 2. Upload
curl -s -X PUT \
  -H "Authorization: Bearer $CORELINK_PAT" \
  -H "Content-Type: application/octet-stream" \
  --data-binary @./artifact.tar.gz \
  "$CORELINK_BASE/v1/cas/$CORELINK_TENANT/$DIGEST"
```

Em caso de sucesso, o servidor retorna **201 Created** (ou **200 OK** se o blob já
existia) e o corpo da resposta é o hex BLAKE3 armazenado — o mesmo valor que você
enviou na URL:

```text
af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262
```

## Baixar um arquivo

```bash
curl -s \
  -H "Authorization: Bearer $CORELINK_PAT" \
  "$CORELINK_BASE/v1/cas/$CORELINK_TENANT/$DIGEST" \
  -o ./artifact-downloaded.tar.gz

# Verify integrity
b3sum ./artifact-downloaded.tar.gz
# should match $DIGEST
```

## Verificar se um blob existe (requisição HEAD)

```bash
curl -s -I \
  -H "Authorization: Bearer $CORELINK_PAT" \
  "$CORELINK_BASE/v1/cas/$CORELINK_TENANT/$DIGEST"
```

- HTTP 200: o blob existe.
- HTTP 404: blob não encontrado.

## Enviar um diretório como um arquivo compactado

Útil para cachear diretórios de saída de build:

```bash
# Archive, compute digest, upload in one pipeline
tar -czf - ./dist/ \
  | tee >(b3sum | awk '{print $1}' > /tmp/digest.txt) \
  | curl -s -X PUT \
      -H "Authorization: Bearer $CORELINK_PAT" \
      -H "Content-Type: application/octet-stream" \
      --data-binary @- \
      "$CORELINK_BASE/v1/cas/$CORELINK_TENANT/$(cat /tmp/digest.txt)"

echo "Uploaded as $(cat /tmp/digest.txt)"
```

## Push + pull com script no GitHub Actions

```yaml
jobs:
  build:
    runs-on: ubuntu-latest
    env:
      CORELINK_PAT: ${{ secrets.CORELINK_PAT }}
      CORELINK_TENANT: acme-prod
    steps:
      - uses: actions/checkout@v4

      - name: Build
        run: make build

      - name: Push artifact to CoreLink
        run: |
          DIGEST=$(b3sum ./dist/app.bin | awk '{print $1}')
          curl -fsSL -X PUT \
            -H "Authorization: Bearer $CORELINK_PAT" \
            -H "Content-Type: application/octet-stream" \
            --data-binary @./dist/app.bin \
            "https://corelink-api.humangr.com/v1/cas/$CORELINK_TENANT/$DIGEST"
          echo "ARTIFACT_DIGEST=$DIGEST" >> $GITHUB_OUTPUT
        id: push

  deploy:
    needs: build
    runs-on: ubuntu-latest
    env:
      CORELINK_PAT: ${{ secrets.CORELINK_PAT }}
      CORELINK_TENANT: acme-prod
    steps:
      - name: Pull artifact from CoreLink
        run: |
          curl -fsSL \
            -H "Authorization: Bearer $CORELINK_PAT" \
            "https://corelink-api.humangr.com/v1/cas/$CORELINK_TENANT/${{ needs.build.outputs.ARTIFACT_DIGEST }}" \
            -o ./app.bin
          chmod +x ./app.bin
```

## Verificar se funcionou

```bash
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
# {"tenant_id":"acme-prod","token_prefix":"corelink","route_kind":"reapi_v1"}
```

Se o `tenant_id` corresponder ao seu tenant e não houver erros, você está totalmente autenticado.

## Armadilhas comuns

| Problema | Causa | Correção |
|---|---|---|
| `422 Unprocessable Entity` | O digest BLAKE3 na URL não corresponde aos bytes enviados (por exemplo, calculado antes do gzip) | Calcule o digest com `b3sum` a partir dos bytes exatos que estão sendo enviados |
| `curl: (22) The requested URL returned error: 401` | PAT não exportado ou incorreto | `echo $CORELINK_PAT` para verificar |
| Arquivo baixado corrompido | Usou `--output -` (stdout) canalizado para um arquivo enquanto o curl também escrevia o progresso no stdout | Sempre use a flag `-o <filename>` ou `-s` |
| Arquivo grande atinge o timeout | Timeout padrão do curl atingido | Adicione `--max-time 300` para artefatos grandes |

Referência completa de erros: [Resolução de problemas](../troubleshooting.md).
