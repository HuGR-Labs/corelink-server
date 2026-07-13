---
id: intro
title: Was ist CoreLink?
sidebar_position: 1
description: CoreLink ist ein mandantenfähiger, inhaltsadressierbarer Cache für Build-Artefakte, Pakete, Container-Layer und ML-Modellgewichte — gehostet auf Cloudflare.
---

<!-- i18n:MT (de) — bootstrap MT-stub from EN source; replace with native-speaker translation before GA -->

> MT: Diese Seite wird übersetzt. Die englische Fassung gilt als verbindlich, bis ein Muttersprachler die Übersetzung freigibt (D+14).
>
> Canonical EN source: `docs/intro.md`

# Was ist CoreLink?

CoreLink ist ein gehosteter, mandantenfähiger **inhaltsadressierbarer Cache** für Build-Artefakte. Er speichert jeden Blob genau einmal anhand seines SHA-256-Digests und liefert ihn vom Cloudflare-Edge aus, der jedem Client am nächsten liegt.

Build-Werkzeuge, die die [Remote Execution API (REAPI)](https://github.com/bazelbuild/remote-apis) unterstützen — Bazel, Buck2, NativeLink und andere — können ohne Codeänderungen direkt auf CoreLink verweisen. Turborepo verbindet sich über eine einzige Umgebungsvariable. Rohe HTTP-Clients nutzen die REST-Endpunkte.

## Für wen es gedacht ist

- **Teams, die Bazel oder Buck2 einsetzen** und einen verwalteten Remote-Cache wollen, ohne selbst S3-Buckets, Redis oder `bazel-remote` zu betreiben.
- **Turborepo-Monorepos**, die einen benutzerdefinierten Remote-Cache außerhalb des gehosteten Angebots von Vercel wollen.
- **Plattform-Engineering-Teams**, die Mandantenisolierung, Audit-Logs und BYOK-Verschlüsselung in einem Dienst wollen.

## Was CoreLink nicht ist

CoreLink ist keine Remote-Execution-Engine. Es speichert und ruft Inhalte anhand ihres Hashes ab; es plant oder führt keine Build-Aktionen aus. Verwenden Sie es zusammen mit [BuildBarn](https://github.com/buildbarn/bb-storage) oder [EngFlow](https://www.engflow.com), wenn Sie Remote-Execution benötigen.

## Wie es funktioniert

```
build tool                CoreLink API (Cloudflare Worker)       R2 / KV
─────────────────────     ─────────────────────────────────     ─────────
PUT /v1/cas/<t>/<hash> ─► auth (PAT) → tenant isolation        → stored once
GET /v1/cas/<t>/<hash> ◄─ cache-hit lookup                     ← returned
```

Jeder Blob wird anhand seines SHA-256-Digests adressiert. Wenn zwei Mandanten dieselben Bytes hochladen, zahlt jeder Mandant für eine Kopie und hat eine unabhängige Zugriffssteuerung — der Inhalt wird auf der Speicherebene geteilt, der Zugriff nicht.

## Wichtige Funktionen

| Funktion | Details |
|---|---|
| Inhaltsadressierbarer Speicher (CAS) | SHA-256-basierter Blob-Speicher. Dedupliziert automatisch. |
| Action Cache (AC) | Ordnet `(action_digest) → (output_digest)` zu, sodass Bazel identische Aktionen überspringt. |
| Mandantenfähigkeit | Jeder Mandant ist auf PAT-Ebene isoliert. Mandantenübergreifende Lesezugriffe sind niemals möglich. |
| BYOK-Verschlüsselung | Mandanten im Enterprise-Plan können ihren eigenen AES-256-Schlüssel bereitstellen. |
| Audit-Log | Jeder Lese- und Schreibvorgang wird an ein unveränderliches, mandantengebundenes Log angehängt. |
| REAPI v2 | Vollständige gRPC-Dienste `ContentAddressableStorage` + `ActionCache` + `ByteStream`. |

## Nächster Schritt

Der schnellste Weg zu Ihrem ersten Cache-Hit ist der [5-minütige Quickstart](./quickstart.md).
