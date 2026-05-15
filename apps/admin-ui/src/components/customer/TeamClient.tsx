// Customer-side team management — member list + invite.

"use client";

import React from "react";
import { CustomerClient } from "@/lib/customer-client";
import type { CustomerTeamMember } from "@/lib/customer-types";

const client = new CustomerClient();

const ROLES: Array<CustomerTeamMember["role"]> = ["Owner", "Admin", "Developer", "Viewer"];

export function TeamClient(): React.ReactElement {
  const [members, setMembers] = React.useState<CustomerTeamMember[]>([]);
  const [email, setEmail] = React.useState("");
  const [role, setRole] = React.useState<CustomerTeamMember["role"]>("Developer");
  const [loading, setLoading] = React.useState(true);
  const [err, setErr] = React.useState<string | null>(null);
  const [lastInvited, setLastInvited] = React.useState<string | null>(null);

  const reload = React.useCallback(async () => {
    setLoading(true);
    try {
      const r = await client.listTeam();
      setMembers(r.members);
    } catch (e) {
      setErr(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  React.useEffect(() => {
    void reload();
  }, [reload]);

  async function onInvite(e: React.FormEvent): Promise<void> {
    e.preventDefault();
    setErr(null);
    try {
      const m = await client.inviteTeam({ email, role });
      setLastInvited(m.email);
      setEmail("");
      await reload();
    } catch (ex) {
      setErr(String(ex));
    }
  }

  return (
    <div data-testid="team-shell">
      <section data-testid="team-invite">
        <h2>Invite teammate</h2>
        <form onSubmit={onInvite}>
          <label>
            Email:{" "}
            <input
              type="email"
              data-testid="team-invite-email"
              required
              value={email}
              onChange={(e) => setEmail(e.target.value)}
            />
          </label>
          <label>
            Role:{" "}
            <select
              data-testid="team-invite-role"
              value={role}
              onChange={(e) => setRole(e.target.value as CustomerTeamMember["role"])}
            >
              {ROLES.map((r) => (
                <option key={r} value={r}>
                  {r}
                </option>
              ))}
            </select>
          </label>
          <button type="submit" data-testid="team-invite-submit">
            Invite
          </button>
        </form>
        {lastInvited ? (
          <p data-testid="team-invite-success">Invited {lastInvited}.</p>
        ) : null}
        {err ? <p data-testid="team-error">{err}</p> : null}
      </section>

      <section data-testid="team-list">
        <h2>Members</h2>
        {loading ? <p data-testid="team-loading">loading…</p> : null}
        <table>
          <thead>
            <tr>
              <th>Email</th>
              <th>Role</th>
              <th>Status</th>
              <th>Joined</th>
            </tr>
          </thead>
          <tbody>
            {members.map((m) => (
              <tr key={m.user_id} data-testid={`team-row-${m.user_id}`}>
                <td>{m.email}</td>
                <td data-testid={`team-role-${m.user_id}`}>{m.role}</td>
                <td>{m.status}</td>
                <td>{m.joined_at.slice(0, 10)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>
    </div>
  );
}

export default TeamClient;
