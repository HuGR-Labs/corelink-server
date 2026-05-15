// Customer-side keys management — PAT list + create + revoke + BYOK status.

"use client";

import React from "react";
import { CustomerClient } from "@/lib/customer-client";
import type { CustomerOverview, CustomerPat } from "@/lib/customer-types";

const client = new CustomerClient();

const SCOPE_OPTIONS = ["cache:r", "cache:w", "cache:find-missing", "admin:audit"] as const;

export function KeysClient(): React.ReactElement {
  const [pats, setPats] = React.useState<CustomerPat[]>([]);
  const [byok, setByok] = React.useState<CustomerOverview["byok"] | null>(null);
  const [loading, setLoading] = React.useState(true);
  const [err, setErr] = React.useState<string | null>(null);
  const [draftName, setDraftName] = React.useState("");
  const [draftScopes, setDraftScopes] = React.useState<string[]>(["cache:r"]);
  const [newToken, setNewToken] = React.useState<string | null>(null);

  const reload = React.useCallback(async () => {
    setLoading(true);
    try {
      const r = await client.listKeys();
      setPats(r.pats);
      setByok(r.byok);
    } catch (e) {
      setErr(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  React.useEffect(() => {
    void reload();
  }, [reload]);

  async function onCreate(e: React.FormEvent): Promise<void> {
    e.preventDefault();
    setErr(null);
    try {
      const r = await client.createPat({ name: draftName, scopes: draftScopes });
      setNewToken(r.token ?? null);
      setDraftName("");
      await reload();
    } catch (ex) {
      setErr(String(ex));
    }
  }

  async function onRevoke(id: string): Promise<void> {
    setErr(null);
    try {
      await client.revokePat(id);
      await reload();
    } catch (ex) {
      setErr(String(ex));
    }
  }

  return (
    <div data-testid="keys-shell">
      <section data-testid="keys-byok">
        <h2>BYOK (customer-managed key)</h2>
        {byok ? (
          <p>
            status: <strong data-testid="keys-byok-status">{byok.status}</strong>
            {byok.cmk_id ? (
              <>
                {" "}
                · CMK <code>{byok.cmk_id}</code>
              </>
            ) : null}
          </p>
        ) : (
          <p>—</p>
        )}
      </section>

      <section data-testid="keys-create">
        <h2>Create token</h2>
        <form onSubmit={onCreate}>
          <label>
            Name:{" "}
            <input
              type="text"
              data-testid="keys-create-name"
              required
              value={draftName}
              onChange={(e) => setDraftName(e.target.value)}
            />
          </label>
          <fieldset>
            <legend>Scopes</legend>
            {SCOPE_OPTIONS.map((s) => (
              <label key={s}>
                <input
                  type="checkbox"
                  data-testid={`keys-scope-${s}`}
                  checked={draftScopes.includes(s)}
                  onChange={(e) => {
                    setDraftScopes((prev) =>
                      e.target.checked ? [...prev, s] : prev.filter((x) => x !== s),
                    );
                  }}
                />
                {s}
              </label>
            ))}
          </fieldset>
          <button type="submit" data-testid="keys-create-submit">
            Create
          </button>
        </form>
        {newToken ? (
          <p data-testid="keys-new-token">
            New token (shown once): <code>{newToken}</code>
          </p>
        ) : null}
      </section>

      <section data-testid="keys-list">
        <h2>Tokens</h2>
        {loading ? <p data-testid="keys-loading">loading…</p> : null}
        {err ? <p data-testid="keys-error">{err}</p> : null}
        <table>
          <thead>
            <tr>
              <th>Name</th>
              <th>Scopes</th>
              <th>Created</th>
              <th>Last used</th>
              <th>Status</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {pats.map((p) => (
              <tr key={p.pat_id} data-testid={`keys-row-${p.pat_id}`}>
                <td>{p.name}</td>
                <td>
                  <code>{p.scopes.join(", ")}</code>
                </td>
                <td>{p.created_at.slice(0, 10)}</td>
                <td>{p.last_used_at?.slice(0, 10) ?? "—"}</td>
                <td data-testid={`keys-status-${p.pat_id}`}>
                  {p.revoked_at ? "revoked" : "active"}
                </td>
                <td>
                  {p.revoked_at ? null : (
                    <button
                      type="button"
                      data-testid={`keys-revoke-${p.pat_id}`}
                      onClick={() => void onRevoke(p.pat_id)}
                    >
                      Revoke
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>
    </div>
  );
}

export default KeysClient;
