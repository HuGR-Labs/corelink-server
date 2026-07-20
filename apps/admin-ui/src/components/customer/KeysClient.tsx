// Customer-side Tokens (PATs) — Linear-styled list + create + rotate + revoke + BYOK status.
//
// W3 (customer-dashboard build wave). Kit-only: every surface is a `@/components/ui/linear`
// primitive; the minted PAT is revealed through the shared `PatModal` (copy + shown-once +
// confirm), never dumped as plain text; revoke AND rotate are gated behind a `ConfirmDialog`;
// every mutation fires a Toast. Data-truth: PAT CRUD is [live]; `last_used_at` is [stub]
// (BE-5 will stamp it on auth) so it is rendered as an honest "—", never a fabricated date
// nor a misleading "Never".
//
// Rotate is composed from the existing client primitives (revokePat + createPat with the same
// name+scopes) — no new client method. Revoked tokens are pruned from the default view via a
// "Show" filter (Active | All); tokens revoked in THIS session stay visible so the user sees
// the row flip to "revoked" before it is tidied away on the next load.

"use client";

import React from "react";
import { useCustomerClient } from "@/lib/use-customer-client";
import type { CustomerOverview, CustomerPat } from "@/lib/customer-types";
import { setPlaintextPat } from "@/lib/onboarding-state";
import { PatModal } from "@/components/onboarding/PatModal";
import {
  Badge,
  Button,
  Callout,
  Card,
  ConfirmDialog,
  EmptyState,
  Field,
  HelpPopover,
  InlineError,
  Input,
  Segmented,
  Skeleton,
  ToastProvider,
  useToast,
} from "@/components/ui/linear";

/** Scope catalog — each token scope with a plain-language description + a
 *  HelpPopover body explaining exactly what it grants. The `id` values are the
 *  wire scopes the backend understands. */
interface ScopeSpec {
  id: string;
  label: string;
  desc: string;
  help: string;
}

const SCOPE_SPECS: readonly ScopeSpec[] = [
  {
    id: "cache:r",
    label: "Read cache",
    desc: "Pull cached artifacts (cache hits). Read-only.",
    help: "Lets a client download objects that are already in your cache — the fast path. It cannot upload or delete anything. Safe for CI jobs that should only benefit from the cache, never fill it.",
  },
  {
    id: "cache:w",
    label: "Write cache",
    desc: "Upload artifacts so future builds hit the cache.",
    help: "Lets a client upload (store) objects into your cache. Grant this to the jobs you want to populate the cache. Usually paired with Read so the same job can both pull and push.",
  },
  {
    id: "cache:find-missing",
    label: "Find missing",
    desc: "Batch-check which artifacts are absent before uploading.",
    help: "The REAPI find-missing-blobs probe: a client asks 'which of these do you already have?' so it only uploads what's missing. Build tools (Bazel, sccache) use this to avoid re-uploading known objects. Least-privilege: it grants ONLY existence probes — no download, no upload.",
  },
  // NOTE: the audit log is read from the dashboard (the "Audit log" page, your
  // signed-in session) — it is NOT a self-serve PAT scope. `admin:audit`
  // (`SCOPE_ADMIN_AUDIT`) is an OPERATOR/internal-mint scope only; there is no
  // customer-PAT endpoint that consumes it, and the self-serve mint never grants
  // an `admin:*` scope. Offering it here minted nothing (401) — removed.
] as const;

const DEFAULT_SCOPES = ["cache:r"];

const PAT_MODAL_LABELS = {
  title: "Your new token",
  warning:
    "Copy this token now — for your security it is shown only once and can never be retrieved again. Store it in a secret manager or CI secret, never in code.",
  copy: "Copy token",
  confirmSaved: "I've stored this token somewhere safe",
  finish: "Done",
} as const;

type RevokedFilter = "active" | "all";

function KeysInner(): React.ReactElement {
  const client = useCustomerClient();
  const { toast } = useToast();

  const [pats, setPats] = React.useState<CustomerPat[]>([]);
  const [byok, setByok] = React.useState<CustomerOverview["byok"] | null>(null);
  const [loading, setLoading] = React.useState(true);
  const [error, setError] = React.useState<unknown | null>(null);

  const [draftName, setDraftName] = React.useState("");
  const [draftScopes, setDraftScopes] = React.useState<string[]>([...DEFAULT_SCOPES]);
  const [creating, setCreating] = React.useState(false);
  const [minted, setMinted] = React.useState<CustomerPat | null>(null);

  const [revokeTarget, setRevokeTarget] = React.useState<CustomerPat | null>(null);
  const [rotateTarget, setRotateTarget] = React.useState<CustomerPat | null>(null);

  // Which view: hide historically-revoked tokens by default (prune the clutter).
  const [revokedFilter, setRevokedFilter] = React.useState<RevokedFilter>("active");
  // Tokens revoked in THIS session stay visible even in the "active" view, so the
  // user gets immediate confirmation that the row flipped to revoked. They are
  // tidied away on the next full load.
  const [sessionRevoked, setSessionRevoked] = React.useState<ReadonlySet<string>>(
    () => new Set<string>(),
  );

  const markSessionRevoked = React.useCallback((id: string) => {
    setSessionRevoked((prev) => {
      const next = new Set(prev);
      next.add(id);
      return next;
    });
  }, []);

  const reload = React.useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const r = await client.listKeys();
      setPats(r.pats);
      setByok(r.byok);
    } catch (e) {
      setError(e);
    } finally {
      setLoading(false);
    }
  }, [client]);

  React.useEffect(() => {
    void reload();
  }, [reload]);

  const toggleScope = React.useCallback((id: string, checked: boolean) => {
    setDraftScopes((prev) => (checked ? [...prev, id] : prev.filter((x) => x !== id)));
  }, []);

  async function onCreate(e: React.FormEvent): Promise<void> {
    e.preventDefault();
    if (creating) return;
    setCreating(true);
    setError(null);
    try {
      const r = await client.createPat({ name: draftName, scopes: draftScopes });
      // Stash the plaintext in the in-memory holder (the ONLY approved channel —
      // never React state, never storage) so PatModal can reveal + zero it.
      if (r.token) setPlaintextPat(r.token);
      // Never retain the plaintext token in React state (it would surface in the
      // fiber tree / devtools). The secret lives only in the shown-once holder
      // above; `minted` just drives the modal open-state + metadata.
      const { token: _discard, ...patOnly } = r;
      void _discard;
      setMinted(patOnly);
      setDraftName("");
      setDraftScopes([...DEFAULT_SCOPES]);
      toast({ title: `Token "${r.name}" created`, tone: "success" });
      await reload();
    } catch (ex) {
      setError(ex);
      toast({ title: "Couldn't create token", tone: "danger" });
    } finally {
      setCreating(false);
    }
  }

  async function onConfirmRevoke(): Promise<void> {
    const target = revokeTarget;
    if (!target) return;
    try {
      await client.revokePat(target.pat_id);
      markSessionRevoked(target.pat_id);
      toast({ title: `Token "${target.name}" revoked`, tone: "success" });
      await reload();
    } catch (ex) {
      setError(ex);
      toast({ title: "Couldn't revoke token", tone: "danger" });
    }
  }

  // Rotate = revoke the old secret + mint a fresh one with the SAME name + scopes,
  // composed from the existing client primitives (no dedicated rotate endpoint). The
  // new secret is revealed once through the shared PatModal, exactly like create.
  async function onConfirmRotate(): Promise<void> {
    const target = rotateTarget;
    if (!target) return;
    try {
      await client.revokePat(target.pat_id);
      const r = await client.createPat({ name: target.name, scopes: target.scopes });
      if (r.token) setPlaintextPat(r.token);
      markSessionRevoked(target.pat_id);
      // Never retain the plaintext token in React state (it would surface in the
      // fiber tree / devtools). The secret lives only in the shown-once holder
      // above; `minted` just drives the modal open-state + metadata.
      const { token: _discard, ...patOnly } = r;
      void _discard;
      setMinted(patOnly);
      toast({ title: `Token "${target.name}" rotated`, tone: "success" });
      await reload();
    } catch (ex) {
      setError(ex);
      toast({ title: "Couldn't rotate token", tone: "danger" });
    }
  }

  const activeCount = pats.filter((p) => !p.revoked_at).length;
  const hasRevoked = pats.some((p) => Boolean(p.revoked_at));
  const visiblePats = pats.filter(
    (p) => revokedFilter === "all" || !p.revoked_at || sessionRevoked.has(p.pat_id),
  );

  return (
    <div data-testid="keys-shell" className="lin-checklist">
      <Card
        title="Personal access tokens"
        meta="Tokens authenticate your build tools to CoreLink."
        actions={
          <HelpPopover label="What is a personal access token (PAT)?">
            A personal access token (PAT) is a long-lived secret a client (CI job,
            build tool, or CLI) sends instead of a password. Each token carries only
            the scopes you grant it and can be revoked at any time without affecting
            your other tokens.
          </HelpPopover>
        }
      >
        {/* ── Create ─────────────────────────────────────────────── */}
        <form data-testid="keys-create" onSubmit={onCreate}>
          <Field label="Token name" htmlFor="keys-create-name" help="A label to help you recognize this token later (e.g. 'ci-github' or 'laptop').">
            <Input
              id="keys-create-name"
              data-testid="keys-create-name"
              required
              placeholder="ci-github"
              value={draftName}
              onChange={(e) => setDraftName(e.target.value)}
            />
          </Field>

          <fieldset className="lin-mt">
            <legend className="lin-label">Scopes</legend>
            <div className="lin-checklist">
              {SCOPE_SPECS.map((s) => (
                <label key={s.id} className="lin-check">
                  <input
                    type="checkbox"
                    data-testid={`keys-scope-${s.id}`}
                    checked={draftScopes.includes(s.id)}
                    onChange={(e) => toggleScope(s.id, e.target.checked)}
                  />
                  <span className="lin-check__label">
                    <code>{s.id}</code> — {s.label}
                    {" "}
                    <HelpPopover label={`What does ${s.id} grant?`}>{s.help}</HelpPopover>
                    <span className="lin-card__meta">{s.desc}</span>
                  </span>
                </label>
              ))}
            </div>
          </fieldset>

          <div className="lin-mt">
            <Button
              type="submit"
              data-testid="keys-create-submit"
              loading={creating}
              disabled={draftScopes.length === 0}
            >
              Create token
            </Button>
          </div>
        </form>

        {/* ── List ───────────────────────────────────────────────── */}
        <div data-testid="keys-list" className="lin-checklist">
          {loading ? (
            <div data-testid="keys-loading" aria-busy="true" aria-label="Loading tokens">
              <Skeleton rows={3} />
            </div>
          ) : error != null ? (
            <InlineError error={error} onRetry={() => void reload()} />
          ) : pats.length === 0 ? (
            <EmptyState
              title="No tokens yet"
              body="Create one to connect your first build cache."
              cta={
                <Button
                  onClick={() => {
                    document.getElementById("keys-create-name")?.focus();
                  }}
                >
                  Create token
                </Button>
              }
            />
          ) : (
            <>
              {hasRevoked ? (
                <div data-testid="keys-revoked-filter" className="lin-card__head">
                  <span className="lin-label">Show</span>
                  <Segmented<RevokedFilter>
                    options={[
                      { value: "active", label: "Active" },
                      { value: "all", label: "All" },
                    ]}
                    value={revokedFilter}
                    onChange={setRevokedFilter}
                  />
                </div>
              ) : null}
              <table className="lin-table">
                <thead>
                  <tr>
                    <th>Name</th>
                    <th>Scopes</th>
                    <th>Created</th>
                    <th>
                      Last used{" "}
                      <HelpPopover label="What does last used mean?">
                        When this token was last presented to authenticate a request.
                        Last-used tracking isn&apos;t wired up yet, so this currently
                        shows &quot;—&quot; for every token.
                      </HelpPopover>
                    </th>
                    <th>Status</th>
                    <th aria-label="Actions" />
                  </tr>
                </thead>
                <tbody>
                  {visiblePats.map((p) => {
                    const revoked = Boolean(p.revoked_at);
                    return (
                      <tr key={p.pat_id} data-testid={`keys-row-${p.pat_id}`}>
                        <td>{p.name}</td>
                        <td>
                          {p.scopes.map((sc) => (
                            <Badge key={sc}>{sc}</Badge>
                          ))}
                        </td>
                        <td>{p.created_at.slice(0, 10)}</td>
                        {/* last_used_at is a BE-5 stub: render an honest em-dash, never
                            a misleading "Never" nor a fabricated timestamp. */}
                        <td>{p.last_used_at ? p.last_used_at.slice(0, 10) : "—"}</td>
                        <td data-testid={`keys-status-${p.pat_id}`}>
                          {revoked ? (
                            <Badge tone="danger" dot>
                              revoked
                            </Badge>
                          ) : (
                            <Badge tone="success" dot>
                              active
                            </Badge>
                          )}
                        </td>
                        <td>
                          {revoked ? null : (
                            <div className="lin-card__actions">
                              <Button
                                variant="ghost"
                                size="sm"
                                data-testid={`keys-rotate-${p.pat_id}`}
                                onClick={() => setRotateTarget(p)}
                              >
                                Rotate
                              </Button>
                              <Button
                                variant="danger"
                                size="sm"
                                data-testid={`keys-revoke-${p.pat_id}`}
                                onClick={() => setRevokeTarget(p)}
                              >
                                Revoke
                              </Button>
                            </div>
                          )}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </>
          )}
        </div>
      </Card>

      {/* ── BYOK status ────────────────────────────────────────── */}
      <Card
        title="BYOK — bring your own key"
        actions={
          <HelpPopover label="What is BYOK?">
            BYOK (bring-your-own-key) lets you encrypt your cached data with a key you
            control in your own KMS (a customer master key, or CMK), so CoreLink never
            holds the raw key. Status here is read-only; self-serve configuration is
            coming to Trust &amp; compliance.
          </HelpPopover>
        }
      >
        {byok ? (
          <p>
            Status:{" "}
            <strong data-testid="keys-byok-status">{byok.status}</strong>
            {byok.cmk_id ? (
              <>
                {" "}
                · CMK <code>{byok.cmk_id}</code>
              </>
            ) : null}
          </p>
        ) : (
          <EmptyState title="No customer-managed key" body="Your data is encrypted with CoreLink-managed keys." />
        )}
      </Card>

      {/* ── Minted-token reveal (shown once, via the shared PatModal) ── */}
      <div data-testid="keys-new-token">
        <PatModal
          open={minted != null}
          labels={PAT_MODAL_LABELS}
          onConfirm={() => setMinted(null)}
        />
      </div>

      {/* ── Revoke confirmation ────────────────────────────────── */}
      <ConfirmDialog
        open={revokeTarget != null}
        title="Revoke this token?"
        danger
        confirmLabel="Revoke token"
        body={
          revokeTarget ? (
            <Callout tone="danger">
              Revoke <strong>{revokeTarget.name}</strong>? Any client using it stops
              working immediately. This can&apos;t be undone — you&apos;ll need to create
              a new token to reconnect.
            </Callout>
          ) : null
        }
        onClose={() => setRevokeTarget(null)}
        onConfirm={onConfirmRevoke}
      />

      {/* ── Rotate confirmation ────────────────────────────────── */}
      <ConfirmDialog
        open={rotateTarget != null}
        title="Rotate this token?"
        confirmLabel="Rotate token"
        body={
          rotateTarget ? (
            <Callout tone="warn">
              Rotate <strong>{rotateTarget.name}</strong>? We&apos;ll revoke the current
              secret and issue a new one with the same name and the same scopes. The old
              secret stops working immediately — update wherever it&apos;s stored. The new
              secret is shown only once.
            </Callout>
          ) : null
        }
        onClose={() => setRotateTarget(null)}
        onConfirm={onConfirmRotate}
      />

      {activeCount === 0 && pats.length > 0 && !loading && error == null ? (
        <Callout tone="info">
          All your tokens are revoked. Create a new one to reconnect a build cache.
        </Callout>
      ) : null}
    </div>
  );
}

/** Public entry — wraps the screen in a local ToastProvider so `useToast` works
 *  even when this client is mounted bare (the customer layout may also provide
 *  one; nested providers are harmless — the innermost wins). */
export function KeysClient(): React.ReactElement {
  return (
    <ToastProvider>
      <KeysInner />
    </ToastProvider>
  );
}

export default KeysClient;
