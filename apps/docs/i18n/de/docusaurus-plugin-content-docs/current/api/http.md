---
id: http
title: HTTP-API-Referenz
sidebar_position: 1
description: Authentifizierung, Endpunkte, Anfrage-/Antwortformate und Fehlercodes der CoreLink-REST-API.
---

<!-- i18n:MT (de) — bootstrap MT-stub from EN source; replace with native-speaker translation before GA -->

> MT: Diese Seite wird übersetzt. Die englische Fassung gilt als verbindlich, bis ein Muttersprachler die Übersetzung freigibt (D+14).
>
> Canonical EN source: `docs/api/http.md`

# HTTP-API-Referenz

Basis-URL: `https://corelink-api.humangr.com`

Alle Endpunkte erfordern HTTPS. HTTP wird nicht akzeptiert.

## Authentifizierung

Jede Anfrage muss einen `Authorization: Bearer <PAT>`-Header enthalten.

```bash
curl -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
```

Kein anderes Authentifizierungsschema (Basic, API-Key-Header, Query-Parameter) wird akzeptiert. Fehlt der Header oder ist er fehlerhaft, gibt die API `401` zurück.

## Endpunkte

### `GET /v1/users/me`

Gibt die Identität des in der Anfrage verwendeten PAT zurück.

**Anfrage**

```bash
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
```

**Antwort 200**

```json
{
  "tenant_id": "acme-prod",
  "token_prefix": "aZ3xQ1",
  "route_kind": "cas"
}
```

| Feld | Typ | Beschreibung |
|---|---|---|
| `tenant_id` | string | Der Tenant, auf den dieses PAT beschränkt ist. Entspricht dem Pfadsegment in CAS-/AC-URLs. |
| `token_prefix` | string | Eine 6-stellige, hash-abgeleitete Kennung für Log-/Rate-Limit-Korrelation — kein wörtliches Präfix Ihres Tokens. |
| `route_kind` | string | Immer `cas` für PATs der Datenebene. |

---

### `PUT /v1/cas/<tenant_id>/<blake3>`

Lädt einen Blob hoch. Der native CAS ist **BLAKE3**-basiert: Der BLAKE3-Digest im URL-Pfad muss mit dem BLAKE3 des Anfragekörpers übereinstimmen. Stimmt er nicht überein, gibt der Server `422 content hash mismatch` zurück. (Berechnen Sie ihn mit `b3sum` — **nicht** mit `sha256sum`.)

**Parameter**

| Name | In | Erforderlich | Beschreibung |
|---|---|---|---|
| `tenant_id` | path | ja | Ihr Tenant-Bezeichner. Muss mit dem Tenant des PAT übereinstimmen. |
| `blake3` | path | ja | Kleinbuchstaben-Hex-BLAKE3 der Blob-Bytes (64 Zeichen). |

**Header**

| Header | Erforderlich | Wert |
|---|---|---|
| `Authorization` | ja | `Bearer <PAT>` |
| `Content-Type` | empfohlen | `application/octet-stream` |
| `Content-Length` | empfohlen | Byte-Länge des Körpers |

**Anfrage**

```bash
DIGEST=$(b3sum ./output.tar.gz | awk '{print $1}')   # BLAKE3, not sha256

curl -s -X PUT \
  -H "Authorization: Bearer $CORELINK_PAT" \
  -H "Content-Type: application/octet-stream" \
  --data-binary @./output.tar.gz \
  "https://corelink-api.humangr.com/v1/cas/acme-prod/$DIGEST"
```

**Antwort 201** — der Körper gibt den gespeicherten BLAKE3-Hex zurück:

```json
{"hash": "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"}
```

**Antwort 409** — Blob existiert bereits (idempotent; kann gefahrlos ignoriert werden)

```json
{"error": "conflict", "message": "blob already exists"}
```

---

### `GET /v1/cas/<tenant_id>/<blake3>`

Lädt einen Blob per Digest herunter.

**Parameter**

| Name | In | Erforderlich | Beschreibung |
|---|---|---|---|
| `tenant_id` | path | ja | Ihr Tenant-Bezeichner. |
| `blake3` | path | ja | Kleinbuchstaben-Hex-BLAKE3. |

**Anfrage**

```bash
curl -s \
  -H "Authorization: Bearer $CORELINK_PAT" \
  "https://corelink-api.humangr.com/v1/cas/acme-prod/$DIGEST" \
  -o ./output-downloaded.tar.gz
```

**Antwort 200** — `Content-Type: application/octet-stream`, der Körper besteht aus Rohbytes.

**Antwort 404** — Blob nicht im CAS des Tenants.

---

### `GET /api/health`

Zustandsprüfung. Gibt `200 OK` mit `{"status": "ok"}` zurück, wenn der Dienst läuft. Keine Authentifizierung erforderlich.

```bash
curl -s https://corelink-api.humangr.com/api/health
# {"status":"ok"}
```

---

### Verwaltung von PATs (`GET`/`POST /v1/pats`, `DELETE /v1/pats/:pat_id`) — geplant, noch nicht live

Diese Routen sind für eine zukünftige Self-Service-PAT-Verwaltung (Liste,
Erstellung, Widerruf) spezifiziert, aber **heute nicht verdrahtet** — jeder
Aufruf liefert `404`. Der einzige aktive PAT-Erstellungsweg ist der
automatische Start-PAT des Registrierungsassistenten. Eine Admin-only,
nur lesende Oberfläche existiert, damit der Support die PATs eines Tenants
einsehen kann (`GET /v1/admin/tenants/{tenant_id}/pats`), erfordert aber
ein Admin-PAT und ist nicht mit einem normalen Kunden-Token aufrufbar.

Bis die Self-Service-PAT-Verwaltung verfügbar ist, schreiben Sie an
[support@humangr.com](mailto:support@humangr.com), um ein zusätzliches PAT
ausstellen oder eines widerrufen zu lassen.

---


## Fehlercodes

| HTTP-Status | Feld `error` | Bedeutung | Behebung |
|---|---|---|---|
| `400 Bad Request` | `bad_request` | Fehlerhafte Anfrage (ungültiges JSON, fehlendes Feld) | Anfragekörper prüfen |
| `401 Unauthorized` | `unauthorized` | Fehlendes oder ungültiges PAT | `Authorization`-Header prüfen |
| `403 Forbidden` | `forbidden` | Das PAT hat nicht den erforderlichen Scope oder es liegt eine Tenant-Abweichung vor | PAT-Scopes und Tenant im URL-Pfad prüfen |
| `404 Not Found` | `not_found` | Der Blob existiert nicht im CAS des Tenants | Vor dem Abrufen hochladen |
| `409 Conflict` | `conflict` | Blob existiert bereits (PUT) | Idempotent — kann gefahrlos ignoriert werden |
| `422 Unprocessable Entity` | `content hash mismatch` | BLAKE3 in der URL stimmt nicht mit dem Körper überein (z. B. haben Sie `sha256sum` verwendet) | Mit `b3sum` neu berechnen |
| `429 Too Many Requests` | `rate_limited` | Anfragerate überschritten | Zurückfahren und erneut versuchen; siehe `Retry-After`-Header |
| `503 Service Unavailable` | `audit_closed` | Der Auditzeitraum des Tenants ist geschlossen — Schreibvorgänge vorübergehend ausgesetzt | Support kontaktieren; Lesevorgänge funktionieren weiterhin |

Alle Fehlerantworten haben dieses Format:

```json
{
  "error": "not_found",
  "message": "blob af1349b9... not found in tenant acme-prod"
}
```

## Ratenbegrenzungen

Ratenbegrenzungen werden pro Tenant und pro Endpunktfamilie angewendet. Die Antwort enthält:

```
X-RateLimit-Limit: 1000
X-RateLimit-Remaining: 998
X-RateLimit-Reset: 1717000000
Retry-After: 60   (only on 429)
```

Standardgrenzen (können je nach Tarif geändert werden):

| Vorgang | Grenze |
|---|---|
| CAS-Lesevorgänge | 1 000 Anf./Min pro Tenant |
| CAS-Schreibvorgänge | 500 Anf./Min pro Tenant |
| Verwaltung (PAT-CRUD) | 60 Anf./Min pro Tenant |

Enterprise-Tarife haben höhere Grenzen. Kontaktieren Sie den Vertrieb für individuelle Grenzen.

## Paginierung

Listen-Endpunkte (Audit-Log, CAS-Auflistung) geben cursorbasierte Paginierung zurück:

```json
{
  "items": [...],
  "next_cursor": "eyJ...",
  "has_more": true
}
```

Übergeben Sie `?cursor=<next_cursor>`, um die nächste Seite abzurufen.
