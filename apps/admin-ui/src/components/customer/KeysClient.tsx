// Customer-side Tokens (PATs) — Linear-styled list + create + revoke + BYOK status.
//
// W3 (customer-dashboard build wave). Kit-only: every surface is a `@/components/ui/linear`
// primitive; the minted PAT is revealed through the shared `PatModal` (copy + shown-once +
// confirm), never dumped as plain text; revoke is gated behind a `ConfirmDialog`; every
// mutation fires a Toast. Data-truth: PAT CRUD is [live]; `last_used_at` is [stub] (BE-5 will
// stamp it on auth) so absent values render an honest "Never", never a fabricated date.

"use client";

import React from "react";
import { useAuth } from "@clerk/nextjs";
import { CustomerClient } from "@/lib/customer-client";
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
    help: "The REAPI find-missing-blobs probe: a client asks 'which of these do you already have?' so it only uploads what's missing. Build tools (Bazel, sccache) use this to avoid re-uploading known objects.",
  },
  {
    id: "admin:audit",
    label: "Read audit log",
    desc: "Read this tenant's audit events. No cache access.",
    help: "Read-only access to your tenant's audit trail (who did what, when). Grant this to compliance/monitoring integrations. It does NOT grant any cache read or write.",
  },
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

function KeysInner(): React.ReactElement {
  const { getToken } = useAuth();
  const client = React.useMemo(() => new CustomerClient({ getToken }), [getToken]);
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
      setMinted(r);
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
      toast({ title: `Token "${target.name}" revoked`, tone: "success" });
      await reload();
    } catch (ex) {
      setError(ex);
      toast({ title: "Couldn't revoke token", tone: "danger" });
    }
  }

  const activeCount = pats.filter((p) => !p.revoked_at).length;

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

          <fieldset>
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

          <Button
            type="submit"
            data-testid="keys-create-submit"
            loading={creating}
            disabled={draftScopes.length === 0}
          >
            Create token
          </Button>
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
                      &quot;Never&quot; means it hasn&apos;t been used yet.
                    </HelpPopover>
                  </th>
                  <th>Status</th>
                  <th aria-label="Actions" />
                </tr>
              </thead>
              <tbody>
                {pats.map((p) => {
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
                      <td>{p.last_used_at ? p.last_used_at.slice(0, 10) : "Never"}</td>
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
                          <Button
                            variant="danger"
                            size="sm"
                            data-testid={`keys-revoke-${p.pat_id}`}
                            onClick={() => setRevokeTarget(p)}
                          >
                            Revoke
                          </Button>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
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
