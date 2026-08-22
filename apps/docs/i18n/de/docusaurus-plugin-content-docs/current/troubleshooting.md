---
id: troubleshooting
title: Fehlerbehebung
sidebar_position: 10
description: Häufige Fehlercodes, ihre Bedeutung und wie man sie behebt.
---

<!-- i18n:MT (de) — bootstrap MT-stub from EN source; replace with native-speaker translation before GA -->

> MT: Diese Seite wird übersetzt. Die englische Fassung gilt als verbindlich, bis ein Muttersprachler die Übersetzung freigibt (D+14).
>
> Canonical EN source: `docs/troubleshooting.md`

# Fehlerbehebung

## Fehlerreferenz

### `401 Unauthorized`

**Bedeutung**: Der `Authorization`-Header fehlt, ist fehlerhaft, oder das PAT wurde widerrufen.

**Diagnose**:

```bash
# Confirm the PAT is set in your shell
echo $CORELINK_PAT

# Test the PAT directly
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
```

Wenn `/v1/users/me` `401` zurückgibt, ist das PAT ungültig. Mögliche Ursachen:

1. Das PAT wurde im Admin-Dashboard widerrufen.
2. Das PAT wurde nie exportiert (`export CORELINK_PAT=...` vs. `CORELINK_PAT=...`).
3. Sie verwenden ein Test-PAT (`clk_test_...`) gegen die Produktions-API.

**Behebung**: Erzeugen Sie ein neues PAT im Admin-Dashboard. Speichern Sie es in einem Secret-Manager, bevor Sie den Tab schließen.

---

### `403 Forbidden`

**Bedeutung**: Das PAT ist gültig, hat aber keine Berechtigung für den angeforderten Vorgang.

Zwei Unterfälle:

1. **Scope-Abweichung**: Das PAT wurde nur mit `cas:read` erstellt, aber Sie versuchen zu schreiben.
2. **Tenant-Abweichung**: Das PAT gehört zu `acme-prod`, aber die Anfrage-URL enthält `acme-staging`.

**Diagnose**:

```bash
# Confirm your tenant
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
# Check "tenant_id" in the response

# Confirm tenant in URL matches
echo $CORELINK_TENANT
```

**Behebung**: Erstellen Sie ein PAT mit den korrekten Scopes oder korrigieren Sie die Umgebungsvariable `CORELINK_TENANT`.

---

### `404 Not Found`

**Bedeutung**: Der Blob mit dem angegebenen Digest existiert nicht im CAS des Tenants.

**Häufige Ursachen**:

- Sie lesen von einem anderen Tenant als dem, der hochgeladen hat.
- Der Blob wurde nie hochgeladen (häufig bei einem neuen Tenant oder einer neuen CI-Pipeline).
- Der Digest wurde falsch berechnet.

**Diagnose**:

```bash
# Verify the digest
sha256sum ./my-file.bin
# Compare to the hash you are requesting
```

**Behebung**: Laden Sie den Blob hoch, bevor Sie ihn abrufen. Bestätigen Sie, dass der Tenant in der URL mit dem hochladenden Tenant übereinstimmt.

---

### `422 Unprocessable Entity` (Hash-Abweichung)

**Bedeutung**: Der SHA-256 im URL-Pfad stimmt nicht mit dem SHA-256 des Anfragekörpers überein.

Dies ist ein clientseitiger Fehler. Der Server berechnet den Digest der empfangenen Bytes und vergleicht ihn mit dem Pfadsegment. Weichen sie ab, wird der Upload abgelehnt.

**Häufige Ursachen**:

- Der Digest wurde vor der Komprimierung berechnet (z. B. auf `file.tar` berechnet, dann `file.tar.gz` hochgeladen).
- Der Digest wurde auf einem partiellen Read berechnet.
- Multipart-Upload-Tooling, das Framing-Bytes hinzufügt.

**Behebung**:

```bash
# Always compute the digest from the exact bytes being sent
DIGEST=$(sha256sum ./artifact.tar.gz | awk '{print $1}')
curl -X PUT ... --data-binary @./artifact.tar.gz \
  ".../v1/cas/$CORELINK_TENANT/$DIGEST"
```

---

### `429 Too Many Requests`

**Bedeutung**: Sie haben die Ratenbegrenzung für diesen Vorgang überschritten.

Die Antwort enthält einen `Retry-After`-Header, der angibt, wie viele Sekunden zu warten sind.

**Behebung**:

```bash
# Parse the retry delay from the response
curl -si ... | grep -i retry-after
```

Für dauerhaft hohen Durchsatz kontaktieren Sie den Support, um die Grenzen Ihres Tarifs anzuheben.

---

### `503 Service Unavailable` mit `audit_closed`

**Bedeutung**: Der Auditzeitraum Ihres Tenants wurde durch einen Admin-Vorgang geschlossen. Schreibvorgänge sind ausgesetzt, bis der Auditzeitraum wieder geöffnet wird.

Dies wird typischerweise während eines Compliance-Audits oder einer Abrechnungsstreitigkeit ausgelöst. Lesevorgänge (Downloads) bleiben verfügbar.

**Behebung**: Kontaktieren Sie den CoreLink-Support unter [support@humangr.com](mailto:support@humangr.com) mit Ihrer Tenant-ID.

---

## Bazel-spezifische Probleme

### `remote_cache: UNAUTHENTICATED`

Der `authorization`-Header wurde nicht weitergeleitet. Prüfen Sie:

1. `CORELINK_PAT` ist in der Shell exportiert, in der Bazel läuft.
2. Ihre `.bazelrc` verwendet `${CORELINK_PAT}` (Shell-Expansion) und keinen literalen Platzhalter.

### `remote_cache: PERMISSION_DENIED`

Tenant-Abweichung. Prüfen Sie, ob `x-corelink-tenant` in `.bazelrc` mit `tenant_id` aus `/v1/users/me` übereinstimmt.

### Alle Aktionen verfehlen bei wiederholten Builds

`--remote_upload_local_results` steht auf `false`. Fügen Sie hinzu:

```text
build --remote_upload_local_results=true
```

---

## Turborepo-spezifische Probleme

### `Remote caching disabled`

`TURBO_TOKEN` ist nicht gesetzt. In Ihrer Shell oder CI-Umgebung:

```bash
export TURBO_TOKEN="$CORELINK_PAT"
```

### Cache-Misses bei jedem Turborepo-Lauf

Prüfen Sie, ob `TURBO_API` das korrekte Tenant-Suffix enthält:

```bash
echo $TURBO_API
# should be: https://corelink-api.humangr.com/turbo/v8/acme-prod
```

---

## Selbstdiagnose-Checkliste

Führen Sie diese der Reihe nach aus:

```bash
# 1. Network reachability
curl -s https://corelink-api.humangr.com/api/health
# {"status":"ok"}

# 2. PAT validity
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
# {"tenant_id":"...","token_prefix":"clk_live","route_kind":"cas"}

# 3. Write a test blob
echo "healthcheck" > /tmp/cl-test.txt
DIGEST=$(sha256sum /tmp/cl-test.txt | awk '{print $1}')
curl -s -X PUT \
  -H "Authorization: Bearer $CORELINK_PAT" \
  --data-binary @/tmp/cl-test.txt \
  "https://corelink-api.humangr.com/v1/cas/$CORELINK_TENANT/$DIGEST"
# {"hash":"sha256:<digest>"}

# 4. Read it back
curl -s \
  -H "Authorization: Bearer $CORELINK_PAT" \
  "https://corelink-api.humangr.com/v1/cas/$CORELINK_TENANT/$DIGEST"
# healthcheck
```

Wenn alle vier Schritte erfolgreich sind, funktioniert CoreLink. Jeder Fehler vor Schritt 4 bedeutet, dass das Problem in der Konfiguration Ihres Build-Tools liegt, nicht in CoreLink.

## Hilfe erhalten

- GitHub Issues: [github.com/HumanGuardrail/corelink-server/issues](https://github.com/HumanGuardrail/corelink-server/issues)
- E-Mail-Support: [support@humangr.com](mailto:support@humangr.com)

Wenn Sie eine Support-Anfrage stellen, fügen Sie die Ausgabe der obigen Schritte 1 bis 4 sowie Ihre Tenant-ID bei.
