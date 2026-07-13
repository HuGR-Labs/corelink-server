---
id: raw-curl
title: Uso de HTTP sin procesar (curl)
sidebar_position: 3
description: Usar CoreLink directamente con curl — para scripting, depuración y pipelines de CI que no usan una herramienta de build.
---

<!-- i18n:MT (es-419) — TMX-seeded machine-translation stub; replace with native-speaker translation before GA -->

> MT: Esta página está en traducción. La versión canónica en inglés es la fuente de verdad hasta la revisión por hablante nativo (D+10).
>
> Canonical EN source: `docs/integrations/raw-curl.md`

# Uso de HTTP sin procesar (curl)

Esta página trata el uso de la API de CoreLink directamente con `curl`. Es útil para:

- Hacer scripting de subidas de artefactos en pipelines de release.
- Depurar problemas de autenticación o de red antes de conectar una herramienta de build.
- Cualquier toolchain que hable HTTP pero no use REAPI.

## Configuración de autenticación

```bash
export CORELINK_PAT="corelink_pat_XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"
export CORELINK_TENANT="acme-prod"
export CORELINK_BASE="https://corelink-api.humangr.com"
```

## Subir un archivo

El CAS nativo hace el direccionamiento por contenido de cada blob mediante su digest **BLAKE3** (hex
en minúsculas), así que calcule el digest con `b3sum` — no `sha256sum`. Instálelo con
`brew install b3sum` (macOS) o `cargo install b3sum` / el paquete de su distribución
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

En caso de éxito, el servidor devuelve **201 Created** (o **200 OK** si el blob ya
existía) y el cuerpo de la respuesta es el hex BLAKE3 almacenado — el mismo valor que
envió en la URL:

```text
af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262
```

## Descargar un archivo

```bash
curl -s \
  -H "Authorization: Bearer $CORELINK_PAT" \
  "$CORELINK_BASE/v1/cas/$CORELINK_TENANT/$DIGEST" \
  -o ./artifact-downloaded.tar.gz

# Verify integrity
b3sum ./artifact-downloaded.tar.gz
# should match $DIGEST
```

## Comprobar si un blob existe (solicitud HEAD)

```bash
curl -s -I \
  -H "Authorization: Bearer $CORELINK_PAT" \
  "$CORELINK_BASE/v1/cas/$CORELINK_TENANT/$DIGEST"
```

- HTTP 200: el blob existe.
- HTTP 404: blob no encontrado.

## Subir un directorio como un archivo comprimido

Útil para almacenar en caché directorios de salida de build:

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

## Push + pull con script en GitHub Actions

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

## Verificar que funcionó

```bash
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
# {"tenant_id":"acme-prod","token_prefix":"corelink","route_kind":"reapi_v1"}
```

Si `tenant_id` coincide con su tenant y no hay errores, está completamente autenticado.

## Errores comunes

| Problema | Causa | Solución |
|---|---|---|
| `422 Unprocessable Entity` | El digest BLAKE3 de la URL no coincide con los bytes subidos (por ejemplo, calculado antes del gzip) | Calcule el digest con `b3sum` a partir de los bytes exactos que se están subiendo |
| `curl: (22) The requested URL returned error: 401` | PAT no exportado o incorrecto | `echo $CORELINK_PAT` para verificar |
| Archivo descargado corrupto | Usó `--output -` (stdout) canalizado a un archivo mientras curl también escribía el progreso en stdout | Use siempre la flag `-o <filename>` o `-s` |
| Un archivo grande agota el tiempo de espera | Se alcanzó el timeout predeterminado de curl | Agregue `--max-time 300` para artefactos grandes |

Referencia completa de errores: [Resolución de problemas](../troubleshooting.md).
