---
id: sccache-cargo
title: sccache-Integration (Rust / cargo)
sidebar_position: 4
description: Verwenden Sie CoreLink als sccache-WebDAV-Build-Cache, damit cargo-Builds kompilierte Artefakte über Maschinen und CI hinweg teilen.
---

<!-- i18n:MT (de) — bootstrap MT-stub from EN source; replace with native-speaker translation before GA -->

> MT: Diese Seite wird übersetzt. Die englische Fassung gilt als verbindlich, bis ein Muttersprachler die Übersetzung freigibt (D+14).
>
> Canonical EN source: `docs/integrations/sccache-cargo.md`

# sccache-Integration (Rust / cargo)

[sccache](https://github.com/mozilla/sccache) ist ein Compiler-Cache. Wenn Sie
`RUSTC_WRAPPER=sccache` setzen, wird jeder `rustc`-Aufruf zwischengespeichert — sodass ein `cargo build`
kompilierte Artefakte wiederverwendet, die auf einer anderen Maschine oder in einem früheren CI-Durchlauf erzeugt wurden.

CoreLink stellt ein sccache-**WebDAV**-Storage-Backend unter
`/cargo/<tenant>` bereit, gestützt auf denselben inhaltsadressierbaren Speicher pro Tenant, den der
native CAS verwendet. Sowohl sccache als auch CoreLink verschlüsseln Artefakte per **BLAKE3**, sodass
keine Digest-Übersetzung nötig ist.

## Voraussetzungen

- `sccache` installiert (`cargo install sccache` oder das Paket Ihrer Distribution).
- Ein CoreLink-PAT (`corelink_pat_...`) mit Lese- und Schreibberechtigung für den Cache.
- Die UUID Ihres Tenants.

## Konfigurieren

Verweisen Sie das WebDAV-Backend von sccache auf den Pfad Ihres Tenants und übergeben Sie den PAT als Bearer-
Token:

```bash
export SCCACHE_WEBDAV_ENDPOINT="https://corelink-api.humangr.com/cargo/<your-tenant-id>"
export SCCACHE_WEBDAV_TOKEN="corelink_pat_XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"
export RUSTC_WRAPPER=sccache
```

Bauen Sie anschließend wie gewohnt:

```bash
cargo build --release
```

sccache stellt `GET`-, `PUT`- und `HEAD`-Anfragen an
`<SCCACHE_WEBDAV_ENDPOINT>/<key>`; CoreLink authentifiziert den Bearer-PAT,
löst daraus Ihren Tenant auf und bedient oder speichert jedes Artefakt im CAS Ihres
Tenants. Ein `GET`/`HEAD`-Fehlschlag gibt 404 zurück, und sccache greift auf lokales
Kompilieren zurück (und macht dann ein `PUT` des Ergebnisses).

:::note Der Tenant stammt aus dem PAT
Der `<tenant>` im Endpunkt wird nur für das Anfrage-Routing verwendet; der
maßgebliche Tenant wird aus dem PAT aufgelöst und serverseitig erneut verifiziert. Ein PAT
kann nur den Cache seines eigenen Tenants lesen und beschreiben.
:::

## Überprüfen, ob es funktioniert hat

Führen Sie einen Build zweimal aus (leeren Sie zuerst den lokalen sccache, damit der zweite Build auf
CoreLink zugreifen muss):

```bash
cargo clean
cargo build --release        # cold — compiles and PUTs artifacts
cargo clean
cargo build --release        # warm — should read from CoreLink
sccache --show-stats
```

`sccache --show-stats` meldet die Cache-Trefferquote und die WebDAV-Backend-URL.
Ein von null verschiedener Wert bei "Cache hits" im zweiten Build bestätigt, dass CoreLink die
Artefakte bereitgestellt hat.

## CI-Beispiel (GitHub Actions)

```yaml
- name: Build with sccache + CoreLink
  env:
    RUSTC_WRAPPER: sccache
    SCCACHE_WEBDAV_ENDPOINT: https://corelink-api.humangr.com/cargo/acme-prod
    SCCACHE_WEBDAV_TOKEN: ${{ secrets.CORELINK_PAT }}
  run: cargo build --release
```

Speichern Sie den PAT als Repository-Secret (**Settings → Secrets and variables →
Actions**).

## Fehlerbehebung

| Symptom | Wahrscheinliche Ursache | Lösung |
|---|---|---|
| Jeder Build kompiliert neu | `RUSTC_WRAPPER` nicht gesetzt | Exportieren Sie `RUSTC_WRAPPER=sccache` in derselben Shell |
| `401 Unauthorized` in den sccache-Protokollen | `SCCACHE_WEBDAV_TOKEN` fehlt oder ist falsch | Setzen Sie ihn auf Ihren `corelink_pat_...`-PAT |
| `403 Forbidden` | PAT auf einen anderen Tenant beschränkt | Prüfen Sie, ob der `<tenant>` im Endpunkt mit dem Tenant Ihres PAT übereinstimmt |
| Cache-Misses bleiben bestehen | Nichtdeterministische Build-Eingaben | Fixieren Sie die Toolchain + `CARGO_INCREMENTAL=0`; führen Sie `sccache --show-stats` zur Analyse aus |

Vollständige Fehlerreferenz: [Fehlerbehebung](../troubleshooting.md).
