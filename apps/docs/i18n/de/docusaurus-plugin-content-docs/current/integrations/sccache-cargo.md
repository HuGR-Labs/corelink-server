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

CoreLink ist die **gemeinsame** Ebene — diejenige, über die eine andere Maschine oder ein
frisch gestarteter CI-Runner wiederverwenden kann, was jemand anderes bereits kompiliert
hat. Sie ist dafür gedacht, *hinter* einer lokalen Festplatten-Ebene zu liegen, und nicht,
diese zu ersetzen. Die folgende Konfiguration richtet beide ein.

## Voraussetzungen

- `sccache` **0.15 oder neuer** (`cargo install sccache` oder das Paket Ihrer
  Distribution). Prüfen Sie es mit `sccache --version` — die mehrstufige Storage-Kette,
  die Ihnen eine lokale Ebene verschafft, kam mit 0.15.0.
- Ein CoreLink-PAT (`corelink_pat_...`) mit Lese- und Schreibberechtigung für den Cache.
- Die UUID Ihres Tenants.

## Konfigurieren

Verweisen Sie das WebDAV-Backend von sccache auf den Pfad Ihres Tenants, übergeben Sie den
PAT als Bearer-Token und schalten Sie eine lokale Festplatten-Ebene davor:

```bash
export SCCACHE_WEBDAV_ENDPOINT="https://corelink-api.humangr.com/cargo/<your-tenant-id>"
export SCCACHE_WEBDAV_TOKEN="corelink_pat_XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"
export SCCACHE_MULTILEVEL_CHAIN="disk,webdav"   # erfordert sccache >= 0.15
export SCCACHE_DIR="$HOME/.cache/sccache"       # Ablageort der lokalen Ebene
export RUSTC_WRAPPER=sccache
```

Was die einzelnen Variablen tun:

| Variable | Funktion |
|---|---|
| `SCCACHE_WEBDAV_ENDPOINT` | Das CoreLink-Backend — die **gemeinsame** Ebene, unter dem Pfad Ihres Tenants |
| `SCCACHE_WEBDAV_TOKEN` | Der PAT, den sccache als `Authorization: Bearer` sendet |
| `SCCACHE_MULTILEVEL_CHAIN` | Die Storage-Kette, nächstgelegene Ebene zuerst. `disk,webdav` = lokale Festplatte vor CoreLink |
| `SCCACHE_DIR` | Dateisystempfad der lokalen Festplatten-Ebene. Nur wirksam, wenn die Kette `disk` enthält |
| `RUSTC_WRAPPER` | Sorgt dafür, dass cargo jeden `rustc`-Aufruf über sccache leitet |

:::caution Lassen Sie `SCCACHE_MULTILEVEL_CHAIN` nicht weg
sccache wählt **genau ein** Storage-Backend aus. Sein `storage_from_config` fällt nur dann
auf das Festplatten-Backend zurück, wenn *kein* Remote-Backend konfiguriert ist — mit
gesetztem `SCCACHE_WEBDAV_ENDPOINT` und nicht gesetzter Kette arbeitet sccache also
ausschließlich remote: Jeder Cache-Lesezugriff ist ein HTTPS-Roundtrip, und `SCCACHE_DIR`
bleibt wirkungslos.

`SCCACHE_MULTILEVEL_CHAIN="disk,webdav"` baut stattdessen eine zweistufige Kette auf —
zuerst die lokale Festplatte, dahinter CoreLink — und ein auf CoreLink gefundener Treffer
wird im Hintergrund in die lokale Ebene zurückgeschrieben. Mehrstufige Ketten erfordern
sccache **0.15.0 oder neuer**; bei einem älteren sccache wird die Variable ignoriert, und
Sie landen unbemerkt wieder im Nur-Remote-Betrieb — prüfen Sie daher `sccache --version`.
:::

Bauen Sie anschließend wie gewohnt:

```bash
cargo build --release
```

Wenn eine Suche die lokale Ebene verfehlt, stellt sccache `GET`-, `PUT`- und
`HEAD`-Anfragen an
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

Mit konfigurierter Kette wird ein zweiter Build auf derselben Maschine normalerweise aus der
*lokalen* Ebene bedient — genau das ist der Sinn der Sache, beweist aber nichts über
CoreLink. Um zu bestätigen, dass CoreLink selbst ausliefert, verwerfen Sie die lokale Ebene
zwischen den beiden Builds:

```bash
cargo clean
cargo build --release        # kalt — kompiliert und legt in beiden Ebenen ab
cargo clean
sccache --stop-server        # Daemon stoppen, bevor Sie seine Festplatten-Ebene anfassen
rm -rf "$SCCACHE_DIR"        # lokale Ebene verwerfen, damit der Lesezugriff CoreLink erreichen muss
cargo build --release        # warm — der Treffer kann nur von CoreLink kommen
sccache --show-stats
```

`sccache --show-stats` meldet die Anzahl der Cache-Treffer und den konfigurierten Storage.
Ein von null verschiedener Wert bei "Cache hits" im zweiten Build — bei geleerter lokaler
Ebene — bestätigt, dass CoreLink die Artefakte bereitgestellt hat. Führen Sie das `rm -rf`
nur für diese Prüfung aus; im Normalbetrieb soll die lokale Ebene erhalten bleiben.

## CI-Beispiel (GitHub Actions)

```yaml
- name: Build with sccache + CoreLink
  env:
    RUSTC_WRAPPER: sccache
    SCCACHE_WEBDAV_ENDPOINT: https://corelink-api.humangr.com/cargo/acme-prod
    SCCACHE_WEBDAV_TOKEN: ${{ secrets.CORELINK_PAT }}
    SCCACHE_MULTILEVEL_CHAIN: disk,webdav
    SCCACHE_DIR: ${{ runner.temp }}/sccache
  run: cargo build --release
```

Speichern Sie den PAT als Repository-Secret (**Settings → Secrets and variables →
Actions**).

Auf einem ephemeren Runner startet die lokale Ebene leer und wird am Ende des Jobs
verworfen, sie hilft also nur *innerhalb* eines Jobs — was bei einem Job mit mehreren
cargo-Aufrufen dennoch etwas bringt. Lassen Sie die Kette in jedem Fall konfiguriert; Sie
können `SCCACHE_DIR` zusätzlich mit der Cache-Action Ihres Runners über mehrere Läufe
hinweg erhalten.

## Fehlerbehebung

| Symptom | Wahrscheinliche Ursache | Lösung |
|---|---|---|
| Jeder Build kompiliert neu | `RUSTC_WRAPPER` nicht gesetzt | Exportieren Sie `RUSTC_WRAPPER=sccache` in derselben Shell |
| `401 Unauthorized` in den sccache-Protokollen | `SCCACHE_WEBDAV_TOKEN` fehlt oder ist falsch | Setzen Sie ihn auf Ihren `corelink_pat_...`-PAT |
| `403 Forbidden` | PAT auf einen anderen Tenant beschränkt | Prüfen Sie, ob der `<tenant>` im Endpunkt mit dem Tenant Ihres PAT übereinstimmt |
| Cache-Misses bleiben bestehen | Nichtdeterministische Build-Eingaben | Fixieren Sie die Toolchain + `CARGO_INCREMENTAL=0`; führen Sie `sccache --show-stats` zur Analyse aus |
| Treffer werden gezählt, aber jeder einzelne ist eine Netzwerkanfrage | `SCCACHE_MULTILEVEL_CHAIN` nicht gesetzt — sccache arbeitet nur remote | Setzen Sie `SCCACHE_MULTILEVEL_CHAIN="disk,webdav"` **und** `SCCACHE_DIR` |
| `SCCACHE_DIR` scheint ignoriert zu werden | Dieselbe Ursache, oder ein sccache älter als 0.15 (Ketten-Variable wird ignoriert) | Kette setzen; `sccache --version` prüfen und auf ≥ 0.15 aktualisieren |

Vollständige Fehlerreferenz: [Fehlerbehebung](../troubleshooting.md).
