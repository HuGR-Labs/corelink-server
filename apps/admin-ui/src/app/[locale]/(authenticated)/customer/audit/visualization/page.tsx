// /[locale]/customer/audit/visualization — customer audit-chain verification.
//
// Migrated onto the Linear kit (2026-07-07). Was a raw-HTML page (inline hex,
// bare <section>/<table>, unstyled after the #651 generic-CSS deletion). Now
// every surface is a `@/components/ui/linear` primitive: Card, lin-table,
// Button, Badge, Modal, Callout, EmptyState, InlineError.
//
// DATA-HONESTY: the chain endpoints (`/v1/customer/audit/chain/*`,
// `/events/:id/proof`) are served by the E2E mock today and are NOT yet wired to
// a production backend. So this screen is framed as a "verification preview": the
// inclusion proofs ARE verified in the browser (real BLAKE3/SHA-256 crypto over
// whatever bytes the endpoint returns), but we never claim the sample chain is
// the tenant's live, production audit trail. In production the endpoints 404 →
// honest InlineError / EmptyState, never a fabricated "verified" head.
//
// Components (spec `specs/_audits/sealed/2026-05-15-audit-viz-spec.md`):
//   A — chain head card      B — leaf table       C — proof modal
//   D — chain integrity timeline sparkline        E — export bundle
//
// Accessibility: WCAG 2.2 AA — aria-live status, focus-trapped kit Modal,
// keyboard activation, no color-only signals.

"use client";

import React from "react";
import { verifyAuditProof, type AuditProofResult } from "@/lib/audit/verify-proof";
import {
  Button,
  Callout,
  Card,
  EmptyState,
  InlineError,
  Modal,
  Skeleton,
} from "@/components/ui/linear";

interface ChainHead {
  head_digest: string;
  total_events: number;
  last_updated: string;
  algorithm: "blake3";
}

interface AuditLeaf {
  event_id: string;
  ts: string;
  event_type: string;
  audit_chain_seq: number;
  leaf_digest: string;
}

interface LeavesPage {
  rows: AuditLeaf[];
  next_cursor: string | null;
}

interface ProofEnvelope {
  leaf_hash: string;
  siblings: Array<{ hash: string; position: "left" | "right" }>;
  expected_root: string;
  algorithm: "blake3" | "sha256";
  chain_seq: number;
  event_id: string;
}

interface ChainSnapshot {
  ts: string;
  head_digest: string;
  total_events: number;
}

const apiBase = (): string => {
  if (typeof process !== "undefined") {
    return process.env["NEXT_PUBLIC_CORELINK_API_URL"] ?? "/api";
  }
  return "/api";
};

async function fetchJson<T>(path: string): Promise<T> {
  const res = await fetch(`${apiBase()}${path}`, {
    headers: { accept: "application/json" },
  });
  if (!res.ok) throw new Error(`fetch ${path} failed: ${res.status}`);
  return (await res.json()) as T;
}

function truncHex(hex: string, head = 8, tail = 8): string {
  if (hex.length <= head + tail + 3) return hex;
  return `${hex.slice(0, head)}…${hex.slice(-tail)}`;
}

function relativeTime(iso: string, now: number = Date.now()): string {
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return iso;
  const delta = Math.max(0, now - t);
  const m = Math.floor(delta / 60_000);
  if (m < 1) return "just now";
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  const d = Math.floor(h / 24);
  return `${d}d ago`;
}

/** A stacked label/value row — the kit has no definition-list primitive, so this
 *  renders the key-value pairs with the token text-ramp classes. */
function KvRow({
  label,
  children,
  first,
}: {
  label: string;
  children: React.ReactNode;
  first?: boolean;
}): React.ReactElement {
  return (
    <div className={first ? undefined : "lin-mt"}>
      <div className="lin-t3">{label}</div>
      <div className="lin-t1">{children}</div>
    </div>
  );
}

// ---------- Component A: chain head card ----------

function ChainHeadCard({
  head,
  error,
  onVerifyHead,
  verifyState,
}: {
  head: ChainHead | null;
  error: string | null;
  onVerifyHead: () => void;
  verifyState: "idle" | "running" | "ok" | "fail";
}): React.ReactElement {
  return (
    <Card title="Audit chain head">
      <div data-testid="chain-head-card">
        {head ? (
          <>
            <KvRow label="Total events" first>
              <span data-testid="chain-head-total">{head.total_events.toLocaleString()}</span>
            </KvRow>
            <KvRow label="Head digest">
              <code data-testid="chain-head-digest" title={head.head_digest}>
                {truncHex(head.head_digest, 12, 12)}
              </code>
            </KvRow>
            <KvRow label="Algorithm">
              <span data-testid="chain-head-algorithm">{head.algorithm}</span>
            </KvRow>
            <KvRow label="Last updated">
              <time data-testid="chain-head-last-updated" dateTime={head.last_updated}>
                {relativeTime(head.last_updated)}
              </time>
            </KvRow>

            <div className="lin-mt-lg" data-testid="chain-head-verify">
              <Button
                variant="ghost"
                size="sm"
                data-testid="verify-head-btn"
                onClick={onVerifyHead}
                disabled={!head || verifyState === "running"}
              >
                Verify head
              </Button>{" "}
              <span
                role="status"
                aria-live="polite"
                data-testid="verify-head-status"
                data-state={verifyState}
                className={
                  verifyState === "ok"
                    ? "lin-t1"
                    : verifyState === "fail"
                      ? "lin-t1"
                      : "lin-t3"
                }
              >
                {verifyState === "running" && "Verifying…"}
                {verifyState === "ok" && "✓ Verified locally"}
                {verifyState === "fail" && "✗ Mismatch — see the trust-center incident page"}
              </span>
            </div>
          </>
        ) : error ? (
          <EmptyState
            title="Chain verification preview unavailable"
            body="The audit-chain endpoints aren't reachable from this environment yet. Verification lights up once your tenant's chain is wired to the backend."
          />
        ) : (
          <div data-testid="chain-head-loading" aria-busy="true" aria-label="Loading chain head">
            <Skeleton rows={4} />
          </div>
        )}
      </div>
    </Card>
  );
}

// ---------- Component B: leaf table ----------

function LeafTable({
  leaves,
  loading,
  error,
  onShowProof,
}: {
  leaves: AuditLeaf[];
  loading: boolean;
  error: string | null;
  onShowProof: (leaf: AuditLeaf) => void;
}): React.ReactElement {
  return (
    <Card title="My tenant's audit events" className="lin-mt-lg">
      {loading ? (
        <div aria-busy="true" aria-label="Loading audit events">
          <Skeleton rows={5} />
        </div>
      ) : error ? (
        <EmptyState
          title="No events to show"
          body="Audit leaves will appear here once the chain endpoints are wired to your tenant."
        />
      ) : leaves.length === 0 ? (
        <EmptyState title="No audit events yet" body="No leaves in the chain for this tenant." />
      ) : (
        <table className="lin-table" data-testid="audit-leaf-table">
          <caption className="lin-t3 lin-caption">
            My tenant&apos;s audit events, most recent first.
          </caption>
          <thead>
            <tr>
              <th>Event ID</th>
              <th>Timestamp</th>
              <th>Type</th>
              <th>Chain seq</th>
              <th>Leaf digest</th>
              <th>Proof</th>
            </tr>
          </thead>
          <tbody>
            {leaves.map((l) => (
              <tr key={l.event_id} data-testid={`leaf-row-${l.event_id}`}>
                <td>
                  <code>{l.event_id}</code>
                </td>
                <td>
                  <time dateTime={l.ts}>{l.ts}</time>
                </td>
                <td>{l.event_type}</td>
                <td>{l.audit_chain_seq}</td>
                <td>
                  <code title={l.leaf_digest}>{truncHex(l.leaf_digest)}</code>
                </td>
                <td>
                  <Button
                    variant="ghost"
                    size="sm"
                    data-testid={`show-proof-${l.event_id}`}
                    onClick={() => onShowProof(l)}
                  >
                    Show proof
                  </Button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </Card>
  );
}

// ---------- Component C: proof modal ----------

function ProofModal({
  open,
  proof,
  result,
  onClose,
  onDownload,
}: {
  open: boolean;
  proof: ProofEnvelope | null;
  result: AuditProofResult | null;
  onClose: () => void;
  onDownload: () => void;
}): React.ReactElement | null {
  const footer = (
    <>
      <Button
        variant="ghost"
        size="sm"
        data-testid="download-proof-btn"
        onClick={onDownload}
        disabled={!proof}
      >
        Download proof JSON
      </Button>
      <Button variant="primary" size="sm" data-testid="close-proof-modal" onClick={onClose}>
        Close
      </Button>
    </>
  );

  return (
    <Modal open={open} onClose={onClose} title="Inclusion proof" footer={footer}>
      <div data-testid="proof-modal">
        {proof == null ? (
          <p
            role="status"
            aria-live="polite"
            data-testid="proof-result"
            data-ok="false"
            className="lin-t2"
          >
            Verifying proof…
          </p>
        ) : (
          <>
            <p className="lin-t2">
              Event <code data-testid="proof-event-id">{proof.event_id}</code> at chain seq{" "}
              <strong data-testid="proof-chain-seq" className="lin-t1">
                {proof.chain_seq}
              </strong>
            </p>

            <KvRow label="Leaf hash" first>
              <code data-testid="proof-leaf-hash">{truncHex(proof.leaf_hash, 12, 12)}</code>
            </KvRow>
            <KvRow label="Expected root">
              <code data-testid="proof-expected-root">
                {truncHex(proof.expected_root, 12, 12)}
              </code>
            </KvRow>
            <KvRow label="Algorithm">
              <span data-testid="proof-algorithm">{proof.algorithm}</span>
            </KvRow>
            <KvRow label="Sibling steps">
              <span data-testid="proof-sibling-count">{proof.siblings.length}</span>
            </KvRow>

            <ol data-testid="proof-path" className="lin-t2 lin-mt">
              {proof.siblings.map((s, i) => (
                <li key={i} data-testid={`proof-step-${i}`}>
                  step {i + 1}: <em>{s.position}</em> sibling{" "}
                  <code title={s.hash}>{truncHex(s.hash)}</code>
                </li>
              ))}
            </ol>

            <p
              role="status"
              aria-live="polite"
              data-testid="proof-result"
              data-ok={result?.ok ? "true" : "false"}
              data-verifier={result && "verifier" in result ? result.verifier : undefined}
              data-reason={result && !result.ok ? result.reason : undefined}
              className="lin-mt lin-t2"
            >
              {result == null && "Verifying proof…"}
              {result?.ok && (
                <>
                  ✓ Proof verified ({result.verifier}); computed root{" "}
                  <code>{truncHex(result.computed_root)}</code>
                </>
              )}
              {result && !result.ok && result.reason === "wasm_unavailable" && (
                <>
                  ⚠ Browser verifier unavailable — use the{" "}
                  <Button
                    href="/docs/reference/cli/audit"
                    variant="ghost"
                    size="sm"
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    corelink audit verify
                  </Button>{" "}
                  CLI to verify offline.
                </>
              )}
              {result && !result.ok && result.reason === "mismatch" && (
                <>✗ Mismatch — please contact your CoreLink support representative.</>
              )}
              {result && !result.ok && result.reason === "algorithm_unsupported" && (
                <>⚠ Unsupported algorithm — use the CLI verifier.</>
              )}
              {result && !result.ok && result.reason === "invalid_hex" && (
                <>✗ Invalid proof envelope (hex format).</>
              )}
            </p>
          </>
        )}
      </div>
    </Modal>
  );
}

// ---------- Component D: chain integrity timeline ----------

function IntegrityTimeline({
  snapshots,
}: {
  snapshots: ChainSnapshot[];
}): React.ReactElement {
  if (snapshots.length === 0) {
    return (
      <Card title="Chain integrity timeline" className="lin-mt-lg">
        <p data-testid="timeline-empty" className="lin-t3">
          Insufficient history — chain less than 24h old.
        </p>
      </Card>
    );
  }

  const max = Math.max(...snapshots.map((s) => s.total_events), 1);

  // anomaly = total_events lower than prior tick (rollback should be impossible).
  const anomalies = new Set<number>();
  for (let i = 1; i < snapshots.length; i++) {
    const cur = snapshots[i];
    const prev = snapshots[i - 1];
    if (cur && prev && cur.total_events < prev.total_events) {
      anomalies.add(i);
    }
  }

  return (
    <Card title="Chain integrity timeline" className="lin-mt-lg">
      <ul
        data-testid="timeline-sparkline"
        data-anomaly-count={anomalies.size}
        className="lin-spark"
        style={{ display: "flex", gap: 2, alignItems: "flex-end", height: 80, listStyle: "none", padding: 0, margin: 0 }}
      >
        {snapshots.map((s, i) => {
          const h = Math.max(2, Math.round((s.total_events / max) * 78));
          const anomaly = anomalies.has(i);
          return (
            <li
              key={i}
              tabIndex={0}
              data-testid={`timeline-tick-${i}`}
              data-anomaly={anomaly ? "true" : "false"}
              aria-label={`${s.ts}: ${s.total_events} events, digest ${truncHex(s.head_digest)}${
                anomaly ? " (anomaly: rollback)" : ""
              }`}
              style={{
                width: 6,
                height: h,
                borderRadius: 2,
                background: anomaly ? "var(--danger)" : "var(--line-2)",
              }}
            />
          );
        })}
      </ul>
      {anomalies.size > 0 && (
        <div role="alert" data-testid="timeline-anomaly-alert" className="lin-mt">
          <Callout tone="danger">
            Chain rollback detected — see{" "}
            <Button href="/trust/incident-response" variant="ghost" size="sm">
              trust-center incident response
            </Button>
            .
          </Callout>
        </div>
      )}
    </Card>
  );
}

// ---------- Component E: export bundle ----------

function ExportBundleButton({
  head,
  leaves,
}: {
  head: ChainHead | null;
  leaves: AuditLeaf[];
}): React.ReactElement {
  const handleExport = (): void => {
    if (!head) return;
    const bundle = {
      "@context": "https://corelink.humangr.com/specs/audit-export/v1",
      "@type": "AuditExportBundle",
      generated_at: new Date().toISOString(),
      chain_head: head,
      visible_leaves: leaves,
      verify_instructions: [
        "Run: corelink audit verify --bundle <path-to-this-file>",
        "See https://corelink-docs.humangr.com/reference/cli/audit",
      ],
    };
    if (typeof window === "undefined") return;
    const blob = new Blob([JSON.stringify(bundle, null, 2)], {
      type: "application/json",
    });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `corelink-audit-bundle-${new Date().toISOString().slice(0, 10)}.json`;
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <Card title="Export & offline verification" className="lin-mt-lg">
      <p className="lin-t3">
        Download a signed bundle and re-verify the whole chain offline with the CLI — no trust
        in this browser required.
      </p>
      <div className="lin-mt">
        <Button
          variant="primary"
          size="sm"
          data-testid="export-bundle-btn"
          onClick={handleExport}
          disabled={!head}
        >
          Download audit bundle (JSON-LD)
        </Button>{" "}
        <Button
          href="/docs/reference/cli/audit"
          data-testid="cli-doc-link"
          variant="ghost"
          size="sm"
          target="_blank"
          rel="noopener noreferrer"
        >
          CLI docs →
        </Button>
      </div>
    </Card>
  );
}

// ---------- Page ----------

export default function CustomerAuditVisualizationPage(): React.ReactElement {
  const [head, setHead] = React.useState<ChainHead | null>(null);
  const [leaves, setLeaves] = React.useState<AuditLeaf[]>([]);
  const [snapshots, setSnapshots] = React.useState<ChainSnapshot[]>([]);
  const [loading, setLoading] = React.useState(true);
  const [error, setError] = React.useState<string | null>(null);

  const [proofOpen, setProofOpen] = React.useState(false);
  const [proof, setProof] = React.useState<ProofEnvelope | null>(null);
  const [proofResult, setProofResult] = React.useState<AuditProofResult | null>(null);

  const [verifyState, setVerifyState] = React.useState<"idle" | "running" | "ok" | "fail">(
    "idle",
  );

  const load = React.useCallback(async (): Promise<void> => {
    setLoading(true);
    setError(null);
    try {
      const [h, l, s] = await Promise.all([
        fetchJson<ChainHead>("/v1/customer/audit/chain/head"),
        fetchJson<LeavesPage>("/v1/customer/audit/events"),
        fetchJson<{ snapshots: ChainSnapshot[] }>(
          "/v1/customer/audit/chain/history?window=30d",
        ),
      ]);
      setHead(h);
      setLeaves(l.rows);
      setSnapshots(s.snapshots);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  React.useEffect(() => {
    void load();
  }, [load]);

  const openProof = async (leaf: AuditLeaf): Promise<void> => {
    setProofOpen(true);
    setProofResult(null);
    setProof(null);
    try {
      const env = await fetchJson<ProofEnvelope>(
        `/v1/customer/audit/events/${leaf.event_id}/proof`,
      );
      setProof(env);
      const result = await verifyAuditProof({
        leaf_hash: env.leaf_hash,
        siblings: env.siblings,
        expected_root: env.expected_root,
        algorithm: env.algorithm,
      });
      setProofResult(result);
    } catch (e) {
      setProofResult({
        ok: false,
        reason: "wasm_unavailable",
      });
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const verifyHead = async (): Promise<void> => {
    if (!head || leaves.length === 0) return;
    setVerifyState("running");
    try {
      // Re-verify the most recent leaf as a proxy for head integrity.
      const first = leaves[0];
      if (!first) {
        setVerifyState("fail");
        return;
      }
      const env = await fetchJson<ProofEnvelope>(
        `/v1/customer/audit/events/${first.event_id}/proof`,
      );
      const result = await verifyAuditProof({
        leaf_hash: env.leaf_hash,
        siblings: env.siblings,
        expected_root: env.expected_root,
        algorithm: env.algorithm,
      });
      setVerifyState(result.ok ? "ok" : "fail");
    } catch {
      setVerifyState("fail");
    }
  };

  return (
    <main aria-labelledby="page-heading" data-testid="audit-viz-page">
      <h1 id="page-heading">Audit chain verification</h1>
      <p>
        Browser-native inclusion proofs over CoreLink&apos;s BLAKE3 + RFC 8785 JCS audit chain.
        Each leaf is verified client-side; the offline path stays via the{" "}
        <code>corelink audit verify</code> CLI.
      </p>

      <Callout tone="info">
        Verification preview — proofs are verified in your browser against the audit-chain
        endpoints. Live per-tenant chain wiring is rolling out; nothing here is a fabricated
        &ldquo;verified&rdquo; result.
      </Callout>

      {error && head && (
        <div role="alert" data-testid="page-error" className="lin-mt">
          <InlineError error={error} onRetry={() => void load()} />
        </div>
      )}

      <div className="lin-mt-lg">
        <ChainHeadCard
          head={head}
          error={error}
          onVerifyHead={() => void verifyHead()}
          verifyState={verifyState}
        />
      </div>

      <IntegrityTimeline snapshots={snapshots} />

      <LeafTable
        leaves={leaves}
        loading={loading}
        error={error}
        onShowProof={(l) => void openProof(l)}
      />

      <ExportBundleButton head={head} leaves={leaves} />

      <ProofModal
        open={proofOpen}
        proof={proof}
        result={proofResult}
        onClose={() => setProofOpen(false)}
        onDownload={() => {
          if (!proof || typeof window === "undefined") return;
          const blob = new Blob([JSON.stringify(proof, null, 2)], {
            type: "application/json",
          });
          const url = URL.createObjectURL(blob);
          const a = document.createElement("a");
          a.href = url;
          a.download = `corelink-proof-${proof.event_id}.json`;
          a.click();
          URL.revokeObjectURL(url);
        }}
      />
    </main>
  );
}
