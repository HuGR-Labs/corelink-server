---
id: quickstart
title: 5-minütiger Quickstart
sidebar_position: 2
description: Registrieren Sie sich, installieren Sie die CLI, laden Sie Ihren ersten Blob hoch und wieder herunter. In unter 5 Minuten ab einer frischen Shell.
---

<!-- i18n:MT (de) — bootstrap MT-stub from EN source; replace with native-speaker translation before GA -->

> MT: Diese Seite wird übersetzt. Die englische Fassung gilt als verbindlich, bis ein Muttersprachler die Übersetzung freigibt (D+14).
>
> Canonical EN source: `docs/quickstart.md`

# 5-minütiger Quickstart

Ziel: authentifiziert, erster Push und Pull, verifiziert in unter 5 Minuten.

:::note CLI kommt bald
`corelink-cli` befindet sich in aktiver Entwicklung (Stream 1.1). Alle folgenden Beispiele funktionieren **heute** mit `curl`. Sobald die CLI erscheint, werden die entsprechenden `corelink`-Befehle daneben angezeigt.
:::

## Schritt 1 — Beschaffen Sie sich ein PAT

1. Registrieren Sie sich unter [humangr.com/corelink/sign-up](https://humangr.com/corelink/sign-up).
2. Nachdem der zweistufige Onboarding-Assistent abgeschlossen ist, wird Ihr Mandant bereitgestellt und ein Start-PAT **genau einmal** auf dem Willkommensbildschirm angezeigt.
3. Kopieren Sie das PAT und speichern Sie es in einem Secret-Manager (1Password, AWS Secrets Manager, GitHub-Actions-Secret — alles außer Klartext). Es wird nie wieder angezeigt.

Ihr PAT sieht so aus:

```text
corelink_pat_01ARZ3NDEKTSV4RRFFQ69G5FAV.4pT7q1yZ9vX2wL8cR5nB3sD6fH0jK1mQ8aV2eS.7bY4tN9oL2x
```

(EN note: `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>` — `<env>` is `pat` for a user-issued token; `ci`/`ro` exist for CI and read-only tokens.)

Exportieren Sie es für die folgenden Beispiele:

```bash
export CORELINK_PAT="corelink_pat_01ARZ3NDEKTSV4RRFFQ69G5FAV.4pT7q1yZ9vX2wL8cR5nB3sD6fH0jK1mQ8aV2eS.7bY4tN9oL2x"
export CORELINK_TENANT="your-tenant-id"   # shown on the welcome screen
```

## Schritt 2 — Überprüfen Sie Ihre Anmeldedaten

```bash
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
```

Erwartete Antwort:

```json
{
  "tenant_id": "your-tenant-id",
  "token_prefix": "aZ3xQ1",
  "route_kind": "cas"
}
```

Wenn Sie `401 Unauthorized` erhalten, ist das PAT falsch oder abgelaufen — erzeugen Sie ein neues über das Admin-Dashboard.

## Schritt 3 — Laden Sie einen Blob hoch

Das native CAS adressiert jeden Blob inhaltsbasiert anhand seines **BLAKE3**-Digests (Hex in Kleinbuchstaben) — berechnen Sie ihn mit `b3sum`, **nicht** mit `sha256sum` (`brew install b3sum` oder `cargo install b3sum`). Laden Sie dann hoch:

```bash
# Compute the BLAKE3 digest
DIGEST=$(b3sum ./my-artifact.bin | awk '{print $1}')

# Upload
curl -s -X PUT \
  -H "Authorization: Bearer $CORELINK_PAT" \
  -H "Content-Type: application/octet-stream" \
  --data-binary @./my-artifact.bin \
  "https://corelink-api.humangr.com/v1/cas/$CORELINK_TENANT/$DIGEST"
```

Erwartete Antwort (HTTP 201) — der Body gibt das gespeicherte BLAKE3-Hex zurück:

```json
{"hash": "<blake3-hex>"}
```

> Wenn Sie den Digest mit `sha256sum` berechnen, schlägt der Upload mit **422 content hash
> mismatch** fehl — der Server hasht den Body erneut mit BLAKE3 und er stimmt nicht mit einem
> SHA-256-URL-Digest überein.

## Schritt 4 — Laden Sie den Blob wieder herunter

```bash
curl -s \
  -H "Authorization: Bearer $CORELINK_PAT" \
  "https://corelink-api.humangr.com/v1/cas/$CORELINK_TENANT/$DIGEST" \
  -o ./my-artifact-downloaded.bin
```

Überprüfen Sie, ob die Bytes identisch sind:

```bash
diff my-artifact.bin my-artifact-downloaded.bin && echo "match"
```

## Schritt 5 — Binden Sie Ihr Build-Werkzeug an

Sobald Sie ein funktionierendes PAT haben, binden Sie Ihr Build-Werkzeug an:

- **Bazel** → [Bazel-Integrationsleitfaden](./integrations/bazel.md)
- **Turborepo** → [Turborepo-Integrationsleitfaden](./integrations/turborepo.md)
- **Rohes HTTP / Scripting** → [Beispiele mit rohem curl](./integrations/raw-curl.md)

## Fehlerbehebung

| Fehler | Ursache | Behebung |
|---|---|---|
| `401 Unauthorized` | Ungültiges oder abgelaufenes PAT | Über das Admin-Dashboard neu erzeugen |
| `403 Forbidden` | PAT auf einen anderen Mandanten beschränkt | Prüfen Sie, ob `CORELINK_TENANT` mit dem Mandanten Ihres PAT übereinstimmt |
| `404 Not Found` beim GET | Blob noch nicht hochgeladen | Erst pushen, dann pullen |
| `422 Unprocessable Entity` | SHA-256 in der URL stimmt nicht mit dem Body überein | Digest aus den tatsächlichen Datei-Bytes neu berechnen |

Vollständige Fehlerreferenz: [Fehlerbehebung](./troubleshooting.md).
