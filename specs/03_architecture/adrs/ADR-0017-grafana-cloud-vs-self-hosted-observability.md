---
id: "ADR-0017"
type: "adr"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "observability", "grafana", "vendor", "stub"]
---

# ADR-0017 — Grafana Cloud (Managed) vs Self-Hosted Prom/Loki/Tempo

## Context

S-09 + S-16 dashboard exigem observability stack: metrics + logs + traces + dashboards. Two approaches:

1. **Grafana Cloud** (Mimir + Loki + Tempo + Alertmanager managed).
2. **Self-hosted** (Prometheus + Loki + Tempo + Grafana on K8s).

## Decision

Adotamos **Grafana Cloud** (managed) for GA.

## Rationale

**Por que Grafana Cloud:**

1. **Operational overhead**: self-hosted Prom/Loki/Tempo = 1+ FTE de SRE para cap planning + scaling + version upgrade. CoreLink team é small at GA.
2. **Multi-region**: Grafana Cloud nativo multi-region; self-hosted requer multi-cluster federation = complexity exponential.
3. **Cost at scale**: Grafana Cloud $$ at scale > self-hosted compute, but FTE cost is much higher. Crossover point ~$100k/year Grafana spend.
4. **Reliability SLA**: Grafana Cloud 99.9% SLA; self-hosted SRE-dependent.

**Counterarguments:**

- **Vendor lock-in**: Mimir/Loki/Tempo são open-source; migration possible se needed.
- **Cardinality budget**: managed tier limits stricter; addressed via INV-OBS-CARDINALITY-BUDGET.
- **Custom backend desired**: pode ser exposto via OpenTelemetry adapter; Grafana Cloud não bloqueia.

## Consequences

**Positive:**
- 0 SRE FTE dedicated to observability infrastructure.
- Multi-region native.
- Faster time to GA.

**Negative:**
- Vendor dependency (Grafana Cloud SLA).
- Cost can escalate (mitigated via cardinality budget).
- Less control over query engine internals.

## Alternatives considered

- **Self-hosted on K8s**: rejected at GA — operational overhead.
- **Datadog APM**: rejected — proprietary lock-in stronger; cost higher at scale.
- **AWS CloudWatch**: rejected — Cloudflare Workers can't natively integrate cleanly.

## Migration path

If we outgrow Grafana Cloud (cost > benefit), migration:

1. Self-host Mimir/Loki/Tempo in CF Containers (S-14).
2. Re-route remote_write/remote_read.
3. Migrate dashboards-as-code (already JSON-as-code).

**Re-evaluate annually** (dashboard cost + customer-facing observability tier).

## References

- Grafana Cloud <https://grafana.com/products/cloud/>.
- `specs/04_sprints/_sealed/S09/_spec_contract.md`.
- `specs/03_architecture/observability_model.md`.
