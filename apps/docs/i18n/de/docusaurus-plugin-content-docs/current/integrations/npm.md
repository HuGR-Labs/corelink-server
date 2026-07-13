---
id: npm
title: npm-Registry-Spiegel
sidebar_position: 6
description: Verweisen Sie npm, pnpm, yarn oder bun als Caching-Spiegel vor der öffentlichen npm-Registry auf CoreLink.
---

<!-- i18n:MT (de) — bootstrap MT-stub from EN source; replace with native-speaker translation before GA -->

> MT: Diese Seite wird übersetzt. Die englische Fassung gilt als verbindlich, bis ein Muttersprachler die Übersetzung freigibt (D+14).
>
> Canonical EN source: `docs/integrations/npm.md`

# npm-Registry-Spiegel

CoreLink ist ein **Caching-Spiegel** vor `registry.npmjs.org`. Verweisen Sie
`npm` (oder `pnpm` / `yarn` / `bun`) auf CoreLink, und Paketmetadaten und Tarballs
werden in Ihrem Tenant zwischengespeichert, sodass wiederholte Installationen — besonders in der CI — schneller und
widerstandsfähiger gegen Upstream-Ausfälle sind. Tarballs werden im CAS Ihres Tenants gespeichert und
vor dem Zwischenspeichern per Integritätsprüfung gegen den `dist.shasum` des Herausgebers geprüft.

Dies ist ein **schreibgeschützter Spiegel** für `npm install`. Er hostet keine privaten
Pakete und akzeptiert kein `npm publish`.

## Voraussetzungen

- `npm` (oder ein kompatibler Client) installiert.
- Ein CoreLink-PAT (`corelink_pat_...`).
- Die UUID Ihres Tenants.

## `.npmrc` konfigurieren

Fügen Sie die Registry Ihres Tenants und ihr Auth-Token zu `.npmrc` hinzu (projektlokal oder
`~/.npmrc`):

```ini
registry=https://corelink-api.humangr.com/npm/<your-tenant-id>/
//corelink-api.humangr.com/npm/<your-tenant-id>/:_authToken=corelink_pat_XXXXXXXXXXXXXXXXXXXXXXXX
```

npm sendet das `_authToken` als `Authorization: Bearer <token>`, genau das,
was CoreLink erwartet. Installieren Sie anschließend wie gewohnt:

```bash
npm install
```

:::tip Halten Sie das Token aus dem Repository heraus
Committen Sie nur die `registry=`-Zeile. Stellen Sie die `_authToken`-Zeile aus einer
umgebungsspezifischen `~/.npmrc` oder einem CI-Secret bereit, damit der PAT nie committet wird.
:::

## Überprüfen, ob es funktioniert hat

Löschen Sie `node_modules` und installieren Sie neu; die zweite Installation sollte aus
CoreLink bedient werden:

```bash
rm -rf node_modules
npm install --loglevel http 2>&1 | grep corelink-api.humangr.com | head
```

Anfragen an `corelink-api.humangr.com/npm/...` im HTTP-Protokoll bestätigen, dass
npm den Spiegel verwendet.

## Fehlerbehebung

| Symptom | Wahrscheinliche Ursache | Lösung |
|---|---|---|
| `401 Unauthorized` | Fehlende oder fehlerhafte `_authToken`-Zeile | Host + Pfad der Token-Zeile müssen exakt mit `registry=` übereinstimmen, und das Token muss ein `corelink_pat_...`-PAT sein |
| Installationen greifen weiterhin auf `registry.npmjs.org` zu | `registry=` wurde nicht übernommen | Prüfen Sie den `.npmrc`-Geltungsbereich (Projekt vs. Benutzer) und führen Sie `npm config get registry` erneut aus |
| `EINTEGRITY` | Der Upstream-Tarball hat sich geändert | CoreLink prüft den `dist.shasum` und schlägt bei einer Abweichung fehlsicher fehl — wiederholen Sie es oder melden Sie das Upstream-Paket |

Vollständige Fehlerreferenz: [Fehlerbehebung](../troubleshooting.md).
