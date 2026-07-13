---
id: turborepo
title: Turborepo-Integration
sidebar_position: 2
description: Konfigurieren Sie Turborepo so, dass CoreLink über TURBO_API und TURBO_TOKEN als Remote-Cache verwendet wird.
---

<!-- i18n:MT (de) — bootstrap MT-stub from EN source; replace with native-speaker translation before GA -->

> MT: Diese Seite wird übersetzt. Die englische Fassung gilt als verbindlich, bis ein Muttersprachler die Übersetzung freigibt (D+14).
>
> Canonical EN source: `docs/integrations/turborepo.md`

# Turborepo-Integration

CoreLink implementiert das `/v8/artifacts`-Protokoll des Vercel Remote Cache, sodass
Turborepo CoreLink als direkten Ersatz für den Remote-Cache von Vercel verwenden kann.

## Funktionsweise

Turborepo unterstützt benutzerdefinierte Remote-Caches über zwei Umgebungsvariablen:

- `TURBO_API` — der **reine Ursprung** des Remote-Cache-Servers. Turborepo
  hängt seinen eigenen `/v8/artifacts/...`-Pfad an — fügen Sie **nicht** selbst einen Pfad oder ein
  Tenant-Segment hinzu.
- `TURBO_TOKEN` — Ihr CoreLink-PAT, übergeben als `Authorization: Bearer`.

Ihr Tenant wird aus dem PAT aufgelöst, **nicht** aus der URL. Die `teamId`,
die Turborepo sendet, wird als logischer Sub-Namespace *innerhalb* Ihres
authentifizierten Tenants behandelt (Teams unter einem Tenant bleiben partitioniert) — sie ist keine
Sicherheitsgrenze und erscheint nicht in der Basis-URL.

## Konfiguration

### Option A: Umgebungsvariablen (empfohlen für CI)

```bash
export TURBO_API="https://corelink-api.humangr.com"
export TURBO_TOKEN="corelink_pat_XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"
```

Führen Sie Turborepo anschließend wie gewohnt aus (übergeben Sie ein `--team`-Label, damit Turborepo das Remote-
Caching aktiviert):

```bash
npx turbo run build --team=acme --token="$TURBO_TOKEN"
```

### Option B: `.turbo/config.json` (pro Repository)

```json
{
  "teamId": "acme",
  "apiUrl": "https://corelink-api.humangr.com"
}
```

Mit dieser Datei im Stammverzeichnis Ihres Repositorys liest Turborepo das Team-Label und die API-URL automatisch. Setzen Sie `TURBO_TOKEN` dennoch als Umgebungsvariable — committen Sie das Token nicht.

### Option C: Remote-Konfiguration in `turbo.json`

```json
{
  "$schema": "https://turbo.build/schema.json",
  "remoteCache": {
    "enabled": true
  }
}
```

Dies aktiviert das Remote-Caching; die URL und das Token stammen aus Umgebungsvariablen.

## GitHub-Actions-Beispiel

```yaml
- name: Build with Turborepo + CoreLink cache
  env:
    TURBO_API: https://corelink-api.humangr.com
    TURBO_TOKEN: ${{ secrets.CORELINK_PAT }}
  run: npx turbo run build test --team=acme --token="$TURBO_TOKEN"
```

Speichern Sie den PAT in `Settings → Secrets and variables → Actions` als `CORELINK_PAT`.

## Überprüfen, ob es funktioniert hat

Führen Sie nach der Konfiguration Ihre Pipeline zweimal aus. Beim zweiten Durchlauf sollte Turborepo Remote-Cache-Treffer melden:

```text
• Packages in scope: web, api, shared
• Running build in 3 packages
• Remote caching enabled

web:build  cache hit, replaying output...  0.8s
api:build  cache hit, replaying output...  0.6s
shared:build  cache hit, replaying output...  0.3s
```

Sie können außerdem bestätigen, dass das Token gültig ist:

```bash
curl -s \
  -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
# {"tenant_id":"acme-prod","token_prefix":"corelink","route_kind":"reapi_v1"}
```

## Fehlerbehebung

| Symptom | Wahrscheinliche Ursache | Lösung |
|---|---|---|
| `Remote caching disabled` | `TURBO_TOKEN` nicht gesetzt | Exportieren Sie `TURBO_TOKEN` in Ihrer Shell oder CI-Umgebung |
| `401`-Fehler in der Turborepo-Ausgabe | Falscher oder abgelaufener PAT | Regenerieren Sie den PAT im Admin-Dashboard |
| Cache-Misses bei jedem Durchlauf | `TURBO_API` hat ein zusätzliches Pfadsegment | `TURBO_API` muss der **reine Ursprung** `https://corelink-api.humangr.com` sein — ohne `/turbo`, `/v8` oder Tenant-Suffix |
| `400 Bad Request` beim Artefakt-PUT/GET | Fehlendes Team-Label | Übergeben Sie `--team=<label>` (oder setzen Sie `teamId` in `.turbo/config.json`) |

Vollständige Fehlerreferenz: [Fehlerbehebung](../troubleshooting.md).
