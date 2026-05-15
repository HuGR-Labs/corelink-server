# CoreLink — Architecture Diagram Index

Diagram sources for the [top-level `ARCHITECTURE.md`](../../../ARCHITECTURE.md)
overview. All diagrams are Mermaid (`.mmd`) so they diff cleanly, render
on GitHub directly, and re-render to SVG/PNG via `mmdc` for slide decks
and customer-facing documents.

> **Canonical source of truth:** the Level-3 specs under
> [`specs/03_architecture/`](../../../specs/03_architecture/). Every
> diagram has a header comment listing the spec(s) it derives from.
> If a diagram disagrees with its spec, the spec wins and the diagram
> is a bug — file a `WI` to reconcile.

## Diagrams

| File | Type | Shows | Primary spec source |
|---|---|---|---|
| [`diagrams/system-context.mmd`](./diagrams/system-context.mmd) | C4 L1 flow | Outer boundary: actors, CF edge, CoreLink planes, storage bindings, sub-processors. | `security_model.md §3`, `data_model.md §3` |
| [`diagrams/data-flow-write.mmd`](./diagrams/data-flow-write.mmd) | Sequence | Write path: PAT verify → quota reserve → chunk → hash → R2 PUT → audit emit. | `data_model.md §9`, `security_model.md §6` |
| [`diagrams/data-flow-read.mmd`](./diagrams/data-flow-read.mmd) | Sequence | Read path: auth → tenant-prefix derivation → R2 GET → integrity re-verify → audit. | `data_model.md §9`, ADR-0028 |
| [`diagrams/data-flow-dsr.mmd`](./diagrams/data-flow-dsr.mmd) | Sequence | DSR erasure: ticket → queue → tombstone + pseudonymize → signed attestation. | `privacy_model.md §6, §8` |
| [`diagrams/byok-envelope.mmd`](./diagrams/byok-envelope.mmd) | Flow | KEK in customer KMS wraps per-tenant DEK; AAD binds tenant+digest+key_version+region; revocation drains cache. | `key_management.md`, `security_model.md §7` |
| [`diagrams/audit-chain-merkle.mmd`](./diagrams/audit-chain-merkle.mmd) | Flow | CloudEvents leaf → BLAKE3 → Merkle accumulator → hourly Ed25519-signed root with RFC 3161 timestamp → inclusion-proof export. | `security_model.md §6`, ADR-0033 |
| [`diagrams/tenant-isolation.mmd`](./diagrams/tenant-isolation.mmd) | Flow | HKDF prefix derivation, two-layer enforcement (Worker authZ + R2 binding policy), TLA+ checked in CI. | `security_model.md §3` (TB-3), `data_model.md §2.2` |
| [`diagrams/region-failover.mmd`](./diagrams/region-failover.mmd) | State machine | Active/passive failover: healthy → degraded → cutover → rollback OR stable; drill mode branch. | `resilience_patterns.md`, SLO §4.18–§4.19 |

## Rendering

### Live preview

Any Markdown viewer that supports Mermaid (GitHub web UI, VS Code with
the *Markdown Preview Mermaid Support* extension, Obsidian) renders
`.mmd` blocks inline.

### Local SVG / PNG

Install `mermaid-cli` once:

```bash
npm install -g @mermaid-js/mermaid-cli
```

Render a single diagram:

```bash
mmdc \
  --input  docs/internal/architecture/diagrams/system-context.mmd \
  --output docs/internal/architecture/diagrams/system-context.svg \
  --theme  default \
  --backgroundColor transparent
```

Render the whole set (zsh / bash):

```bash
for f in docs/internal/architecture/diagrams/*.mmd; do
  mmdc --input "$f" --output "${f%.mmd}.svg" --backgroundColor transparent
done
```

### CI render (optional)

We do **not** commit generated SVGs to the repo (they would churn on
every Mermaid version bump). For slide decks or PDF compliance
exports, render on demand via the snippet above.

## Adding a new diagram

1. Add the `.mmd` source under `diagrams/`.
2. Include a top-of-file comment listing the canonical spec source(s)
   it derives from.
3. Add a row to the table in this README.
4. Cross-link from `ARCHITECTURE.md` in the section that consumes it.
5. If the diagram materially changes the threat model or the data
   lifecycle, update the relevant `specs/03_architecture/*.md` first
   — the diagram follows the spec, not the other way around.
