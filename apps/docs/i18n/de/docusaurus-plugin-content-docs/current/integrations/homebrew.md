---
id: homebrew
title: Homebrew-Bottle-Spiegel
sidebar_position: 8
description: Verweisen Sie Homebrew auf CoreLink, um Bottle-Downloads in Ihrem Tenant zwischenzuspeichern.
---

<!-- i18n:MT (de) — bootstrap MT-stub from EN source; replace with native-speaker translation before GA -->

> MT: Diese Seite wird übersetzt. Die englische Fassung gilt als verbindlich, bis ein Muttersprachler die Übersetzung freigibt (D+14).
>
> Canonical EN source: `docs/integrations/homebrew.md`

# Homebrew-Bottle-Spiegel

CoreLink speichert **Homebrew-Bottles** zwischen (die vorkompilierten `.tar.gz`-Binärdateien, die `brew
install` herunterlädt). Bei einem Cache-Treffer wird die Bottle aus dem CAS Ihres Tenants bedient;
bei einem Fehlschlag holt CoreLink sie vom Upstream (`ghcr.io`), speichert sie zwischen und streamt
sie zurück. Das beschleunigt wiederholte Installationen über Maschinen hinweg und in der CI.

Dies ist ein Spiegel für den **Lesepfad** von `brew install`. Tap-Quellen, Casks
(`.dmg`/`.pkg`) und `brew bottle` sind nicht im Umfang enthalten.

## Voraussetzungen

- Homebrew installiert.
- Ein CoreLink-PAT (`corelink_pat_...`).
- Die UUID Ihres Tenants.

## Konfigurieren

Homebrew hängt nur dann einen `Authorization`-Header an, wenn der Bottle-Host über
`HOMEBREW_ARTIFACT_DOMAIN` erreicht wird (was die authentifizierte GitHub-Packages-Download-Strategie
von Homebrew beibehält). Setzen Sie die Artefaktdomäne auf den Pfad Ihres Tenants
und übergeben Sie den PAT über `HOMEBREW_DOCKER_REGISTRY_TOKEN`:

```bash
export HOMEBREW_ARTIFACT_DOMAIN="https://corelink-api.humangr.com/brew/<your-tenant-id>"
export HOMEBREW_DOCKER_REGISTRY_TOKEN="corelink_pat_XXXXXXXXXXXXXXXXXXXXXXXX"
brew install <formula>
```

Homebrew wandelt `HOMEBREW_DOCKER_REGISTRY_TOKEN` bei jedem Bottle-Download in
`Authorization: Bearer corelink_pat_...` um, was CoreLink authentifiziert.

:::warning Verwenden Sie `HOMEBREW_ARTIFACT_DOMAIN`, nicht `HOMEBREW_BOTTLE_DOMAIN`
Eine bloße `HOMEBREW_BOTTLE_DOMAIN` wählt die einfache Download-Strategie von Homebrew,
die **keinen** Authentifizierungs-Header sendet — sie kann sich also nicht gegenüber CoreLink authentifizieren und
jeder Download schlägt mit 401 fehl. `HOMEBREW_ARTIFACT_DOMAIN` ist erforderlich.
:::

## Überprüfen, ob es funktioniert hat

Installieren Sie eine kleine Formula zweimal auf verschiedenen Maschinen (oder leeren Sie den lokalen
Download-Cache zwischen den Durchläufen). Die zweite Installation zieht die zwischengespeicherte Bottle von
CoreLink:

```bash
brew install --verbose jq 2>&1 | grep corelink-api.humangr.com | head
```

Anfragen an `corelink-api.humangr.com/brew/...` bestätigen, dass Homebrew den
Spiegel verwendet.

## Fehlerbehebung

| Symptom | Wahrscheinliche Ursache | Lösung |
|---|---|---|
| `401 Unauthorized` bei jedem Download | `HOMEBREW_BOTTLE_DOMAIN` verwendet (kein Authentifizierungs-Header) | Wechseln Sie zu `HOMEBREW_ARTIFACT_DOMAIN` |
| `401` bei gesetzter Artefaktdomäne | Token fehlt oder ist fehlerhaft | Setzen Sie `HOMEBREW_DOCKER_REGISTRY_TOKEN=corelink_pat_...` |
| `403 Forbidden` | PAT auf einen anderen Tenant beschränkt | Prüfen Sie, ob der `<tenant>` in der Domäne mit dem Tenant Ihres PAT übereinstimmt |

Vollständige Fehlerreferenz: [Fehlerbehebung](../troubleshooting.md).
