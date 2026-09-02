---
id: homebrew
title: Homebrew-Bottle-Spiegel
sidebar_position: 8
description: Der sichere authentifizierte Homebrew-Bottle-Spiegel.
---

# Homebrew-Bottle-Spiegel

:::caution Sicherer authentifizierter Spiegelpfad
Modernes Homebrew (der Standard `install-from-API`, Homebrew 4.x und neuer)
kann seine `ghcr.io`-Bottle-URLs über `HOMEBREW_ARTIFACT_DOMAIN` umschreiben.
Der CoreLink-Endpunkt `/brew/<tenant>` akzeptiert das resultierende Bearer-PAT
und liest die Bottle serverseitig vom festen Upstream `ghcr.io`. Diese Seite
bleibt erhalten, weil sowohl der authentifizierte als auch der normale
Homebrew-Ablauf unterstützt werden. Tap-Quellen, Casks (`.dmg`/`.pkg`) und
`brew bottle` gehören hier nicht zum Umfang.

Die Einstellung ohne Fallback ist zwingend. Ohne sie versucht Homebrew bei
einem Spiegel-Fehler erneut die ursprüngliche `ghcr.io`-URL und kann das Bearer
an GitHub statt an CoreLink senden. Verwenden Sie niemals die alte Anleitung
ohne diese Sperre.
:::

<!-- WP-B161-AUTH-NO-FALLBACK-20260901: der authentifizierte Spiegel ist auf CoreLink festgelegt. -->

## Sicherer Weg: authentifizierter CoreLink-Spiegel

Beziehen Sie ein echtes CoreLink-PAT aus einem Secret-Manager und stellen Sie es
nur als `CORELINK_PAT` in der Shell bereit, in der Homebrew läuft. Fügen Sie
kein Token in Dokumente, die Shell-Historie oder Logs ein. Setzen Sie alle drei
Variablen gemeinsam:

```bash
export HOMEBREW_ARTIFACT_DOMAIN="https://corelink-api.humangr.com/brew/<your-tenant-id>"
export HOMEBREW_ARTIFACT_DOMAIN_NO_FALLBACK=1
export HOMEBREW_DOCKER_REGISTRY_TOKEN="$CORELINK_PAT"
brew install jq
```

Homebrew behält seine GitHub-Packages-Strategie für die `ghcr.io`-Bottle bei,
schreibt die URL auf die Artefaktdomäne um und macht aus
`HOMEBREW_DOCKER_REGISTRY_TOKEN` den Header `Authorization: Bearer <token>`.
Mit `HOMEBREW_ARTIFACT_DOMAIN_NO_FALLBACK=1` wird dieser Bearer nur an die
CoreLink-Domäne gesendet; ein Spiegel-Fehler wird zum Fehler statt zu einem
Retry bei `ghcr.io`. Der CoreLink-Adapter authentifiziert den Bearer und ruft
den Inhalt serverseitig von `ghcr.io` ab.

Ersetzen Sie die Artefaktdomäne nicht durch `HOMEBREW_BOTTLE_DOMAIN`. Dieser
ältere Flat-File-Override wählt nicht die authentifizierte GitHub-Packages-
Strategie von Homebrew und kann den für `/brew` nötigen Bearer nicht liefern.

Für einen reinen Download-Test setzen Sie dieselben drei Variablen und führen
Sie Folgendes aus:

```bash
brew fetch --force jq
```

Der öffentliche Weg ohne Konfiguration bleibt ebenfalls gültig: Entfernen Sie
alle drei CoreLink-Variablen, dann lädt Homebrew direkt vom Upstream.

## Warum die alte Spiegel-Anleitung entfernt wurde

Die frühere Anleitung setzte `HOMEBREW_ARTIFACT_DOMAIN` und
`HOMEBREW_DOCKER_REGISTRY_TOKEN`, ließ aber
`HOMEBREW_ARTIFACT_DOMAIN_NO_FALLBACK` weg. Das war unsicher: Bei einem
Spiegel-Fehler konnte Homebrew `ghcr.io` mit dem CoreLink-Bearer erneut
versuchen. Ein CoreLink-PAT darf niemals an den Upstream gehen; sperren Sie
den Fallback, bevor Sie das Token setzen.

Der Statuscode unterscheidet die beiden bei der Prüfung beobachteten Fälle:

| Prüfung | Ergebnis | Bedeutung |
|---|---|---|
| Keine CoreLink-Variablen | `brew` beendet sich mit `0` | Der direkte Upstream-Pfad funktioniert. |
| Artefaktdomäne ohne Zugangsdaten | `401 Unauthorized` | Der Spiegel erhielt keinen Bearer; verwenden Sie den PAT nur im gesperrten Spiegelpfad. |
| Artefaktdomäne mit Token und ohne Fallback | `Authorization: Bearer <token>` bei CoreLink | Der authentifizierte Spiegelvertrag ist aktiv. |
| Token ohne Artefaktdomäne oder ohne Fallback-Sperre | `403 Forbidden` kann von `ghcr.io` kommen | Unsichere Konfiguration; Block entfernen und sicher neu setzen. |

Der Unterschied zwischen `401` und `403` ist ein Beleg für die Übertragung von
Zugangsdaten, keine Lösung. Setzen Sie kein Token, um einen `401` in einen
`403` umzuwandeln, und senden Sie niemals ein CoreLink-PAT an `ghcr.io`.

## Fehlerbehebung

| Symptom | Bedeutung | Maßnahme |
|---|---|---|
| `brew install` funktioniert ohne CoreLink-Variablen | Homebrew verwendet den direkten Upstream-Pfad | Verwenden Sie den Ablauf ohne Konfiguration. |
| `401 Unauthorized` von der CoreLink-Domäne | Bearer fehlt oder ist ungültig | Secret-Quelle und PAT-Scope prüfen; keinen Fallback hinzufügen. |
| `403 Forbidden` von `ghcr.io` | Token wurde an den Upstream gesendet | Stoppen, Token entfernen und den Block mit Fallback-Sperre neu setzen. |
| Eigene Domäne ohne `HOMEBREW_ARTIFACT_DOMAIN_NO_FALLBACK=1` | Spiegel-Fehler kann auf `ghcr.io` zurückfallen | Konfiguration als unsicher behandeln, bis die Sperre vorhanden ist. |

Für eine unterstützte Cache-Integration lesen Sie [Fehlerbehebung](../troubleshooting.md)
und wählen Sie eine der oben genannten Integrationen.
