---
type: "ADR"
title: "ADR-0017 — Grafana Cloud (managed) vs self-hosted Prom/Loki/Tempo"
description: "Chooses managed Grafana Cloud over a self-hosted observability stack for GA to avoid a dedicated SRE FTE, with a documented open-source migration path if cost outgrows benefit."
source_files:
  - "specs/03_architecture/adrs/ADR-0017-grafana-cloud-vs-self-hosted-observability.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "observability", "grafana", "vendor", "draft"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0017 — Grafana Cloud (managed) vs self-hosted Prom/Loki/Tempo

CoreLink's dashboards need a metrics + logs + traces stack, and at GA the team is too small to also run one. This ADR (a DRAFT) chooses managed Grafana Cloud over self-hosting Prometheus/Loki/Tempo, trading some vendor dependency and cardinality limits for zero dedicated observability SRE headcount. It matters as the decision that keeps the launch team out of the cap-planning/scaling/upgrade business while leaving an open-source exit open.

# Context

S-09 + S-16 dashboards require a full observability stack, and there are two approaches: managed Grafana Cloud (Mimir + Loki + Tempo + Alertmanager) or self-hosted Prometheus/Loki/Tempo/Grafana on Kubernetes (`specs/03_architecture/adrs/ADR-0017-grafana-cloud-vs-self-hosted-observability.md:21-25`).

# Decision

Adopt managed Grafana Cloud for GA (`specs/03_architecture/adrs/ADR-0017-grafana-cloud-vs-self-hosted-observability.md:28`). The rationale: self-hosting costs 1+ SRE FTE for capacity planning, scaling, and upgrades; Grafana Cloud is natively multi-region (self-hosting needs multi-cluster federation); and although managed spend rises at scale, the FTE cost is higher until a ~$100k/year crossover, with a 99.9% SLA (`specs/03_architecture/adrs/ADR-0017-grafana-cloud-vs-self-hosted-observability.md:32-37`). Self-hosted on K8s, Datadog APM, and AWS CloudWatch were all rejected (operational overhead, stronger lock-in, and poor Cloudflare Workers integration respectively) (`specs/03_architecture/adrs/ADR-0017-grafana-cloud-vs-self-hosted-observability.md:59-61`).

# Consequences

- Zero SRE FTE on observability infrastructure, native multi-region, faster time to GA (`specs/03_architecture/adrs/ADR-0017-grafana-cloud-vs-self-hosted-observability.md:47-50`).
- Vendor dependency on the Grafana Cloud SLA and escalating cost, mitigated by a cardinality budget; less control over query-engine internals (`specs/03_architecture/adrs/ADR-0017-grafana-cloud-vs-self-hosted-observability.md:52-54`).
- Because Mimir/Loki/Tempo are open-source, an exit exists — self-host in CF Containers and re-route remote_write/remote_read with dashboards already as JSON-as-code, re-evaluated annually (`specs/03_architecture/adrs/ADR-0017-grafana-cloud-vs-self-hosted-observability.md:65-71`).

# Citations

1. `specs/03_architecture/adrs/ADR-0017-grafana-cloud-vs-self-hosted-observability.md:21-25` — the two observability approaches.
2. `specs/03_architecture/adrs/ADR-0017-grafana-cloud-vs-self-hosted-observability.md:28` — the decision: managed Grafana Cloud.
3. `specs/03_architecture/adrs/ADR-0017-grafana-cloud-vs-self-hosted-observability.md:32-37` — FTE / multi-region / cost / SLA rationale.
4. `specs/03_architecture/adrs/ADR-0017-grafana-cloud-vs-self-hosted-observability.md:59-61` — rejected alternatives.
5. `specs/03_architecture/adrs/ADR-0017-grafana-cloud-vs-self-hosted-observability.md:47-54` — positive and negative consequences.
6. `specs/03_architecture/adrs/ADR-0017-grafana-cloud-vs-self-hosted-observability.md:65-71` — the open-source migration path.
