---
id: raw-curl
title: Rohe HTTP-Nutzung (curl)
sidebar_position: 3
description: CoreLink direkt mit curl verwenden — für Scripting, Debugging und CI-Pipelines, die kein Build-Tool nutzen.
---

<!-- i18n:MT (de) — bootstrap MT-stub from EN source; replace with native-speaker translation before GA -->

> MT: Diese Seite wird übersetzt. Die englische Fassung gilt als verbindlich, bis ein Muttersprachler die Übersetzung freigibt (D+14).
>
> Canonical EN source: `docs/integrations/raw-curl.md`

# Rohe HTTP-Nutzung (curl)

Diese Seite behandelt die direkte Nutzung der CoreLink-API mit `curl`. Sie ist nützlich für:

- Das Scripting von Artefakt-Uploads in Release-Pipelines.
- Das Debuggen von Authentifizierungs- oder Netzwerkproblemen, bevor ein Build-Tool angebunden wird.
- Jede Toolchain, die HTTP spricht, aber kein REAPI verwendet.

## Authentifizierungs-Setup

```bash
export CORELINK_PAT="corelink_pat_XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"
export CORELINK_TENANT="acme-prod"
export CORELINK_BASE="https://corelink-api.humangr.com"
```

## Eine Datei hochladen

Der native CAS adressiert jeden Blob per Inhalt über seinen **BLAKE3**-Digest (hex in
Kleinbuchstaben), berechnen Sie den Digest also mit `b3sum` — nicht `sha256sum`. Installieren Sie es mit
`brew install b3sum` (macOS) oder `cargo install b3sum` / dem Paket Ihrer Distribution
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

Bei Erfolg gibt der Server **201 Created** zurück (oder **200 OK**, falls der Blob bereits
existierte), und der Antwort-Body ist der gespeicherte BLAKE3-Hex — derselbe Wert, den Sie
in der URL gesendet haben:

```text
af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262
```

## Eine Datei herunterladen

```bash
curl -s \
  -H "Authorization: Bearer $CORELINK_PAT" \
  "$CORELINK_BASE/v1/cas/$CORELINK_TENANT/$DIGEST" \
  -o ./artifact-downloaded.tar.gz

# Verify integrity
b3sum ./artifact-downloaded.tar.gz
# should match $DIGEST
```

## Prüfen, ob ein Blob existiert (HEAD-Anfrage)

```bash
curl -s -I \
  -H "Authorization: Bearer $CORELINK_PAT" \
  "$CORELINK_BASE/v1/cas/$CORELINK_TENANT/$DIGEST"
```

- HTTP 200: Der Blob existiert.
- HTTP 404: Blob nicht gefunden.

## Ein Verzeichnis als Archiv hochladen

Nützlich zum Zwischenspeichern von Build-Ausgabeverzeichnissen:

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

## Gescriptetes Push + Pull in GitHub Actions

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

## Überprüfen, ob es funktioniert hat

```bash
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
# {"tenant_id":"acme-prod","token_prefix":"corelink","route_kind":"reapi_v1"}
```

Wenn `tenant_id` mit Ihrem Tenant übereinstimmt und keine Fehler auftreten, sind Sie vollständig authentifiziert.

## Häufige Fallstricke

| Problem | Ursache | Lösung |
|---|---|---|
| `422 Unprocessable Entity` | Der BLAKE3-Digest in der URL stimmt nicht mit den hochgeladenen Bytes überein (z. B. vor dem Gzip berechnet) | Berechnen Sie den Digest mit `b3sum` aus genau den Bytes, die hochgeladen werden |
| `curl: (22) The requested URL returned error: 401` | PAT nicht exportiert oder falsch | `echo $CORELINK_PAT` zur Überprüfung |
| Beschädigte heruntergeladene Datei | `--output -` (stdout) in eine Datei umgeleitet, während curl den Fortschritt ebenfalls nach stdout schrieb | Verwenden Sie immer `-o <filename>` oder die `-s`-Flag |
| Große Datei überschreitet das Zeitlimit | Standard-Timeout von curl erreicht | Fügen Sie für große Artefakte `--max-time 300` hinzu |

Vollständige Fehlerreferenz: [Fehlerbehebung](../troubleshooting.md).
